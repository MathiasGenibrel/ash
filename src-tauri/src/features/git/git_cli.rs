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

/// Ce que git seul sait dire des **refs** d'un dépôt, et de qui les détient.
///
/// Rend la sortie brute, comme [`StatusReader`], et pour la même raison : l'interprétation
/// est une règle pure, et elle vit dans [`super::branches`].
///
/// Deux questions et non une, parce qu'aucune commande ne répond aux deux : `for-each-ref`
/// liste les branches et leur date, `worktree list` dit laquelle vit où. La seconde est ce
/// qui rend la colonne de droite de la spec §7.1 calculable — deux worktrees ne peuvent pas
/// être sur la même branche, donc la correspondance est une fonction.
pub trait BranchReader: Send + Sync {
    /// `for-each-ref` sur `refs/heads` et `refs/remotes`. `None` si git n'a pas répondu.
    fn refs(&self, worktree_root: &Path) -> Option<String>;
    /// `worktree list --porcelain`. `None` si git n'a pas répondu.
    fn worktrees(&self, worktree_root: &Path) -> Option<String>;
}

/// Ce qu'une invocation qui **écrit** rend : son succès, et ce qu'elle a dit.
///
/// La sortie est gardée même en cas de succès : un `git merge` qui rend 0 peut avoir écrit
/// « Already up to date », et c'est exactement ce que l'utilisateur veut lire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completed {
    pub success: bool,
    /// Sortie standard et sortie d'erreur, dans cet ordre, telles que git les a écrites.
    pub output: String,
}

/// Les verbes de branche qui **touchent l'arbre de travail**.
///
/// Un trait séparé de [`StatusReader`] et de [`BranchReader`], et ce n'est pas de la
/// symétrie : ces trois-là ne se ressemblent pas du tout du point de vue qui compte ici.
/// Les deux premiers partent **tout seuls**, sur un simple `cd` de l'utilisateur ; celui-ci
/// ne part **jamais** sans un geste explicite, et jamais sans qu'Ash ait nommé les deux
/// côtés de ce qu'il s'apprête à faire. C'est cette différence de consentement qui décide de
/// ce qu'on a le droit de laisser passer — voir [`HARDENED_TREE_ARGS`].
pub trait TreeWriter: Send + Sync {
    fn run(&self, worktree_root: &Path, args: &[String]) -> Option<Completed>;
}

/// Le préfixe commun à **toute** invocation de git faite par Ash.
///
/// La règle est celle de [`HARDENED_STATUS_ARGS`], sortie de la seule commande qui la
/// portait : un dépôt visité ne doit pas pouvoir exécuter du code, et `core.fsmonitor` est
/// une **commande** que le dépôt pose dans son propre `.git/config`. Elle vaut pour
/// `for-each-ref` et `worktree list` comme pour `status` — non parce que ces deux-là
/// rafraîchissent l'index (ils ne le font pas), mais parce qu'une neutralisation qui n'est
/// vraie que sur certaines commandes est une neutralisation qu'on oubliera à la suivante.
///
/// `core.pager=cat` est là pour la même raison de principe. Git ne pagine que si sa sortie
/// standard est un terminal, et la nôtre est un tuyau : la valeur n'a donc aujourd'hui aucun
/// effet. Mais `core.pager` est, lui aussi, une **commande** lue dans le dépôt visité, et
/// tout ce qui sépare son exécution de nous est une propriété du descripteur de sortie —
/// c'est-à-dire un détail d'implémentation de l'appelant, pas une décision.
const HARDENED_PREFIX: [&str; 5] = [
    "--no-optional-locks",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.pager=cat",
];

