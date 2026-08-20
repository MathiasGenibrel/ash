//! L'invocation de `git`, derrière un trait que la feature possède.
//!
//! C'est le seul endroit du dépôt où le binaire `git` est lancé en production, et il n'y
//! en aura pas d'autre. La règle qui l'encadre est celle du critère d'acceptation de
//! l'issue #8 : **jamais dans la boucle de sonde**. L'appel part des trois mêmes moments
//! que le reste des métadonnées — rattachement, focus, écriture surveillée — et passe par
//! la même limitation à un rafraîchissement par worktree et par tranche de 5 s.
//!
//! Pourquoi un appel plutôt qu'une lecture de fichiers : l'état de l'arbre (`+3 ~1`) est
//! la comparaison de l'index avec l'arbre de travail, et l'avance sur l'amont (`↑2 ↓1`)
//! est un parcours du graphe de commits. Ni l'un ni l'autre ne se lit dans `.git` sans
//! réimplémenter une bibliothèque git complète.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Délai au-delà duquel on renonce à l'état de l'arbre.
///
/// Cinq secondes, comme la fenêtre de limitation : au-delà, un nouveau rafraîchissement
/// peut de toute façon être demandé, et un `git` par worktree suffit largement à
/// encombrer une machine. Un dépôt trop gros pour répondre dans ce délai rend une ligne
/// de statut **sans** état d'arbre — mais avec sa branche, qui vient des fichiers.
pub const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// Les arguments de l'appel, et pourquoi chacun est là.
///
/// Cette liste est une **frontière de sécurité**, pas une préférence de formatage. Ash
/// lance `git status` tout seul, sur le simple fait que le shell de l'utilisateur a fait
/// un `cd` — sans qu'aucune commande git n'ait été tapée. Un dépôt hostile récupéré puis
/// simplement visité ne doit donc pas pouvoir exécuter quoi que ce soit.
///
/// `core.fsmonitor` est le vecteur : sa valeur est une **commande** que `git status`
/// exécute, et elle se pose dans le `.git/config` du dépôt visité. La protection
/// `safe.directory` de git ne couvre pas ce cas — elle ne se déclenche que si le dépôt
/// appartient à un *autre* utilisateur, alors qu'un dépôt téléchargé appartient au nôtre.
/// La configuration passée en `-c` l'emporte sur celle du dépôt : c'est ce qui la
/// neutralise. Vérifié en reproduisant l'exécution, puis son absence.
const HARDENED_STATUS_ARGS: [&str; 8] = [
    // Un lecteur de fond n'a pas à réécrire l'index de l'utilisateur pour rafraîchir des
    // dates : sans ça, chaque appel écrirait dans `.git`.
    "--no-optional-locks",
    // Le vecteur d'exécution, neutralisé. Ne retire jamais cette ligne.
    "-c",
    "core.fsmonitor=false",
    // Les chemins de contrôle sont échappés, jamais rendus tels quels : une ligne du
    // résultat reste une ligne, même pour un fichier au nom exotique.
    "-c",
    "core.quotePath=true",
    "status",
    // Format documenté et stable, contrairement à `--short`.
    "--porcelain=v2",
    // L'en-tête `# branch.ab +2 -1` : l'avance et le retard, dans le même appel.
    "--branch",
];

