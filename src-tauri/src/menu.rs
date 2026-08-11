//! Le menu applicatif macOS — source unique des raccourcis de la spec §4.4.
//!
//! Il vit à côté du composition root, pas dans une feature : un menu est un objet de
//! fenêtre, partagé par toutes les features, et aucune d'elles ne doit avoir à le
//! connaître. Ce module ne fait rien lui-même — il traduit un clic ou un accélérateur en
//! une **action**, et l'émet vers la webview qui la joue.
//!
//! Pourquoi un menu natif plutôt qu'un `keydown` dans la webview :
//!
//! - c'est le même code pour le clavier et pour la souris, or la spec §4.4 exige que
//!   « toutes ces actions soient également atteignables à la souris » ;
//! - sur macOS, un accélérateur de menu est consommé par `performKeyEquivalent:` avant
//!   d'atteindre la webview : `Cmd+K` ne part donc pas dans le shell ;
//! - `Cmd+W` est sinon capté par la fermeture de fenêtre. Le menu « Window » construit
//!   ici n'a **volontairement pas** d'entrée « Close Window » : `Cmd+W` ferme un onglet,
//!   comme dans tout émulateur de terminal.
//!
//! Le prix est que la liste des accélérateurs est en Rust et leur effet en TypeScript.
//! C'est assumé : le frontend rend les onglets, il ne les détient pas
//! ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)), et faire l'inverse —
//! des raccourcis côté webview — aurait donné deux chemins différents pour la souris et
//! pour le clavier.

use tauri::menu::{
    AboutMetadata, CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};
use tauri::{AppHandle, Emitter, Runtime};

use crate::features::theme::{commands as theme, ThemeMode};

/// Nom de l'event qui porte l'action choisie. Contrat avec `src/app/menu.ts`.
const MENU_ACTION_EVENT: &str = "ash://menu-action";

/// Nombre d'onglets directement adressables — `Cmd+1` … `Cmd+9` (spec §4.4).
const DIRECT_TABS: u8 = 9;