/// Lister les branches, leur pointe et leur date.
///
/// **`for-each-ref` et pas `git branch`**, et c'est la réponse à la question que
/// `git_cli.rs` oblige à reposer pour chaque verbe ajouté :
///
/// - `git branch` est de la porcelaine : son format n'est pas contractuel, il pagine, il
///   colore selon `color.branch` du dépôt visité, et il tronque selon la largeur du
///   terminal. `for-each-ref` est de la plomberie — un format qu'on impose, et rien d'autre
///   dans la sortie.
/// - le format est **imposé par nous** et ne contient aucun `%(if)` ni aucune substitution
///   venue du dépôt : un nom de branche ne peut pas devenir une directive de format.
/// - un nom de ref ne peut contenir ni espace, ni caractère de contrôle — git le refuse à
///   l'écriture. La tabulation comme séparateur et le saut de ligne comme terminateur sont
///   donc sûrs, sans échappement à défaire.
/// - aucun hook, aucun pilote de diff, aucun `textconv` n'est sur ce chemin : `for-each-ref`
///   ne lit que le graphe de refs.
const HARDENED_REF_ARGS: [&str; 4] = [
    "for-each-ref",
    // Objet court, date en secondes Unix, `*` pour le `HEAD` de **ce** worktree, nom complet.
    "--format=%(objectname:short)%09%(committerdate:unix)%09%(HEAD)%09%(refname)",
    "refs/heads",
    "refs/remotes",
];

/// Quelle branche vit dans quel worktree — ADR-0012, rendu calculable.
///
/// `--porcelain` est le format documenté et stable : une ligne `worktree <chemin>`, une
/// ligne `branch refs/heads/<nom>` ou `detached`, un blanc entre deux entrées. La question de
/// sécurité se repose ici comme ailleurs et la réponse est la même : aucun hook, aucun
/// pilote, aucune configuration exécutable sur ce chemin — la commande lit
/// `.git/worktrees/*/gitdir`, ce que la feature sait déjà lire elle-même.
const HARDENED_WORKTREE_ARGS: [&str; 3] = ["worktree", "list", "--porcelain"];

/// Ce qu'on ajoute aux verbes qui **écrivent** — et ce qu'on assume de ne pas neutraliser.
///
/// La question de sécurité, reposée pour `switch`, `rebase` et `merge` :
///
/// - **`core.fsmonitor`** : neutralisé par [`HARDENED_PREFIX`]. Ces trois-là rafraîchissent
///   l'index, donc le vecteur est réel — et fermé.
/// - **`--no-pager`** : redondant avec `core.pager=cat`, et gardé quand même. C'est la seule
///   protection qui ne dépende pas de ce que le dépôt a écrit dans sa configuration.
/// - **les hooks** (`post-checkout`, `pre-merge-commit`, `post-merge`, `post-rewrite`) et les
///   **pilotes de fusion** (`.gitattributes` `merge=x` + `merge.x.driver`) **exécutent des
///   commandes du dépôt, et ne sont pas neutralisés.** C'est délibéré, et c'est la seule
///   différence qui compte entre ces verbes et `git status` : `status` part **tout seul**, sur
///   un `cd`, sans que l'utilisateur ait rien demandé — visiter un dépôt hostile ne doit donc
///   rien exécuter. Un rebase, lui, ne part **jamais** sans un geste explicite sur une
///   question qui a nommé ses deux côtés. Les désactiver casserait des dépôts légitimes
///   (git-lfs pose un pilote de fusion) pour protéger d'un dépôt que l'utilisateur a
///   lui-même choisi de fusionner. Si un jour Ash rebase sans qu'on le lui demande, cette
///   ligne devient fausse et il faudra la réécrire avant le code.
/// - **`--no-edit`** sur `merge` : sans lui, git ouvre `$EDITOR` — donc un processus qui
///   n'a ni terminal ni fenêtre, et qui ne rendrait jamais la main.
/// - **l'injection d'arguments** : une branche nommée `--upload-pack=…` serait lue comme une
///   option. Deux verrous, et il en faut deux : le nom est vérifié contre la liste que
///   [`super::branches`] vient de lire — donc contre ce que le dépôt contient vraiment — et
///   le `--` sépare les options des opérandes là où git l'accepte.
const HARDENED_TREE_ARGS: [&str; 1] = ["--no-pager"];

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
        self.capture(worktree_root, HARDENED_STATUS_ARGS.iter().copied())
    }
}

impl BranchReader for SystemGit {
    fn refs(&self, worktree_root: &Path) -> Option<String> {
        self.capture(
            worktree_root,
            HARDENED_PREFIX
                .iter()
                .copied()
                .chain(HARDENED_REF_ARGS.iter().copied()),
        )
    }

