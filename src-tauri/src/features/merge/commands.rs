//! La surface de la feature vers le frontend.
//!
//! Le frontend ne connaît de `merge` que ces cinq noms et les types qui traversent. Il
//! **rend** ce que le backend détient
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : l'écran ne lit
//! aucun fichier, ne compte aucun conflit et ne décide jamais quel côté est lequel.
//!
//! La liste des onglets **n'est pas ici** : elle est au composition root
//! (`src-tauri/src/tabs.rs`), avec celle des shells, parce que l'ordre que `⌘1..9`
//! numérote couvre les deux.

use std::path::Path;
use std::sync::Arc;

use super::error::MergeError;
use super::ports::MergeOutcome;
use super::surface::{MergeSurface, MergeView};
use super::tabs::TabId;

/// Ouvre l'onglet de merge d'un worktree — la seconde route de la spec §7.4.
///
/// Rendre l'onglet **déjà ouvert** quand il y en a un : un worktree n'a qu'une opération
/// arrêtée, et deux onglets dessus se contrediraient dès le premier hunk tranché.
///
/// Refuse quand rien n'est arrêté. C'est aussi la réponse dont `⌘⌃M` (#32) a besoin pour
/// savoir s'il est actif, et c'est la même que celle de `git_stopped_operation`.
#[tauri::command]
pub fn merge_open(
    surface: tauri::State<'_, Arc<MergeSurface>>,
    worktree_root: String,
) -> Result<TabId, MergeError> {
    surface.open(
        Path::new(&worktree_root),
        ulid::Ulid::generate().to_string(),
    )
}

/// Ferme l'onglet. **Rien n'est écrit, rien n'est perdu** (spec §7.4).
#[tauri::command]
pub fn merge_close(surface: tauri::State<'_, Arc<MergeSurface>>, tab_id: String) {
    surface.close(&tab_id);
}

/// Ce que l'onglet montre maintenant : les fichiers, leurs hunks, les deux côtés nommés,
/// le compte, `ORIG_HEAD` et les sorties de secours.
///
/// Relu à chaque appel, sans rien retenir entre deux.
#[tauri::command]
pub fn merge_view(
    surface: tauri::State<'_, Arc<MergeSurface>>,
    tab_id: String,
) -> Result<MergeView, MergeError> {
    surface.view(&tab_id)
}

/// Tranche un hunk : réécrit le fichier, puis `git add` s'il n'a plus de marqueur.
///
/// Le seul endroit d'Ash qui réécrive un fichier de travail de l'utilisateur, et il ne part
/// jamais sans un geste sur un hunk qu'il a sous les yeux.
#[tauri::command]
pub fn merge_resolve(
    surface: tauri::State<'_, Arc<MergeSurface>>,
    tab_id: String,
    path: String,
    hunk: u32,
    resolution: String,
) -> Result<MergeView, MergeError> {
    surface.resolve(&tab_id, &path, hunk, &resolution)
}

/// `git <op> --continue` — le bouton du critère, quand il est allumé.
///
/// La garde est **ici aussi** : un `continue` demandé alors qu'un conflit reste ne lance
/// rien et dit pourquoi. Un bouton éteint est une politesse, pas une garantie.
#[tauri::command]
pub fn merge_continue(
    surface: tauri::State<'_, Arc<MergeSurface>>,
    tab_id: String,
) -> Result<MergeOutcome, MergeError> {
    surface.resume(&tab_id)
}

/// Le prompt pour « passer le reste à l'agent », sur les conflits qui **restent**.
///
/// Rendre ce texte n'écrit rien nulle part : c'est `pty_compose` qui le pose dans le
/// terminal, et l'utilisateur seul qui l'envoie
/// ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)). Le
/// compositeur est celui de #29, appelé sur un sous-ensemble de chemins — il n'y en a pas
/// de second.
///
/// `None` quand il ne reste rien à passer.
#[tauri::command]
pub fn merge_rest_prompt(
    surface: tauri::State<'_, Arc<MergeSurface>>,
    tab_id: String,
) -> Result<Option<String>, MergeError> {
    surface.rest_prompt(&tab_id)
}
