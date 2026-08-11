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
//! Rien ici n'invoque le binaire `git` : tout se lit dans les fichiers de contrôle du
//! dépôt, derrière le trait [`FileSystem`]. Un `git status` par cycle de sonde coûterait
//! un `fork` par onglet trois fois par seconde ; c'est ce que l'ADR-0011 exclut, et ce que
//! la surveillance remplace.
//!
//! **Les effets système de la feature**, chacun avec ses deux adaptateurs — celui du
//! système, et celui des tests :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `FileSystem` (`ports.rs`) | `system_fs.rs` | `fake_fs.rs`, `fakes.rs` |
//! | `FileWatcher` (`watcher.rs`) | `watcher.rs` | `fakes.rs` |
//! | `Clock`, `Scheduler` (`time.rs`) | `time.rs` | `fakes.rs` |

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod control;
mod error;
mod metadata;
mod metadata_watch;
mod ports;
mod system_fs;
mod targets;
mod throttle;
mod time;
mod watcher;
mod worktree;

/// L'arbre en mémoire qui double le port `FileSystem` dans les tests de la feature.
#[cfg(test)]
mod fake_fs;

/// Les doubles des autres effets système : surveillance, horloge, reports.
#[cfg(test)]
mod fakes;

pub use error::GitError;
pub use metadata::{read_metadata, Head, Operation, OperationKind, Progress, WorktreeMetadata};
pub use metadata_watch::MetadataWatch;
pub use ports::{Entry, FileSystem};
pub use system_fs::SystemFileSystem;
pub use worktree::{resolve_worktree, Repo, Worktree, WorktreeLocation};
