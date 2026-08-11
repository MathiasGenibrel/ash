//! La surface de la feature vers le frontend : une commande, un event.
//!
//! Le frontend ne connaît de `git` que ces deux noms et les types qui traversent. Il
//! **rend** l'état ; c'est ici qu'on le lui pousse
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Runtime};

use super::metadata::WorktreeMetadata;
use super::metadata_watch::MetadataWatch;
use super::system_fs::SystemFileSystem;
use super::throttle::MIN_INTERVAL;
use super::time::{SystemClock, ThreadScheduler};
use super::watcher::SystemWatcher;

/// Nom de l'event qui porte l'état git d'un worktree.
///
/// Contrat avec `src/shared/ipc/` : une chaîne que rien ne vérifie à la compilation, comme
/// celle des onglets.
pub const METADATA_CHANGED_EVENT: &str = "ash://git-metadata";

/// L'état git d'un worktree, tel qu'il traverse la frontière.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataChanged {
    /// La racine du worktree — la même clé que celle des onglets (`TabLocation`).
    pub worktree_root: String,
    pub metadata: WorktreeMetadata,
}

/// L'état git connu d'un worktree.
///
/// Ce que le frontend lit en s'affichant ; ensuite, c'est l'event qui le tient à jour.
/// `None` pour un répertoire hors de tout dépôt, ou dont les fichiers de contrôle ne se
/// lisent pas — les deux se rendent pareil : sans métadonnées git.
#[tauri::command]
pub fn git_metadata(
    watch: tauri::State<'_, Arc<MetadataWatch>>,
    worktree_root: String,
) -> Option<WorktreeMetadata> {
    watch.metadata(Path::new(&worktree_root))
}

/// Construit la surveillance avec les adaptateurs du système, et la relie à la fenêtre.
///
/// Le pendant de `pty::commands::watch_tabs` : l'assemblage des effets réels d'une feature
/// se fait dans son `commands.rs`, et le composition root ne fait que déclencher et arrêter.
pub fn watch_metadata<R: Runtime>(app: AppHandle<R>) -> Arc<MetadataWatch> {
    MetadataWatch::new(
        Arc::new(SystemFileSystem),
        Arc::new(SystemWatcher),
        Arc::new(SystemClock),
        Arc::new(ThreadScheduler),
        MIN_INTERVAL,
        Arc::new(move |root: &Path, metadata: &WorktreeMetadata| {
            // Échouer à émettre signifie qu'il n'y a plus de webview à prévenir : rien à
            // rattraper, et surtout pas de panique dans un fil de fond.
            let _ = app.emit(
                METADATA_CHANGED_EVENT,
                MetadataChanged {
                    worktree_root: root.display().to_string(),
                    metadata: metadata.clone(),
                },
            );
        }),
    )
}
