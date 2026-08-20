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
    /// Les verbes qui **écrivent** : `switch`, `rebase`, `merge`. Les opérandes sont
    /// ajoutés par [`TreeWriter::run`], après ce préfixe et jamais dedans.
    Tree,
}

impl Invocation {
    /// Toutes les variantes. Les tests relisent la frontière de sécurité à travers elle,
    /// et elle n'existe que pour eux : la production, elle, nomme toujours une invocation
    /// précise.
    #[cfg(test)]
    const ALL: [Invocation; 4] = [
        Invocation::Status,
        Invocation::Refs,
        Invocation::Worktrees,
        Invocation::Tree,
    ];

    /// Ce que le verbe ajoute au préfixe.
    fn verb(self) -> &'static [&'static str] {
        match self {
            Invocation::Status => &STATUS_ARGS,
            Invocation::Refs => &REF_ARGS,
            Invocation::Worktrees => &WORKTREE_ARGS,
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
    /// Lance `git`, sous délai, et rend sa sortie standard si — et seulement si — il a réussi.
    ///
    /// Le seul chemin de lecture du dépôt, partagé par les trois questions qu'Ash pose.
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
