//! Git, domaine de premier plan ([ADR-0011](../../../../docs/adr/0011-git-domaine-de-premier-plan.md)).
//!
//! La feature apporte deux choses, et la seconde s'appuie sur la première :
//!
//! - la **résolution** d'un `cwd` vers son worktree et son dépôt commun — la brique dont
//!   dépend la hiérarchie à trois niveaux d'
//!   [ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md) ;
//! - les **métadonnées** d'un worktree — branche et opération en cours — tenues à jour par
//!   **surveillance de fichiers**, jamais par sondage (spec §5.3).
//!
//! La résolution et la lecture des fichiers de contrôle n'invoquent **jamais** le binaire
//! `git` : tout se lit derrière le trait [`FileSystem`]. Le seul appel à `git` de tout le
//! dépôt est celui de [`git_cli`], pour l'état de l'arbre et l'avance sur l'amont, que
//! rien dans `.git` ne porte. Il est déclenché par la surveillance et par elle seule —
//! jamais par la boucle de sonde, ce que l'ADR-0011 exclut explicitement.
//!
//! **Les effets système de la feature**, chacun avec ses deux adaptateurs — celui du
//! système, et celui des tests :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `FileSystem` (`ports.rs`) | `system_fs.rs` | `fake_fs.rs`, `fakes.rs` |
//! | `FileWatcher` (`watcher.rs`) | `watcher.rs` | `fakes.rs` |
//! | `Clock`, `Scheduler` (`shared/time.rs`) | `shared/time.rs` | `fakes.rs` |
//! | `StatusReader` (`git_cli.rs`) | `git_cli.rs` | `fakes.rs` |

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod control;
mod error;
mod git_cli;
mod metadata;
mod metadata_watch;
mod porcelain;
mod ports;
mod system_fs;
mod targets;
mod throttle;
mod watcher;
mod worktree;

/// L'arbre en mémoire qui double le port `FileSystem` dans les tests de la feature.
#[cfg(test)]
mod fake_fs;

/// Les doubles des autres effets système : surveillance, horloge, reports.
#[cfg(test)]
mod fakes;

pub use error::GitError;
pub use git_cli::{CommitRecord, StatusReader, SystemGit, STATUS_TIMEOUT};
pub use metadata::{
    read_metadata, Head, Operation, OperationKind, Progress, Status, TreeStatus, Upstream,
    WorktreeMetadata,
};
pub use metadata_watch::MetadataWatch;
pub use porcelain::parse_status;
pub use ports::{Entry, FileSystem};
pub use system_fs::SystemFileSystem;
pub use worktree::{resolve_worktree, Repo, Worktree, WorktreeLocation};
