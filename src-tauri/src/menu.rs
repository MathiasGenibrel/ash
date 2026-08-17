//! Le menu applicatif macOS — source unique des raccourcis de la spec §4.4.
//!
//! Il vit à côté du composition root, pas dans une feature : un menu est un objet de
//! fenêtre, partagé par toutes les features, et aucune d'elles ne doit avoir à le
//! connaître. Ce module ne fait rien lui-même — il traduit un clic ou un accélérateur en
//! une **action**, et l'envoie à la fenêtre qui la joue. Un menu est global à
//! l'application, donc une action y naît sans surface : c'est [`route`] qui lui en donne
//! une, à partir de la fenêtre au premier plan.
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
//! **Une exception, et une seule : `Ctrl+Tab` et `Ctrl+Shift+Tab`.** `muda` traduit
//! `Key::Tab` en équivalent clavier `⇥` (U+21E5), le **glyphe** d'affichage, alors que
//! `NSEvent` rend `\t` (U+0009) quand on presse Tab. `-[NSMenu performKeyEquivalent:]`
//! compare ces chaînes : l'entrée s'affiche donc correctement, mais ne s'allume jamais au
//! clavier.
//!
//! **Ce qu'il faut revérifier en montant `muda`** — constaté sur **0.19.3** : la ligne
//! `Key::Tab => "⇥".into()` de `src/platform_impl/macos/accelerator.rs`. C'est la seule
//! de cette table à rendre un glyphe ; `Escape` y vaut `\u{1b}`, `Enter` `\r`,
//! `Backspace` `\u{8}` — les vrais caractères. Le jour où elle rendra `\u{9}`, les deux
//! entrées ci-dessous s'allumeront au clavier d'elles-mêmes, et `src/app/shortcuts.ts`
//! n'aura plus lieu d'être.
//!
//! La mesure qui l'établit se refait en quelques lignes de Swift — un `NSMenu` monté à
//! la main, un `NSEvent` synthétisé, `performKeyEquivalent:` :
//!
//! ```text
//! muda glyph U+21E5 + ctrl   vs Ctrl+Tab                -> matched=0
//! real tab U+0009  + ctrl    vs Ctrl+Tab                -> matched=1
//! real tab U+0009  + ctrl    vs plain Tab (no modifier) -> matched=0
//! ```
//!
//! La conséquence est heureuse pour nous : la touche traverse jusqu'à la webview, où
//! `src/app/shortcuts.ts` la capte — et c'est de toute façon le côté qu'il fallait pour
//! `Tab`, la seule touche de cette table dont le shell a un usage propre. La troisième
//! ligne de la mesure compte donc autant que les deux autres : un **accélérateur** ne se
//! confond pas avec une touche nue, `Tab` seul n'a pas le drapeau `Control`, donc ni
//! AppKit ni notre gestionnaire ne le retiennent, et la complétion de `zsh` reste
//! intacte.
//!
//! Le prix est que la liste des accélérateurs est en Rust et leur effet en TypeScript.
//! C'est assumé : le frontend rend les onglets, il ne les détient pas
//! ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)), et faire l'inverse —
//! des raccourcis côté webview — aurait donné deux chemins différents pour la souris et
//! pour le clavier.

