//! La surface de la feature vers le frontend : une commande, un event.
//!
//! Le frontend ne connaît du thème que ces deux noms et les trois identifiants de mode. Il
//! **rend** la palette ; le choix, lui, est ici
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).

use std::sync::Arc;

use tauri::menu::MenuItemKind;
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

/// Retient un choix et le fait suivre — à la webview, et aux coches du menu.
///
/// Appelée depuis le menu natif, qui est le seul point d'entrée à ce jalon : la fenêtre de
/// réglages est l'issue #14 et son écran d'apparence l'issue #22.
pub fn choose<R: Runtime>(app: &AppHandle<R>, mode: ThemeMode) {
    let Some(state) = app.try_state::<Arc<ThemeState>>() else {
        return;
    };
    if !state.set(mode) {
        return;
    }

    // Échouer à émettre signifie qu'il n'y a plus de webview à prévenir : rien à
    // rattraper, et surtout pas de panique dans un gestionnaire d'event de menu.
    let _ = app.emit(THEME_MODE_EVENT, mode);
    check_only(app, mode);
}

/// Coche le mode retenu, et lui seul.
///
/// Sans ça, un menu à trois coches les garderait toutes : `CheckMenuItem` bascule sa
/// propre coche au clic et ne sait rien de ses voisines.
pub fn check_only<R: Runtime>(app: &AppHandle<R>, mode: ThemeMode) {
    let Some(menu) = app.menu() else {
        return;
    };
    for candidate in ThemeMode::ALL {
        if let Some(item) = find_check(&menu.items().unwrap_or_default(), &item_id(candidate)) {
            let _ = item.set_checked(candidate == mode);
        }
    }
}

/// L'identifiant de l'entrée de menu d'un mode. Contrat avec `src-tauri/src/menu.rs`.
pub fn item_id(mode: ThemeMode) -> String {
    format!("view:theme:{}", mode.as_id())
}

/// Le mode que désigne un identifiant d'entrée de menu, s'il en désigne un.
pub fn mode_of(id: &str) -> Option<ThemeMode> {
    ThemeMode::from_id(id.strip_prefix("view:theme:")?)
}

/// Retrouve une entrée à cocher dans l'arbre du menu.
///
/// Un menu natif est un arbre, et `Menu::items` n'en rend que le premier niveau : les
/// entrées de thème vivent deux niveaux plus bas, sous « View » puis « Theme ». La
/// descente est bornée par la forme du menu, qu'on construit nous-mêmes.
fn find_check<R: Runtime>(
    items: &[MenuItemKind<R>],
    id: &str,
) -> Option<tauri::menu::CheckMenuItem<R>> {
    for item in items {
        match item {
            MenuItemKind::Check(check) if check.id().as_ref() == id => return Some(check.clone()),
            MenuItemKind::Submenu(submenu) => {
                if let Some(found) = find_check(&submenu.items().unwrap_or_default(), id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_theme_menu_entry_when_its_identifier_is_read_back_then_it_names_the_same_mode() {
        // Given — l'identifiant traverse le menu natif sous forme de chaîne, et rien ne le
        // vérifie à la compilation
        let modes = ThemeMode::ALL;

        // When
        let round_trip: Vec<Option<ThemeMode>> =
            modes.iter().map(|mode| mode_of(&item_id(*mode))).collect();

        // Then
        assert_eq!(round_trip, modes.map(Some).to_vec());
    }

    #[test]
    fn given_a_menu_entry_that_is_not_about_the_theme_when_it_is_read_then_it_is_not_a_mode() {
        // Given / When — `view:toggle-sidebar` partage le même préfixe de menu
        let sidebar = mode_of("view:toggle-sidebar");

        // Then
        assert_eq!(sidebar, None);
    }
}
