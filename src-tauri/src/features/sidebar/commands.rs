//! La surface de la feature vers le frontend : trois commandes, un event.
//!
//! Le frontend ne connaît de l'état de la colonne que ces noms et la forme de
//! [`SidebarRows`]. Il **rend** les lignes ; ce qui survit à la fermeture est ici
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! **Un geste ne rend rien, il annonce.** Épingler et replier suivent le chemin du thème :
//! la commande retient, l'event porte l'état entier, et la webview redessine à partir de lui.
//! Rendre le nouvel état à l'appelante aurait donné deux routes vers l'écran — celle du
//! retour d'appel et celle de l'annonce — et il aurait fallu qu'elles restent d'accord.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Runtime};

use super::state::{SidebarRows, SidebarState};

/// Nom de l'event qui porte l'état de la colonne. Contrat avec `src/app/sidebar-rows.ts`.
pub const SIDEBAR_ROWS_EVENT: &str = "ash://sidebar-rows";

/// Les épingles et les lignes repliées, lues par la webview en s'affichant.
///
/// Ensuite, c'est l'event qui la tient à jour : elle ne redemande jamais.
#[tauri::command]
pub fn sidebar_rows(state: tauri::State<'_, Arc<SidebarState>>) -> SidebarRows {
    state.snapshot()
}

/// Épingle ou désépingle un worktree — le geste de la spec §5.2.
#[tauri::command]
pub fn sidebar_pin<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, Arc<SidebarState>>,
    worktree_root: String,
    pinned: bool,
) {
    announce(&app, &state, state.pin(worktree_root, pinned));
}

/// Replie ou déplie une ligne — un worktree, ou un groupe de dépôt.
#[tauri::command]
pub fn sidebar_collapse<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, Arc<SidebarState>>,
    key: String,
    collapsed: bool,
) {
    announce(&app, &state, state.collapse(key, collapsed));
}

/// Annonce l'état de la colonne, et seulement s'il a bougé.
///
/// Un geste qui ne change rien — une épingle reposée, une ligne repliée deux fois — ne fait
/// pas repartir un rendu de la colonne pour rien.
fn announce<R: Runtime>(app: &AppHandle<R>, state: &Arc<SidebarState>, changed: bool) {
    if !changed {
        return;
    }
    // Échouer à émettre signifie qu'il n'y a plus de webview à prévenir : rien à rattraper,
    // et surtout pas de panique dans une commande.
    let _ = app.emit(SIDEBAR_ROWS_EVENT, state.snapshot());
}