use tauri::menu::{
    AboutMetadata, CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::features::settings::commands as settings;
use crate::features::theme::{commands as theme, FontStep, ThemeMode};

/// Nom de l'event qui porte l'action choisie. Contrat avec `src/app/menu.ts`.
const MENU_ACTION_EVENT: &str = "ash://menu-action";

/// Label de la **seule** fenêtre qui porte des onglets.
///
/// `tauri.conf.json` déclare la fenêtre principale sans la nommer : Tauri lui donne alors
/// `"main"`. C'est la surface de terminal, et la seule qui sache jouer une action d'onglet
/// — voir [`route`].
const MAIN_WINDOW: &str = "main";

/// Nombre d'onglets directement adressables — `Cmd+1` … `Cmd+9` (spec §4.4).
const DIRECT_TABS: u8 = 9;

/// Construit le menu de l'application.
///
/// `theme` est le mode retenu de la session précédente : le menu est construit avant que
/// la webview n'existe, et une coche posée après coup serait une seconde source de vérité.
pub fn build<R: Runtime>(app: &AppHandle<R>, theme_mode: ThemeMode) -> tauri::Result<Menu<R>> {
    // `Cmd+,` ouvre les réglages : c'est le raccourci que macOS attend dans le menu
    // applicatif, et le seul endroit où un utilisateur va le chercher. Il est écrit
    // `Cmd+Comma` parce que l'analyseur d'accélérateurs de Tauri lit des **noms** de
    // touches, pas des caractères.
    let settings_item = MenuItem::with_id(
        app,
        Action::OpenSettings.id(),
        "Settings…",
        true,
        Some("Cmd+Comma"),
    )?;

    let application = Submenu::with_items(
        app,
        "Ash",
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(AboutMetadata::default()))?,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
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

    // `Cmd+T`, et non `Cmd+N` : c'est le geste que macOS a posé pour « nouvel onglet »
    // dans Safari, Terminal.app, iTerm et Chrome, où `Cmd+N` ouvre une **fenêtre**. Ash
    // n'a pas de seconde fenêtre de terminal à ouvrir, donc `Cmd+N` ne fait plus rien
    // plutôt que de rester un doublon : deux conventions pour le même geste, c'est celle
    // qu'on ne connaît pas qui gagne.
    let new_tab = MenuItem::with_id(app, Action::NewTab.id(), "New Tab", true, Some("Cmd+T"))?;
    let new_home_tab = MenuItem::with_id(
        app,
        Action::NewHomeTab.id(),
        "New Tab at ~",
        true,
        Some("Cmd+Shift+T"),
    )?;
    let close_tab =
        MenuItem::with_id(app, Action::CloseTab.id(), "Close Tab", true, Some("Cmd+W"))?;

    // `Ctrl+Tab` / `Ctrl+Shift+Tab` : la convention des navigateurs et d'iTerm2 pour
    // circuler, là où `Cmd+1`…`Cmd+9` s'arrête à neuf et ne dit rien du « suivant ».
    //
    // **Ces deux accélérateurs-là ne sont pas joués par le menu**, contrairement à tous
    // les autres de ce module — voir la note d'en-tête. Ils figurent ici pour être vus
    // (⌃⇥ dans le menu) et cliquables à la souris ; la touche, elle, est captée par
    // `src/app/shortcuts.ts`. Les garder déclarés fait aussi que le jour où `muda`
    // corrigera son équivalent clavier, le chemin natif reprendra la main tout seul, sans
    // double déclenchement : un accélérateur capté par AppKit n'atteint jamais la webview.
    let next_tab = MenuItem::with_id(
        app,
        Action::NextTab.id(),
        "Select Next Tab",
        true,
        Some("Ctrl+Tab"),
    )?;
    let previous_tab = MenuItem::with_id(
        app,
        Action::PreviousTab.id(),
        "Select Previous Tab",
        true,
        Some("Ctrl+Shift+Tab"),
    )?;
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
        &next_tab,
        &previous_tab,
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
    // La taille de police du terminal — `Cmd++`, `Cmd+-`, `Cmd+0`.
    //
    // Dans « View » et non dans « Terminal », parce que c'est un réglage de **toute
    // l'application** et non de l'onglet actif : voir `features::theme::FontSize`, qui
    // porte cette décision et ses bornes. Les trois entrées existent en permanence, même
    // quand la taille est déjà à une borne — une entrée qui s'active et se désactive au
    // fil des frappes ferait vivre l'état de la taille à deux endroits, et le pas sans
    // effet est déjà ignoré par la feature.
    //
    // **`Cmd+0` est libre** : les positions d'onglet s'arrêtent à `Cmd+9` (`DIRECT_TABS`),
    // et `Action::from_id` refuse `tab:select:0`.
    //
    // L'accélérateur de « Bigger » s'écrit `Cmd+NumpadAdd`, et ce n'est pas le pavé
    // numérique : sur macOS, un accélérateur de menu est un **caractère**, pas une touche
    // physique, et `NumpadAdd` est le seul nom que l'analyseur de `muda` traduit en `+`.
    // L'entrée affiche donc « ⌘+ » et répond au `+` du clavier principal — celui qui se
    // tape avec ⇧, comme dans tous les navigateurs.
    //
    // **Limite connue, et à laisser visible : `Cmd+=` n'est pas lié.** C'est pourtant la
    // touche que la moitié des gens presse pour agrandir, parce qu'elle porte le `+` sans ⇧
    // sur un clavier américain — et sur un AZERTY, ni `+` ni `=` ne sont là où on les
    // imagine. AppKit sait donner un second key equivalent à une entrée (`alternate item`) ;
    // `muda` ne l'expose pas, donc un second accélérateur demanderait de descendre à AppKit
    // sous l'arbre que `muda` construit. À rouvrir si la plainte remonte, pas avant.
    let font_sizes: Vec<MenuItem<R>> = FontStep::ALL
        .into_iter()
        .map(|step| {
            MenuItem::with_id(
                app,
                Action::ResizeFont(step).id(),
                step.label(),
                true,
                Some(match step {
                    FontStep::Bigger => "Cmd+NumpadAdd",
                    FontStep::Smaller => "Cmd+Minus",
                    FontStep::Default => "Cmd+0",
                }),
            )
        })
        .collect::<tauri::Result<_>>()?;

    // Les trois thèmes, en coches exclusives. C'est le **seul** point d'entrée du choix à
    // ce jalon : la fenêtre de réglages existe, mais sa section `appearance` est l'issue
    // #22 — elle n'y montre pour l'instant que d'où le thème se choisit.
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

    let view_separator = PredefinedMenuItem::separator(app)?;
    let mut view_items: Vec<&dyn tauri::menu::IsMenuItem<R>> =
        vec![&toggle_sidebar, &view_separator];
    view_items.extend(
        font_sizes
            .iter()
            .map(|item| item as &dyn tauri::menu::IsMenuItem<R>),
    );
    view_items.push(&view_separator);
    view_items.push(&theme_menu);

    let view = Submenu::with_items(app, "View", true, &view_items)?;

    // Pas de « Close Window » ici : son `Cmd+W` prendrait le pas sur celui des onglets.
    // C'est `route` qui rend `Cmd+W` juste devant une fenêtre sans onglets — la même
    // entrée « Close Tab », acheminée vers la surface au premier plan.
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

/// Traduit un item de menu en action, décide **qui** la reçoit, et la lui donne.
///
/// Trois chemins, et les différences ne sont pas des détails :
///
/// - le thème et la taille de police sont des **états**, retenus par `features::theme`
///   avant d'être annoncés ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)) —
///   une bascule qui ne vivrait que dans la webview serait perdue à la première seconde
///   fenêtre ;
/// - les actions d'onglet partent vers **une** webview, celle qui détient les onglets ;
/// - `Cmd+W` sur une fenêtre sans onglets ferme cette fenêtre-là.
///
/// La règle de destination est dans [`route`], qui est pure et testée : ce gestionnaire
/// n'a pas de test unitaire, et n'en aura pas.
///
/// Un identifiant inconnu est ignoré : les items prédéfinis (copier, quitter…) sont
/// traités par le système et ne passent pas par ici.
pub fn dispatch<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let Some(action) = Action::from_id(id) else {
        return;
    };

    // `Manager::get_focused_window` est derrière la feature `unstable` de Tauri : on lit le
    // focus sur les fenêtres qu'on a, ce qui est l'API stable équivalente. Une fenêtre dont
    // l'état de focus n'est pas lisible est traitée comme non focalisée — mieux vaut ne rien
    // router que router au hasard, voir [`route`].
    let focused = app
        .webview_windows()
        .into_iter()
        .find(|(_, window)| window.is_focused().unwrap_or(false));

    match route(action, focused.as_ref().map(|(label, _)| label.as_str())) {
        Route::Backend => match action {
            Action::ChooseTheme(mode) => {
                theme::choose(app, mode);
                // **Toujours**, et pas seulement quand le mode a changé : un `CheckMenuItem`
                // bascule sa propre coche au clic. Cliquer l'entrée déjà cochée la décocherait
                // donc, et le menu n'aurait plus aucun mode coché.
                check_only(app, mode);
            }
            Action::ResizeFont(step) => theme::resize_terminal_font(app, step),
            // Une fenêtre est un objet du backend, comme le thème : l'ouvrir depuis la webview
            // demanderait à la fenêtre principale d'exister pour que la seconde puisse naître.
            Action::OpenSettings => settings::open(app),
            // `route` ne rend `Backend` que pour les trois ci-dessus.
            _ => {}
        },
        // L'échec d'émission signifie qu'il n'y a plus de webview à prévenir : rien à
        // rattraper, et surtout pas de panique dans un gestionnaire d'event.
        Route::Webview(label) => {
            let _ = app.emit_to(label, MENU_ACTION_EVENT, action.id());
        }
        // Fermer, et non cacher : la fenêtre de réglages est construite à l'exécution, donc
        // `settings::open` la refait à la demande suivante — c'est la décision de
        // `features::settings::commands::open`, et rien n'a à porter un état « ouverte ».
        Route::CloseWindow(_) => {
            if let Some((_, window)) = focused.as_ref() {
                let _ = window.close();
            }
        }
        Route::Nowhere => {}
    }
}

/// Où va une action de menu, sachant quelle fenêtre est au premier plan.
///
/// C'est ici qu'est la correction de #107. `AppHandle::emit` **diffuse à toutes les
/// webviews** : les réglages devant, `Cmd+W` fermait donc un onglet de la fenêtre
/// principale, derrière, hors de vue — et y posait sa confirmation si un agent y tournait.
/// Un raccourci qui détruit une surface invisible est pire que l'absence de raccourci.
///
/// La réponse n'est **pas** une entrée « Close Window » dans le menu : son `Cmd+W`
/// prendrait le pas sur celui des onglets, et `Cmd+W` ferme un onglet dans tout émulateur
/// de terminal (voir l'en-tête de ce module). C'est la même action qui est routée selon la
/// surface qui la reçoit.
///
/// Trois règles, et une seule est sensible au focus :
///
/// 1. le thème, la taille de police et l'ouverture des réglages sont des préférences de
///    **l'application** : elles se jouent en Rust, quelle que soit la fenêtre devant ;
/// 2. `CloseTab` appartient à la fenêtre du **premier plan** — la principale y ferme son
///    onglet actif, une autre se ferme elle-même, et un premier plan inconnu ne ferme rien ;
/// 3. les autres actions d'onglet (`Cmd+T`, `Cmd+K`, `Cmd+1`…`Cmd+9`) vont à la fenêtre
///    principale, seule surface qui porte des onglets. Elles ne détruisent rien : les
///    router selon le focus les rendrait sans effet depuis les réglages, ce que personne
///    n'a demandé.
fn route(action: Action, focused: Option<&str>) -> Route<'_> {
    match action {
        Action::ChooseTheme(_) | Action::ResizeFont(_) | Action::OpenSettings => Route::Backend,
        Action::CloseTab => match focused {
            Some(MAIN_WINDOW) => Route::Webview(MAIN_WINDOW),
            Some(other) => Route::CloseWindow(other),
            // Aucune fenêtre devant : fermer l'onglet actif de la principale serait
            // détruire ce que personne ne regarde.
            None => Route::Nowhere,
        },
        _ => Route::Webview(MAIN_WINDOW),
    }
}