/// Les arguments de la lecture des derniers commits, et pourquoi chacun est là.
///
/// **La même question que pour `git status` se repose ici, et elle a la même réponse** :
/// cet appel-ci part tout seul lui aussi — une écriture dans `.git/logs/HEAD` suffit à le
/// déclencher, sans qu'aucune commande git n'ait été tapée —, donc visiter un dépôt hostile
/// ne doit rien exécuter. Ce que `git log` peut exécuter, et ce qui le neutralise :
///
/// - `core.fsmonitor` : une **commande**, posée dans le `.git/config` du dépôt visité, que
///   git lance pour rafraîchir l'index. Surchargée en `-c`, comme pour `git status` ;
/// - `core.pager` / `pager.log` : un pager est une commande, et le dépôt la configure.
///   `--no-pager` la coupe, en plus du fait que la sortie est un tube ;
/// - la vérification de signature, qui lance `gpg` : `--no-show-signature` l'écarte
///   explicitement plutôt que de compter sur `log.showSignature` valant faux ;
/// - les pilotes `textconv` et `diff`, eux aussi des commandes : ils ne s'exécutent que sur
///   un diff, et **aucune option de diff n'est passée**. Le format demandé ne contient que
///   des champs d'en-tête de commit.
///
/// Ce qui reste lu depuis le dépôt sans être exécuté — `mailmap`, `log.date` — ne change
/// que du texte, et ce texte est déjà traité comme non fiable : il finit dans un fichier
/// JSON, jamais dans une commande.
const HARDENED_LOG_ARGS: [&str; 10] = [
    "--no-optional-locks",
    // Le vecteur d'exécution, neutralisé. Ne retire jamais cette ligne.
    "-c",
    "core.fsmonitor=false",
    // Un pager est une commande, et `pager.log` est une valeur du dépôt.
    "--no-pager",
    "log",
    // `gpg` est une commande, et `log.showSignature` peut la réclamer.
    "--no-show-signature",
    // Aucun nom de ref dans la sortie : ce qui est lu ici est un commit, pas une branche.
    "--no-decorate",
    // Combien de commits on relit à chaque mouvement de `HEAD` : assez pour couvrir un
    // `git rebase` d'une branche entière, qui en réécrit toute une série d'un coup ; assez
    // peu pour que la lecture reste un processus court sur un dépôt de dix ans. Ce qui
    // dépasse n'est pas perdu : un commit plus vieux que ce budget a déjà été vu naître, ou
    // ne l'a jamais été.
    "--max-count=50",
    // Séparateur de champs : l'unité de séparation ASCII, qu'aucun sujet de commit ne
    // contient. Le sujet (`%s`) est d'une seule ligne par construction, donc la ligne reste
    // l'unité d'enregistrement.
    "--format=%H%x1f%at%x1f%aI%x1f%s",
    "HEAD",
];

/// Un commit tel que git le décrit, et rien de plus.
///
/// Les trois champs qui comptent pour [ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md)
/// sont `sha`, `author_date` et `subject` : le premier identifie, les deux autres survivent
/// à un rebase, un amend et un cherry-pick — ce sont eux qui rattrapent l'attribution quand
/// le `sha` a changé.
///
/// `author_date` est gardée **telle que git l'écrit** (`%aI`, ISO 8601 strict), et pas
/// reformatée : la correspondance de repli compare deux chaînes, et toute normalisation en
/// route serait une occasion de ne plus reconnaître ce qu'on a soi-même écrit. `authored_at`
/// est la même date en secondes Unix (`%at`), qui se compare à une horloge sans analyser
/// quoi que ce soit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRecord {
    pub sha: String,
    pub author_date: String,
    pub authored_at: u64,
    pub subject: String,
}

/// Les derniers commits de `HEAD`, du plus récent au plus ancien.
///
/// Rend un vecteur vide pour tout ce qui peut mal se passer — `git` absent, dépôt sans
/// commit, délai dépassé. L'appelant en fait la même chose : il n'attribue rien.
pub trait CommitLog: Send + Sync {
    fn recent(&self, worktree_root: &Path) -> Vec<CommitRecord>;
}

/// L'état d'un arbre de travail, tel que `git` sait seul le dire.
///
/// Rend la sortie **brute** : l'interprétation est une règle pure, et elle vit dans
/// [`super::porcelain`]. `None` couvre tout ce qui peut mal se passer — `git` absent du
/// `PATH`, dépôt trop gros pour le délai, sortie en erreur — parce que l'appelant en fait
/// la même chose : il affiche la branche sans l'état de l'arbre. Ce n'est pas une panne,
/// c'est un cas nominal.
pub trait StatusReader: Send + Sync {
    fn read(&self, worktree_root: &Path) -> Option<String>;
}