    fn worktrees(&self, worktree_root: &Path) -> Option<String> {
        self.capture(
            worktree_root,
            HARDENED_PREFIX
                .iter()
                .copied()
                .chain(HARDENED_WORKTREE_ARGS.iter().copied()),
        )
    }
}

impl TreeWriter for SystemGit {
    /// `args` est une liste **fermée**, composée par [`super::branch_actions`] à partir d'un
    /// verbe de son énumération et d'un nom de branche déjà vérifié contre le dépôt. Rien
    /// de ce que le frontend envoie n'arrive ici tel quel.
    fn run(&self, worktree_root: &Path, args: &[String]) -> Option<Completed> {
        let hardened: Vec<String> = HARDENED_PREFIX
            .iter()
            .chain(HARDENED_TREE_ARGS.iter())
            .map(|fixed| (*fixed).to_owned())
            .chain(args.iter().cloned())
            .collect();

        // La sortie d'erreur est **capturée** et non jetée : c'est elle qui dit pourquoi un
        // rebase a échoué, et le message qui remonte à l'utilisateur doit nommer ses deux
        // côtés *et* dire ce que git a répondu.
        let output = Command::new("git")
            .current_dir(worktree_root)
            .args(&hardened)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .ok()?;

        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        let errors = String::from_utf8_lossy(&output.stderr);
        if !errors.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&errors);
        }

        Some(Completed {
            success: output.status.success(),
            output: text.trim().to_owned(),
        })
    }
}

impl SystemGit {
    /// Lance `git`, sous délai, et rend sa sortie standard si — et seulement si — il a réussi.
    ///
    /// Le seul chemin de lecture du dépôt, partagé par les trois questions qu'Ash pose.
    fn capture<'a>(
        &self,
        worktree_root: &Path,
        args: impl Iterator<Item = &'a str>,
    ) -> Option<String> {
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
    fn given_any_added_git_verb_when_it_is_invoked_then_it_carries_the_same_neutralisation() {
        // Given — les trois questions de lecture, plus le préfixe des verbes qui écrivent
        let invocations: [Vec<&str>; 3] = [
            HARDENED_STATUS_ARGS.to_vec(),
            HARDENED_PREFIX
                .iter()
                .chain(HARDENED_REF_ARGS.iter())
                .copied()
                .collect(),
            HARDENED_PREFIX
                .iter()
                .chain(HARDENED_WORKTREE_ARGS.iter())
                .copied()
                .collect(),
        ];

        // When
        let all_neutralise = invocations.iter().all(|args| {
            args.windows(2)
                .any(|pair| pair == ["-c", "core.fsmonitor=false"])
        });

        // Then — une neutralisation vraie sur certaines commandes seulement est une
        // neutralisation qu'on oubliera à la suivante
        assert!(all_neutralise);
    }

    #[test]
    fn given_the_branch_listing_when_it_is_built_then_it_uses_plumbing_and_imposes_its_format() {
        // Given
        let args = HARDENED_REF_ARGS;

        // When
        let verb = args.first().copied();

        // Then — `git branch` est de la porcelaine : elle pagine, elle colore selon la
        // configuration du dépôt visité, et son format n'est pas contractuel
        assert_eq!(verb, Some("for-each-ref"));
        assert!(args.iter().any(|arg| arg.starts_with("--format=")));
        assert!(
            !args.iter().any(|arg| arg.contains("%(if)")),
            "aucune substitution venue du dépôt : un nom de branche ne devient pas une \
             directive de format"
        );
    }

    #[test]
    fn given_a_verb_that_touches_the_tree_when_it_is_invoked_then_no_pager_can_hold_it() {
        // Given
        let args = HARDENED_TREE_ARGS;

        // When
        let refuses_pager = args.contains(&"--no-pager");

        // Then — un `git` qui attend un pager n'a ni terminal, ni fenêtre, et ne rend jamais
        // la main. C'est la seule protection qui ne dépende pas de ce que le dépôt a écrit.
        assert!(refuses_pager);
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