/// Construit le menu de l'application.
///
/// `theme` est le mode retenu de la session précédente : le menu est construit avant que
/// la webview n'existe, et une coche posée après coup serait une seconde source de vérité.
pub fn build<R: Runtime>(app: &AppHandle<R>, theme_mode: ThemeMode) -> tauri::Result<Menu<R>> {
    let application = Submenu::with_items(
        app,
        "Ash",
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(AboutMetadata::default()))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    // Sans ces entrées, `Cmd+C` et `Cmd+V` ne fonctionnent pas dans une WKWebView : le
    // copier-coller de macOS passe par les items de menu, pas par la page.
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let new_tab = MenuItem::with_id(app, Action::NewTab.id(), "New Tab", true, Some("Cmd+N"))?;
    let new_home_tab = MenuItem::with_id(
        app,
        Action::NewHomeTab.id(),
        "New Tab at ~",
        true,
        Some("Cmd+Shift+N"),
    )?;
    let close_tab =
        MenuItem::with_id(app, Action::CloseTab.id(), "Close Tab", true, Some("Cmd+W"))?;
    let clear = MenuItem::with_id(
        app,
        Action::ClearScrollback.id(),
        "Clear Scrollback",
        true,
        Some("Cmd+K"),
    )?;

    // Les neuf entrées existent en permanence, même quand il y a moins d'onglets : une
    // action qui ne désigne personne est ignorée côté webview. Les activer et les
    // désactiver au fil des ouvertures ferait vivre l'état des onglets à deux endroits.
    let select: Vec<MenuItem<R>> = (1..=DIRECT_TABS)
        .map(|position| {
            MenuItem::with_id(
                app,
                Action::SelectTab(position).id(),
                format!("Tab {position}"),
                true,
                Some(format!("Cmd+{position}").as_str()),
            )
        })
        .collect::<tauri::Result<_>>()?;

    let separator = PredefinedMenuItem::separator(app)?;
    let mut terminal_items: Vec<&dyn tauri::menu::IsMenuItem<R>> = vec![
        &new_tab,
        &new_home_tab,
        &close_tab,
        &separator,
        &clear,
        &separator,
    ];
    terminal_items.extend(
        select
            .iter()
            .map(|item| item as &dyn tauri::menu::IsMenuItem<R>),
    );

    let terminal = Submenu::with_items(app, "Terminal", true, &terminal_items)?;

    // `Cmd+B` est un raccourci de **fenêtre**, pas d'onglet : il vit dans « View », à côté
    // de ce qui touchera à l'affichage. Comme les autres, il passe par le menu natif pour
    // ne pas partir dans le shell.
    let toggle_sidebar = MenuItem::with_id(
        app,
        Action::ToggleSidebar.id(),
        "Toggle Sidebar",
        true,
        Some("Cmd+B"),
    )?;
    // Les trois thèmes, en coches exclusives. C'est le **seul** point d'entrée du choix à
    // ce jalon : la fenêtre de réglages est l'issue #14, son écran d'apparence l'issue #22.
    // Pas d'accélérateur — un thème se change une fois par saison, pas une fois par heure,
    // et chaque raccourci pris ici est un raccourci perdu pour le shell.
    let themes: Vec<CheckMenuItem<R>> = ThemeMode::ALL
        .into_iter()
        .map(|mode| {
            CheckMenuItem::with_id(
                app,
                Action::ChooseTheme(mode).id(),
                mode.label(),
                true,
                mode == theme_mode,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    let theme_menu = Submenu::with_items(
        app,
        "Theme",
        true,
        &themes
            .iter()
            .map(|item| item as &dyn tauri::menu::IsMenuItem<R>)
            .collect::<Vec<_>>(),
    )?;

    let view = Submenu::with_items(app, "View", true, &[&toggle_sidebar, &theme_menu])?;

    // Pas de « Close Window » ici : son `Cmd+W` prendrait le pas sur celui des onglets.
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    Menu::with_items(app, &[&application, &edit, &view, &terminal, &window])
}

/// Traduit un item de menu en action et la donne à qui sait la jouer.
///
/// Deux chemins, et la différence n'est pas un détail : les actions d'onglet partent vers
/// la webview, qui détient les surfaces de rendu ; le thème, lui, est un **état**, et il
/// est retenu par `features::theme` avant d'être annoncé
/// ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)). Une bascule de thème qui
/// ne vivrait que dans la webview serait perdue à la première seconde fenêtre.
///
/// Un identifiant inconnu est ignoré : les items prédéfinis (copier, quitter…) sont
/// traités par le système et ne passent pas par ici.
pub fn dispatch<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let Some(action) = Action::from_id(id) else {
        return;
    };

    match action {
        Action::ChooseTheme(mode) => {
            theme::choose(app, mode);
            // **Toujours**, et pas seulement quand le mode a changé : un `CheckMenuItem`
            // bascule sa propre coche au clic. Cliquer l'entrée déjà cochée la décocherait
            // donc, et le menu n'aurait plus aucun mode coché.
            check_only(app, mode);
        }
        // L'échec d'émission signifie qu'il n'y a plus de webview à prévenir : rien à
        // rattraper, et surtout pas de panique dans un gestionnaire d'event.
        other => {
            let _ = app.emit(MENU_ACTION_EVENT, other.id());
        }
    }
}

/// Coche le mode retenu, et lui seul.
///
/// Sans ça, un menu à trois coches les garderait toutes : `CheckMenuItem` ne sait rien de
/// ses voisines.
fn check_only<R: Runtime>(app: &AppHandle<R>, mode: ThemeMode) {
    let Some(menu) = app.menu() else {
        return;
    };
    for candidate in ThemeMode::ALL {
        let id = Action::ChooseTheme(candidate).id();
        if let Some(item) = find_check(&menu.items().unwrap_or_default(), &id) {
            let _ = item.set_checked(candidate == mode);
        }
    }
}

/// Retrouve une entrée à cocher dans l'arbre du menu.
///
/// Un menu natif est un arbre, et `Menu::items` n'en rend que le premier niveau : les
/// entrées de thème vivent deux niveaux plus bas, sous « View » puis « Theme ». La
/// descente est bornée par la forme du menu, qu'on construit juste au-dessus.
fn find_check<R: Runtime>(items: &[MenuItemKind<R>], id: &str) -> Option<CheckMenuItem<R>> {
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

/// Les entrées de menu qu'Ash traite lui-même, et leur identifiant.
///
/// Une seule table pour tout l'espace des identifiants — `tab:*` et `view:*` —, parce que
/// c'en est **un seul** : `view:theme:light` et `view:toggle-sidebar` se disputeraient le
/// même préfixe s'ils étaient décidés à deux endroits. Toutes n'ont pas la même suite :
/// `ChooseTheme` est retenue ici, les autres partent vers la webview. Voir [`dispatch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    NewTab,
    NewHomeTab,
    CloseTab,
    ClearScrollback,
    /// Sélectionne le n-ième onglet, à partir de 1.
    SelectTab(u8),
    /// Replie ou déplie la sidebar — `Cmd+B`.
    ToggleSidebar,
    /// Choisit le thème de la fenêtre — la seule qui ne parte pas vers la webview.
    ChooseTheme(ThemeMode),
}

impl Action {
    fn id(self) -> String {
        match self {
            Action::NewTab => "tab:new".to_owned(),
            Action::NewHomeTab => "tab:new-home".to_owned(),
            Action::CloseTab => "tab:close".to_owned(),
            Action::ClearScrollback => "tab:clear".to_owned(),
            Action::SelectTab(position) => format!("tab:select:{position}"),
            Action::ToggleSidebar => "view:toggle-sidebar".to_owned(),
            Action::ChooseTheme(mode) => format!("view:theme:{}", mode.as_id()),
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "tab:new" => Some(Action::NewTab),
            "tab:new-home" => Some(Action::NewHomeTab),
            "tab:close" => Some(Action::CloseTab),
            "tab:clear" => Some(Action::ClearScrollback),
            "view:toggle-sidebar" => Some(Action::ToggleSidebar),
            other => match other.strip_prefix("view:theme:") {
                Some(mode) => ThemeMode::from_id(mode).map(Action::ChooseTheme),
                None => other
                    .strip_prefix("tab:select:")
                    .and_then(|position| position.parse().ok())
                    .filter(|position| (1..=DIRECT_TABS).contains(position))
                    .map(Action::SelectTab),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_menu_item_identifier_when_it_is_read_back_then_it_names_the_same_action() {
        // Given — l'identifiant est le contrat avec `src/app/menu.ts` ; il traverse
        // la frontière sous forme de chaîne et rien ne le vérifie à la compilation.
        let actions = [
            Action::NewTab,
            Action::NewHomeTab,
            Action::CloseTab,
            Action::ClearScrollback,
            Action::SelectTab(1),
            Action::SelectTab(9),
            Action::ToggleSidebar,
            Action::ChooseTheme(ThemeMode::Light),
            Action::ChooseTheme(ThemeMode::Dark),
            Action::ChooseTheme(ThemeMode::System),
        ];

        // When
        let round_trip: Vec<Option<Action>> =
            actions.iter().map(|a| Action::from_id(&a.id())).collect();

        // Then
        assert_eq!(round_trip, actions.map(Some).to_vec());
    }

    #[test]
    fn given_a_tab_position_beyond_the_nine_shortcuts_when_it_is_read_back_then_it_is_not_an_action(
    ) {
        // Given / When
        let tenth = Action::from_id("tab:select:10");

        // Then — la spec s'arrête à `Cmd+9` ; accepter au-delà ouvrirait une action que
        // rien ne peut déclencher, et que le frontend devrait pourtant gérer.
        assert_eq!(tenth, None);
    }

    #[test]
    fn given_a_theme_identifier_no_mode_carries_when_it_is_read_then_it_is_not_an_action() {
        // Given / When — `view:theme:` et `view:toggle-sidebar` partagent le même préfixe
        // de menu : un identifiant de thème inconnu ne doit ni être joué, ni retomber sur
        // la lecture d'une position d'onglet
        let unknown = Action::from_id("view:theme:solarized");

        // Then
        assert_eq!(unknown, None);
    }
}
