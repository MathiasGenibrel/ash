//! Quels commits viennent de naître dans ce worktree ? Le port par lequel le journal le
//! demande.
//!
//! **C'est le troisième effet système de la feature, et il lui appartient comme les deux
//! autres.** La question se pose ici, donc le port est ici : c'est la convention du dépôt —
//! *les effets système passent par un trait que la feature possède* — et c'est aussi ce que
//! `pty` fait déjà avec `AgentStates`, qu'il possède alors que la réponse vient de `agents`.
//!
//! Ce que cela ne change pas : **il n'y a toujours qu'un seul endroit du dépôt où le binaire
//! `git` est lancé**, et c'est `features/git/git_cli.rs`, avec la frontière de sécurité qui
//! l'encadre. `SystemGit::recent_commits` y reste une méthode, sans trait ; le composition
//! root la relie à ce port, comme il relie le registre de PTY à [`super::Tabs`]. Le journal
//! ne connaît donc pas `git` : il connaît une question, et un [`CommitRecord`] — le
//! vocabulaire d'un commit, qui appartient à `features/git` parce que c'est git qui le dit,
//! et que la colonne `by` du graphe (#27) lira le même.

use std::path::Path;

pub use crate::features::git::CommitRecord;

/// À qui le journal demande ce que `HEAD` porte de plus récent.
///
/// Rend les commits **du plus récent au plus ancien**, et un vecteur vide pour tout ce qui
/// peut mal se passer — `git` absent, dépôt sans commit, délai dépassé. L'appelant en fait
/// la même chose : il n'attribue rien.
pub trait CommitLog: Send + Sync {
    fn recent(&self, worktree_root: &Path) -> Vec<CommitRecord>;
}
