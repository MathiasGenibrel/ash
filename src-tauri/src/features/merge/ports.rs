//! Les trois effets que la feature ne fait pas elle-même.
//!
//! Chacun est un trait **que cette feature possède**, et non un type emprunté à `git` :
//! c'est ce qui permet de vérifier la totalité des règles de l'onglet — trancher un hunk,
//! réécrire le fichier, mettre à jour le compte, refuser `continue` — sans lancer un seul
//! processus `git` ni toucher au disque.
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | [`StoppedWorktree`] | `features::git::MetadataWatch`, câblé dans `lib.rs` | `fakes.rs` |
//! | [`ConflictFiles`] | `std::fs`, câblé dans `lib.rs` | `fakes.rs` |
//! | [`TreeGit`] | `features::git::TreeWriter`, câblé dans `lib.rs` | `fakes.rs` |
//!
//! Le dernier est le seul qui **écrive** quoi que ce soit dans le dépôt, et il ne part
//! jamais sans un geste — voir [`TreeGit`].

use std::path::Path;

use crate::features::git::{Head, OperationKind, StoppedOperation};

/// Ce que le worktree dit de son opération arrêtée.
///
/// Deux questions et non une : `stopped` porte l'opération, les chemins et le filet de
/// secours ; `head` porte la branche courante, dont **seul un merge** a besoin pour nommer
/// son côté gauche (voir [`super::sides`]). Les fondre en une seule réponse obligerait à
/// transporter un `HEAD` détaché dont un rebase ne fait rien.
pub trait StoppedWorktree: Send + Sync {
    fn stopped(&self, worktree_root: &Path) -> Option<StoppedOperation>;
    fn head(&self, worktree_root: &Path) -> Option<Head>;
}

/// Lire et réécrire un fichier du worktree.
///
/// C'est un port distinct du `FileSystem` de `features::git`, qui est en **lecture seule**
/// et doit le rester : cette feature-ci est la seule d'Ash à réécrire un fichier de travail
/// de l'utilisateur, et ce droit n'a pas à s'étendre à la résolution de worktree ni à la
/// surveillance de `.git` par le seul fait qu'elles partagent un trait.
pub trait ConflictFiles: Send + Sync {
    /// Le contenu, ou `None` s'il n'est pas lisible en UTF-8.
    fn read(&self, path: &Path) -> Option<String>;
    /// Réécrit le fichier **entier**. Rend la raison de l'échec.
    fn write(&self, path: &Path, text: &str) -> Result<(), String>;
}

/// Ce qu'une invocation git rend : son succès, et ce qu'elle a dit.
///
/// La sortie est gardée même en cas de succès — un `git rebase --continue` réussi écrit
/// « Successfully rebased », et c'est exactement ce que l'utilisateur veut lire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MergeOutcome {
    /// La phrase qui nomme ce qui a été tenté — les deux côtés, comme pour une action de
    /// branche (spec §7.1). Présente même quand `success` est faux.
    pub label: String,
    pub success: bool,
    /// Ce que git a dit, tel quel. Vide quand il n'a rien dit.
    pub output: String,
}

/// Les deux verbes git qui **écrivent**, et qui ne partent jamais tout seuls.
///
/// La question de sécurité de `features::git::git_cli` se repose pour chacun, et la réponse
/// tient à la colonne « consentement » de son tableau :
///
/// - **`git add -- <chemin>`** : met dans l'index un fichier que l'utilisateur vient de
///   trancher, hunk par hunk, dans l'écran. Il déclenche les filtres `clean` du dépôt
///   (`.gitattributes` `filter=x`) — donc du code du dépôt visité. Ce n'est **pas**
///   neutralisé, délibérément, et pour la même raison que les pilotes de fusion de #25 :
///   ce verbe part d'un clic sur un hunk que l'utilisateur regarde. Le neutraliser
///   casserait git-lfs sur un dépôt légitime pour se protéger d'un dépôt qu'on est déjà en
///   train de fusionner. Un `git add` qui partirait **tout seul**, sur un simple `cd`,
///   rendrait cette ligne fausse.
/// - **`git <op> --continue`** : conclut l'opération, donc écrit un commit, donc déclenche
///   `pre-commit`, `commit-msg`, `post-commit`, `post-rewrite`. Même réponse, avec un
///   argument de plus : ce sont les hooks **du projet de l'utilisateur**, et un rebase
///   terminé par Ash sans eux serait un rebase que son `pre-commit` n'a jamais vu — un
///   commit qu'il croit vérifié et qui ne l'est pas. Les couper serait le danger.
///
/// Ce qui **est** ajouté au durcissement commun : `core.editor=true`, dans
/// `features::git::git_cli`. Sans lui, `git rebase --continue` ouvre `$EDITOR` pour le
/// message du commit — un processus sans terminal ni fenêtre, qui ne rendrait jamais la
/// main. Le message d'origine est repris tel quel : Ash n'en réécrit aucun.
pub trait TreeGit: Send + Sync {
    /// `git add -- <chemin>` : ce fichier n'a plus de conflit.
    fn stage(&self, worktree_root: &Path, path: &str) -> MergeOutcome;
    /// `git <rebase|am|merge> --continue`.
    fn resume(&self, worktree_root: &Path, kind: OperationKind) -> MergeOutcome;
}