/// La destination d'une action de menu. Voir [`route`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route<'a> {
    /// Jouée en Rust : c'est un état de l'application, pas un ordre d'affichage.
    Backend,
    /// Émise à **une** webview, nommée par son label — jamais diffusée.
    Webview(&'a str),
    /// Ferme la fenêtre au premier plan, nommée par son label.
    CloseWindow(&'a str),
    /// Personne ne joue cette action : la surface qu'elle viserait n'est pas devant.
    Nowhere,
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
    /// `Ctrl+Tab` : l'onglet suivant, en bouclant après le dernier.
    NextTab,
    /// `Ctrl+Shift+Tab` : l'onglet précédent, en bouclant avant le premier.
    PreviousTab,
    /// Replie ou déplie la sidebar — `Cmd+B`.
    ToggleSidebar,
    /// Choisit le thème de la fenêtre — l'une de celles qui ne partent pas vers la webview.
    ChooseTheme(ThemeMode),
    /// Change la taille de police du terminal — `Cmd++`, `Cmd+-`, `Cmd+0`. Retenue en
    /// Rust comme le thème : c'est un état, pas un ordre d'affichage.
    ResizeFont(FontStep),
    /// Ouvre la fenêtre de réglages — `Cmd+,`. Traitée ici, comme le thème.
    OpenSettings,
}

impl Action {
    fn id(self) -> String {
        match self {
            Action::NewTab => "tab:new".to_owned(),
            Action::NewHomeTab => "tab:new-home".to_owned(),
            Action::CloseTab => "tab:close".to_owned(),
            Action::ClearScrollback => "tab:clear".to_owned(),
            Action::SelectTab(position) => format!("tab:select:{position}"),
            Action::NextTab => "tab:next".to_owned(),
            Action::PreviousTab => "tab:previous".to_owned(),
            Action::ToggleSidebar => "view:toggle-sidebar".to_owned(),
            Action::ChooseTheme(mode) => format!("view:theme:{}", mode.as_id()),
            Action::ResizeFont(step) => format!("view:font:{}", step.as_id()),
            Action::OpenSettings => "app:settings".to_owned(),
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "tab:new" => Some(Action::NewTab),
            "tab:new-home" => Some(Action::NewHomeTab),
            "tab:close" => Some(Action::CloseTab),
            "tab:clear" => Some(Action::ClearScrollback),
            "tab:next" => Some(Action::NextTab),
            "tab:previous" => Some(Action::PreviousTab),
            "view:toggle-sidebar" => Some(Action::ToggleSidebar),
            "app:settings" => Some(Action::OpenSettings),
            other => match (
                other.strip_prefix("view:theme:"),
                other.strip_prefix("view:font:"),
            ) {
                (Some(mode), _) => ThemeMode::from_id(mode).map(Action::ChooseTheme),
                (_, Some(step)) => FontStep::from_id(step).map(Action::ResizeFont),
                _ => other
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
            Action::NextTab,
            Action::PreviousTab,
            Action::ToggleSidebar,
            Action::ChooseTheme(ThemeMode::Light),
            Action::ChooseTheme(ThemeMode::Dark),
            Action::ChooseTheme(ThemeMode::System),
            Action::ResizeFont(FontStep::Bigger),
            Action::ResizeFont(FontStep::Smaller),
            Action::ResizeFont(FontStep::Default),
            Action::OpenSettings,
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

    #[test]
    fn given_the_settings_window_in_front_when_close_tab_is_chosen_then_that_window_is_closed() {
        // Given — les réglages devant, un onglet de la fenêtre principale derrière (#107)
        let focused = Some("settings");

        // When
        let destination = route(Action::CloseTab, focused);

        // Then — la fenêtre du premier plan se ferme, et **rien** ne part vers la webview
        // qui porte les onglets : c'est ce qui fermait un onglet hors de vue, et y posait sa
        // confirmation si un agent y tournait
        assert_eq!(destination, Route::CloseWindow("settings"));
    }

    #[test]
    fn given_the_main_window_in_front_when_close_tab_is_chosen_then_it_reaches_the_tabs() {
        // Given
        let focused = Some(MAIN_WINDOW);

        // When
        let destination = route(Action::CloseTab, focused);

        // Then — `Cmd+W` ferme un onglet, comme dans tout émulateur de terminal, et par un
        // envoi **ciblé** : `emit` diffuserait à toutes les webviews
        assert_eq!(destination, Route::Webview("main"));
    }

    #[test]
    fn given_no_window_in_front_when_close_tab_is_chosen_then_nothing_is_closed() {
        // Given — toutes les fenêtres réduites, le menu applicatif reste atteignable
        let focused = None;

        // When
        let destination = route(Action::CloseTab, focused);

        // Then — un raccourci qui détruit une surface invisible est pire que l'absence de
        // raccourci
        assert_eq!(destination, Route::Nowhere);
    }

    #[test]
    fn given_the_settings_window_in_front_when_a_theme_is_chosen_then_it_is_still_played_in_rust() {
        // Given — le thème est une préférence de l'application, et les réglages sont devant
        let focused = Some("settings");

        // When
        let destination = route(Action::ChooseTheme(ThemeMode::Dark), focused);

        // Then — il reste retenu par le backend, qui repeint les deux fenêtres
        // ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md))
        assert_eq!(destination, Route::Backend);
    }

    #[test]
    fn given_the_settings_window_in_front_when_a_new_tab_is_asked_then_the_main_window_gets_it() {
        // Given — la seule action sensible au focus est `CloseTab`, parce qu'elle est la
        // seule à détruire quelque chose
        let focused = Some("settings");

        // When
        let destinations = [
            route(Action::NewTab, focused),
            route(Action::ClearScrollback, focused),
            route(Action::SelectTab(1), focused),
        ];

        // Then — `Cmd+T`, `Cmd+K` et `Cmd+1` gardent leur effet sur la fenêtre à onglets
        assert_eq!(destinations, [Route::Webview("main"); 3]);
    }

    #[test]
    fn given_a_font_identifier_no_step_carries_when_it_is_read_then_it_is_not_an_action() {
        // Given / When — `view:font:`, `view:theme:` et `view:toggle-sidebar` partagent le
        // même préfixe : un pas inconnu ne doit ni changer la taille, ni retomber sur une
        // autre action de « View »
        let unknown = Action::from_id("view:font:huge");

        // Then
        assert_eq!(unknown, None);
    }
}
