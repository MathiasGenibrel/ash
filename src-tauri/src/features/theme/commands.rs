//! La surface de la feature vers le frontend : une commande, un event.
//!
//! Le frontend ne connaît du thème que ces deux noms et les trois identifiants de mode. Il
//! **rend** la palette ; le choix, lui, est ici
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! Ce que ce module ne contient **pas**, et c'est délibéré : le menu natif. Ses
//! identifiants d'entrée et ses coches vivent dans `src-tauri/src/menu.rs`, avec le code
//! qui construit l'arbre — une feature n'a pas à connaître la forme du menu.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::mode::ThemeMode;
use super::state::ThemeState;

/// Nom de l'event qui porte le mode choisi. Contrat avec `src/app/theme.ts`.
pub const THEME_MODE_EVENT: &str = "ash://theme-mode";

/// Le mode courant, lu par la webview en s'affichant.
///
/// Ensuite, c'est l'event qui la tient à jour : elle ne redemande jamais.
#[tauri::command]
pub fn theme_mode(state: tauri::State<'_, Arc<ThemeState>>) -> ThemeMode {
    state.mode()
}

/// Retient un choix et l'annonce à la webview.
///
/// Rend `true` si le mode a changé — un même mode rechoisi n'émet rien : la palette est
/// déjà la bonne, et réémettre ferait repeindre la fenêtre pour rien.
pub fn choose<R: Runtime>(app: &AppHandle<R>, mode: ThemeMode) -> bool {
    let Some(state) = app.try_state::<Arc<ThemeState>>() else {
        return false;
    };
    if !state.set(mode) {
        return false;
    }

    // Échouer à émettre signifie qu'il n'y a plus de webview à prévenir : rien à
    // rattraper, et surtout pas de panique dans un gestionnaire d'event de menu.
    let _ = app.emit(THEME_MODE_EVENT, mode);
    true
}