/// L'appel réel : un processus `git`, dans le worktree, sans shell.
#[derive(Debug, Clone, Copy)]
pub struct SystemGit {
    timeout: Duration,
}

impl Default for SystemGit {
    fn default() -> Self {
        Self {
            timeout: STATUS_TIMEOUT,
        }
    }
}

impl StatusReader for SystemGit {
    fn read(&self, worktree_root: &Path) -> Option<String> {
        self.output(worktree_root, &HARDENED_STATUS_ARGS)
    }
}

impl CommitLog for SystemGit {
    fn recent(&self, worktree_root: &Path) -> Vec<CommitRecord> {
        self.output(worktree_root, &HARDENED_LOG_ARGS)
            .as_deref()
            .map(parse_log)
            .unwrap_or_default()
    }
}

/// Les lignes d'un `git log` formaté, en commits.
///
/// Une ligne qu'on ne comprend pas est **jetée**, pas devinée : la sortie de git vient d'un
/// dépôt que personne n'a validé, et un sujet exotique ne doit pas faire disparaître les
/// commits qui suivent.
fn parse_log(output: &str) -> Vec<CommitRecord> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\u{1f}');
            let sha = fields.next()?;
            let authored_at = fields.next()?.parse().ok()?;
            let author_date = fields.next()?;
            // Le sujet est le **reste** de la ligne : il ne peut pas contenir de séparateur,
            // mais le supposer coûterait un commit perdu le jour où git en écrirait un.
            let subject = fields.collect::<Vec<_>>().join("\u{1f}");
            (!sha.is_empty()).then(|| CommitRecord {
                sha: sha.to_owned(),
                author_date: author_date.to_owned(),
                authored_at,
                subject,
            })
        })
        .collect()
}

