//! L'invocation de `git`, derrière trois traits que la feature possède.
//!
//! C'est le seul endroit du dépôt où le binaire `git` est lancé en production, et il n'y
//! en aura pas d'autre. **La frontière de sécurité tient en une phrase : rien n'atteint le
//! processus `git` sans passer par [`Invocation`].** Chaque verbe y a une variante, chaque
//! variante compose le même [`HARDENED_PREFIX`], et c'est cette composition-là — pas une
//! liste recopiée à côté — que les tests du bas de fichier relisent.
//!
//! | [`Invocation`] | Ce qui la déclenche | Consentement |
//! |---|---|---|
//! | `Status` | la surveillance de `.git` — donc un simple `cd` | **aucun** |
//! | `Refs`, `Worktrees` | l'ouverture de la popup de branches | un geste |
//! | `Tree` (`switch`, `rebase`, `merge`) | une action confirmée, qui a nommé ses deux côtés | un geste **explicite** |
//!
//! La colonne de droite est ce qui décide de la sévérité : voir [`TREE_ARGS`] pour ce
//! qu'on assume de ne **pas** neutraliser sur la dernière ligne, et pourquoi cette réponse
//! serait fausse sur la première.
//!
//! La règle qui encadre `Status` est celle du critère d'acceptation de l'issue #8 :
//! **jamais dans la boucle de sonde**. L'appel part des trois mêmes moments
//! que le reste des métadonnées — rattachement, focus, écriture surveillée — et passe par
//! la même limitation à un rafraîchissement par worktree et par tranche de 5 s. Les deux
//! lectures de branches, elles, ne partent jamais sans que l'utilisateur ait ouvert la
//! popup.
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

