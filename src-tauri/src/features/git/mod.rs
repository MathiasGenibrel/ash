//! Git, domaine de premier plan ([ADR-0011](../../../../docs/adr/0011-git-domaine-de-premier-plan.md)).
//!
//! Cette feature possédera les refs, le graphe et l'état de rebase. Elle n'apporte pour
//! l'instant que la **résolution** d'un `cwd` vers son worktree et son dépôt commun —
//! la brique dont dépend la hiérarchie à trois niveaux d'
//! [ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md).
//!
//! Rien ici n'invoque le binaire `git` : tout se lit dans les fichiers de contrôle du
//! dépôt, derrière le trait [`FileSystem`].

mod error;
mod ports;
mod system_fs;
mod worktree;

pub use error::GitError;
pub use ports::{Entry, FileSystem};
pub use system_fs::SystemFileSystem;
pub use worktree::{resolve_workspace, Repo, Workspace, Worktree};