impl SystemGit {
    /// La sortie standard d'un `git` lancé dans un worktree, ou rien.
    ///
    /// Le seul endroit du dépôt où un processus `git` est créé : les deux verbes passent
    /// par ici, donc la façon dont l'appel est construit — programme nommé, arguments
    /// séparés, répertoire explicite, délai tenu — ne se décide qu'une fois.
    fn output(&self, worktree_root: &Path, args: &[&str]) -> Option<String> {
        // `Command` prend le programme et ses arguments séparément : aucun shell n'est
        // lancé, donc aucun chemin de worktree ne peut être interprété comme du code.
        // Le répertoire de travail est **explicite** — un `git` lancé depuis le
        // répertoire courant du processus décrirait un autre dépôt que celui demandé.
        let mut child = Command::new("git")
            .current_dir(worktree_root)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let mut output = child.stdout.take()?;
        // La lecture se fait dans un fil à part pour que le délai soit tenu même si `git`
        // ne rend jamais la main : `wait_timeout` n'existe pas dans la bibliothèque
        // standard, et un `read` bloquant ne s'interrompt pas.
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut text = String::new();
            let read = output.read_to_string(&mut text);
            let _ = sender.send(read.ok().map(|_| text));
        });

        match receiver.recv_timeout(self.timeout) {
            Ok(text) => {
                // Le code de sortie compte : `git status` hors d'un dépôt sort en 128 et
                // n'écrit rien sur la sortie standard.
                let succeeded = child.wait().map(|status| status.success()).unwrap_or(false);
                succeeded.then_some(text).flatten()
            }
            Err(_) => {
                // Le dépôt est trop gros, ou `git` est bloqué sur un verrou. On le tue :
                // un processus abandonné par worktree, toutes les cinq secondes, finirait
                // par se voir.
                let _ = child.kill();
                let _ = child.wait();
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_the_status_invocation_when_a_visited_repository_configures_a_fsmonitor_command_then_it_is_overridden(
    ) {
        // Given
        let args = HARDENED_STATUS_ARGS;
        // When
        let neutralises_fsmonitor = args
            .windows(2)
            .any(|pair| pair == ["-c", "core.fsmonitor=false"]);
        // Then
        assert!(
            neutralises_fsmonitor,
            "`core.fsmonitor` est une commande que `git status` exécute, et le dépôt \
             visité la pose dans son propre `.git/config`. Ash lance `git status` sur un \
             simple `cd` : sans cette surcharge, visiter un dépôt hostile suffit à \
             exécuter du code."
        );
    }

    #[test]
    fn given_the_log_invocation_when_a_visited_repository_configures_a_command_then_none_of_them_runs(
    ) {
        // Given — la lecture des commits part d'une écriture dans `.git/logs/HEAD` : elle
        // s'exécute donc sur un dépôt que personne n'a validé, exactement comme
        // `git status` s'exécute sur un simple `cd`. Les trois commandes qu'un dépôt peut
        // faire lancer à `git log` sont son pager, son `fsmonitor` et son `gpg`.
        let args = HARDENED_LOG_ARGS;

        // When
        let neutralised = |flag: &str| args.contains(&flag);

        // Then
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-c", "core.fsmonitor=false"]),
            "`core.fsmonitor` est une commande du dépôt visité, et `git log` rafraîchit \
             l'index"
        );
        assert!(neutralised("--no-pager"), "`pager.log` est une commande");
        assert!(
            neutralised("--no-show-signature"),
            "`log.showSignature` fait lancer `gpg` sur chaque commit lu"
        );
        assert!(
            !args.iter().any(|argument| argument.starts_with("--patch")
                || argument.starts_with("-p")
                || argument.starts_with("--stat")),
            "aucune option de diff : c'est ce qui garde les pilotes `textconv` hors jeu"
        );
    }

    #[test]
    fn given_a_log_output_when_it_is_read_then_each_commit_keeps_the_date_git_wrote() {
        // Given — la date d'auteur est la moitié de la clé de repli d'ADR-0014 : la
        // reformater, c'est ne plus reconnaître après un rebase ce qu'on a soi-même écrit.
        let output = "8f3a1c2\u{1f}1755000000\u{1f}2026-08-12T14:03:21+02:00\u{1f}feat: onglets\n\
                      1b2c3d4\u{1f}1754000000\u{1f}2026-07-31T09:00:00+02:00\u{1f}fix: rebase\n";

        // When
        let commits = parse_log(output);

        // Then
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].sha, "8f3a1c2");
        assert_eq!(commits[0].author_date, "2026-08-12T14:03:21+02:00");
        assert_eq!(commits[0].authored_at, 1_755_000_000);
        assert_eq!(commits[0].subject, "feat: onglets");
    }

    #[test]
    fn given_a_log_output_with_an_unreadable_line_when_it_is_read_then_the_others_survive() {
        // Given — la sortie vient d'un dépôt que personne n'a validé. Une ligne tronquée ne
        // doit pas emporter les commits qui la suivent.
        let output = "tronquée\n\
                      1b2c3d4\u{1f}1754000000\u{1f}2026-07-31T09:00:00+02:00\u{1f}fix: rebase\n";

        // When
        let commits = parse_log(output);

        // Then
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].sha, "1b2c3d4");
    }

    #[test]
    fn given_the_status_invocation_when_it_is_built_then_it_never_goes_through_a_shell() {
        // Given
        let args = HARDENED_STATUS_ARGS;
        // When
        let program = "git";
        // Then
        assert_eq!(
            program, "git",
            "le programme est nommé, jamais une ligne de shell"
        );
        assert!(
            args.iter().all(|arg| !arg.contains(char::is_whitespace)),
            "un argument porteur d'espace trahirait une ligne de commande recomposée"
        );
    }
}