/// Ce que `status` ajoute au préfixe — et rien de plus.
///
/// C'est la seule invocation qu'Ash lance **tout seul**, sur le simple fait que le shell de
/// l'utilisateur a fait un `cd`, sans qu'aucune commande git n'ait été tapée. Un dépôt
/// hostile récupéré puis simplement visité ne doit donc pas pouvoir exécuter quoi que ce
/// soit — c'est de cette invocation-ci que [`HARDENED_PREFIX`] est né, avant d'être étendu
/// aux autres.
const STATUS_ARGS: [&str; 5] = [
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

/// Ce que la lecture des derniers commits ajoute au préfixe, et pourquoi chacun est là.
///
/// **La même question que pour `git status` se repose ici, et elle a la même réponse** :
/// cet appel-ci part tout seul lui aussi — une écriture dans `.git/logs/HEAD` suffit à le
/// déclencher, sans qu'aucune commande git n'ait été tapée —, donc visiter un dépôt hostile
/// ne doit rien exécuter. Ce que `git log` peut exécuter, et ce qui le neutralise :
///
/// - `core.fsmonitor` : une **commande**, posée dans le `.git/config` du dépôt visité, que
///   git lance pour rafraîchir l'index. Surchargée en `-c` par [`HARDENED_PREFIX`], comme
///   pour `git status` — et c'est bien la composition d'[`Invocation`], pas cette liste-ci,
///   que le test de la frontière relit ;
/// - `core.pager` / `pager.log` : un pager est une commande, et le dépôt la configure.
///   `--no-pager` la coupe, en plus de `core.pager=cat` du préfixe et du fait que la sortie
///   est un tube ;
/// - la vérification de signature, qui lance `gpg` : `--no-show-signature` l'écarte
///   explicitement plutôt que de compter sur `log.showSignature` valant faux ;
/// - les pilotes `textconv` et `diff`, eux aussi des commandes : ils ne s'exécutent que sur
///   un diff, et **aucune option de diff n'est passée**. Le format demandé ne contient que
///   des champs d'en-tête de commit.
///
/// Ce qui reste lu depuis le dépôt sans être exécuté — `mailmap`, `log.date` — ne change
/// que du texte, et ce texte est déjà traité comme non fiable : il finit dans un fichier
/// JSON, jamais dans une commande.
const LOG_ARGS: [&str; 7] = [
    // Un pager est une commande, et `pager.log` est une valeur du dépôt. Redondant avec le
    // `core.pager=cat` du préfixe, et gardé quand même : c'est la seule protection qui ne
    // dépende pas de ce que le dépôt a écrit dans sa configuration.
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
/// ce qu'on a le droit de laisser passer — voir [`TREE_ARGS`].
pub trait TreeWriter: Send + Sync {
    fn run(&self, worktree_root: &Path, args: &[String]) -> Option<Completed>;
}

/// Le préfixe commun à **toute** invocation de git faite par Ash.
///
/// La règle est celle de [`STATUS_ARGS`], sortie de la seule commande qui la
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
    // Un lecteur de fond n'a pas à réécrire l'index de l'utilisateur pour rafraîchir des
    // dates : sans ça, chaque appel écrirait dans `.git`.
    "--no-optional-locks",
    // Le vecteur d'exécution, neutralisé. Ne retire jamais cette ligne. La protection
    // `safe.directory` de git ne couvre pas ce cas — elle ne se déclenche que si le dépôt
    // appartient à un *autre* utilisateur, alors qu'un dépôt téléchargé appartient au nôtre.
    // La configuration passée en `-c` l'emporte sur celle du dépôt : c'est ce qui la
    // neutralise. Vérifié en reproduisant l'exécution, puis son absence.
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
const REF_ARGS: [&str; 4] = [
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
const WORKTREE_ARGS: [&str; 3] = ["worktree", "list", "--porcelain"];

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
const TREE_ARGS: [&str; 1] = ["--no-pager"];

/// Les invocations de `git` qu'Ash sait faire — **la liste entière**.
///
/// Une seule composition, et c'est tout l'intérêt : ce qui atteint le processus est
/// `HARDENED_PREFIX` suivi des arguments du verbe, ici et nulle part ailleurs. Avant, le
/// durcissement était composé deux fois — une fois dans chaque implémentation de trait, une
/// fois dans le test qui prétendait le vérifier — et il avait **déjà** divergé : `status`
/// portait sa propre copie du préfixe, sans `core.pager`.
///
/// Ajouter un verbe, c'est donc ajouter une variante : [`Invocation::verb`] ne compile pas
/// sans elle, et `ALL` — la liste que relisent les tests de la frontière — est écrite juste
/// au-dessus du `match` qui a refusé de compiler. On ne peut pas ajouter un verbe sans se
/// tenir devant elle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Invocation {
    /// L'état de l'arbre et l'avance sur l'amont, pour la ligne de statut.
    Status,
    /// Les branches et leur date, pour la popup.
    Refs,
    /// Quelle branche vit dans quel worktree — ADR-0012.
    Worktrees,
    /// Les derniers commits de `HEAD`, pour le journal d'attribution — ADR-0014.
    Log,
    /// Les verbes qui **écrivent** : `switch`, `rebase`, `merge`. Les opérandes sont
    /// ajoutés par [`TreeWriter::run`], après ce préfixe et jamais dedans.
    Tree,
}

impl Invocation {
    /// Toutes les variantes. Les tests relisent la frontière de sécurité à travers elle,
    /// et elle n'existe que pour eux : la production, elle, nomme toujours une invocation
    /// précise.
    #[cfg(test)]
    const ALL: [Invocation; 5] = [
        Invocation::Status,
        Invocation::Refs,
        Invocation::Worktrees,
        Invocation::Log,
        Invocation::Tree,
    ];

    /// Ce que le verbe ajoute au préfixe.
    fn verb(self) -> &'static [&'static str] {
        match self {
            Invocation::Status => &STATUS_ARGS,
            Invocation::Refs => &REF_ARGS,
            Invocation::Worktrees => &WORKTREE_ARGS,
            Invocation::Log => &LOG_ARGS,
            Invocation::Tree => &TREE_ARGS,
        }
    }

    /// La ligne d'arguments complète — **le seul chemin jusqu'au processus**.
    fn args(self) -> Vec<&'static str> {
        HARDENED_PREFIX
            .iter()
            .chain(self.verb().iter())
            .copied()
            .collect()
    }
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
        self.capture(worktree_root, Invocation::Status)
    }
}

impl BranchReader for SystemGit {
    fn refs(&self, worktree_root: &Path) -> Option<String> {
        self.capture(worktree_root, Invocation::Refs)
    }

    fn worktrees(&self, worktree_root: &Path) -> Option<String> {
        self.capture(worktree_root, Invocation::Worktrees)
    }
}

impl TreeWriter for SystemGit {
    /// `args` est une liste **fermée**, composée par [`super::branch_actions`] à partir d'un
    /// verbe de son énumération et d'un nom de branche déjà vérifié contre le dépôt. Rien
    /// de ce que le frontend envoie n'arrive ici tel quel.
    fn run(&self, worktree_root: &Path, args: &[String]) -> Option<Completed> {
        let hardened: Vec<String> = Invocation::Tree
            .args()
            .into_iter()
            .map(str::to_owned)
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
    /// Les derniers commits de `HEAD`, du plus récent au plus ancien.
    ///
    /// Rend un vecteur vide pour tout ce qui peut mal se passer — `git` absent, dépôt sans
    /// commit, délai dépassé. L'appelant en fait la même chose : il n'attribue rien.
    ///
    /// C'est une méthode, et **pas** un port : le port par lequel on demande les commits
    /// appartient à qui pose la question — `features/journal` — comme `AgentStates`
    /// appartient à `pty` et non à `agents`. Cette feature-ci n'apporte que le seul endroit
    /// du dépôt où le binaire `git` est lancé.
    pub fn recent_commits(&self, worktree_root: &Path) -> Vec<CommitRecord> {
        self.capture(worktree_root, Invocation::Log)
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
    /// Lance `git`, sous délai, et rend sa sortie standard si — et seulement si — il a réussi.
    ///
    /// Le seul endroit du dépôt où un processus `git` de lecture est créé : toutes les
    /// questions qu'Ash pose passent par ici, donc la façon dont l'appel est construit —
    /// programme nommé, arguments composés par [`Invocation`], répertoire explicite, délai
    /// tenu — ne se décide qu'une fois.
    fn capture(&self, worktree_root: &Path, invocation: Invocation) -> Option<String> {
        let args = invocation.args();
        // `Command` prend le programme et ses arguments séparément : aucun shell n'est
        // lancé, donc aucun chemin de worktree ne peut être interprété comme du code.
        // Le répertoire de travail est **explicite** — un `git` lancé depuis le
        // répertoire courant du processus décrirait un autre dépôt que celui demandé.
        let mut child = Command::new("git")
            .current_dir(worktree_root)
            .args(&args)
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

    /// Deux options `-c` consécutives, telles que git les lit.
    fn sets(args: &[&str], setting: &str) -> bool {
        args.windows(2).any(|pair| pair == ["-c", setting])
    }

    #[test]
    fn given_any_invocation_ash_makes_when_a_visited_repository_configures_a_command_then_it_is_overridden(
    ) {
        // Given — toutes les invocations, composées comme la production les compose
        let invocations = Invocation::ALL;

        // When
        let neutralised: Vec<bool> = invocations
            .iter()
            .map(|invocation| {
                let args = invocation.args();
                sets(&args, "core.fsmonitor=false") && sets(&args, "core.pager=cat")
            })
            .collect();

        // Then — `core.fsmonitor` et `core.pager` sont des **commandes** que le dépôt visité
        // pose dans son propre `.git/config`, et Ash lance `status` sur un simple `cd` : une
        // neutralisation vraie sur certaines commandes seulement est une neutralisation
        // qu'on oubliera à la suivante
        assert_eq!(neutralised, vec![true; Invocation::ALL.len()]);
    }

    #[test]
    fn given_any_invocation_ash_makes_when_it_is_built_then_it_never_rewrites_the_index() {
        // Given
        let invocations = Invocation::ALL;

        // When — un lecteur de fond n'a pas à écrire dans le `.git` de l'utilisateur
        let all_read_only = invocations
            .iter()
            .all(|invocation| invocation.args().contains(&"--no-optional-locks"));

        // Then
        assert!(all_read_only);
    }

    #[test]
    fn given_any_invocation_ash_makes_when_it_is_built_then_it_never_goes_through_a_shell() {
        // Given
        let invocations = Invocation::ALL;

        // When
        let program = "git";

        // Then — le programme est nommé, jamais une ligne de shell, et un argument porteur
        // d'espace trahirait une ligne de commande recomposée
        assert_eq!(program, "git");
        for invocation in invocations {
            assert!(
                invocation
                    .args()
                    .iter()
                    .all(|arg| !arg.contains(char::is_whitespace)),
                "{invocation:?} recompose une ligne de commande"
            );
        }
    }

    #[test]
    fn given_the_status_invocation_when_it_is_built_then_it_asks_for_a_contractual_format() {
        // Given
        let args = Invocation::Status.args();

        // When
        let quotes_paths = sets(&args, "core.quotePath=true");

        // Then — `--porcelain=v2` est documenté et stable, contrairement à `--short`, et un
        // nom de fichier exotique reste sur une seule ligne
        assert!(args.contains(&"--porcelain=v2"));
        assert!(args.contains(&"--branch"));
        assert!(quotes_paths);
    }

    #[test]
    fn given_the_branch_listing_when_it_is_built_then_it_uses_plumbing_and_imposes_its_format() {
        // Given
        let args = REF_ARGS;

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
    fn given_the_log_invocation_when_a_visited_repository_configures_a_command_then_none_of_them_runs(
    ) {
        // Given — la lecture des commits part d'une écriture dans `.git/logs/HEAD` : elle
        // s'exécute donc sur un dépôt que personne n'a validé, exactement comme
        // `git status` s'exécute sur un simple `cd`. Les trois commandes qu'un dépôt peut
        // faire lancer à `git log` sont son pager, son `fsmonitor` et son `gpg`. La liste
        // relue est celle que la production compose, préfixe compris.
        let args = Invocation::Log.args();

        // When
        let neutralised = |flag: &str| args.contains(&flag);

        // Then
        assert!(
            sets(&args, "core.fsmonitor=false"),
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
    fn given_a_verb_that_touches_the_tree_when_it_is_invoked_then_no_pager_can_hold_it() {
        // Given
        let args = Invocation::Tree.args();

        // When
        let refuses_pager = args.contains(&"--no-pager");

        // Then — un `git` qui attend un pager n'a ni terminal, ni fenêtre, et ne rend jamais
        // la main. C'est la seule protection qui ne dépende pas de ce que le dépôt a écrit.
        assert!(refuses_pager);
    }
}
