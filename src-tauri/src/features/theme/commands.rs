//! La surface de la feature vers le frontend : six commandes, trois events.
//!
//! Le frontend ne connaît de l'apparence que ces noms, les trois identifiants de mode et les
//! trois pas de taille. Il **rend** la palette ; les choix, eux, sont ici
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! Ce que ce module ne contient **pas**, et c'est délibéré : le menu natif. Ses
//! identifiants d'entrée et ses coches vivent dans `src-tauri/src/menu.rs`, avec le code
//! qui construit l'arbre — une feature n'a pas à connaître la forme du menu. C'est pour cette
//! raison que le **choix** de thème n'a pas de commande ici alors que le pas de taille en a
//! une : une bascule de thème doit corriger trois coches, un pas de taille ne touche rien du
//! menu. Voir `menu::theme_set_mode`.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::density::SidebarDensity;
use super::font::{FontCatalog, TerminalFont};
use super::font_size::{FontSize, FontStep};
use super::mode::ThemeMode;
use super::sidebar_column::{SidebarColumn, SidebarWidth};
use super::state::ThemeState;

/// Nom de l'event qui porte le mode choisi. Contrat avec `src/app/theme.ts`.
pub const THEME_MODE_EVENT: &str = "ash://theme-mode";

/// Nom de l'event qui porte la taille de police. Contrat avec `src/app/font-size.ts`.
pub const TERMINAL_FONT_SIZE_EVENT: &str = "ash://terminal-font-size";

/// Nom de l'event qui porte la largeur et le repli de la colonne. Contrat avec
/// `src/app/sidebar-column.ts`.
pub const SIDEBAR_COLUMN_EVENT: &str = "ash://sidebar-column";

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
    if !state.set_mode(mode) {
        return false;
    }

    // Échouer à émettre signifie qu'il n'y a plus de webview à prévenir : rien à
    // rattraper, et surtout pas de panique dans un gestionnaire d'event de menu.
    let _ = app.emit(THEME_MODE_EVENT, mode);
    true
}

/// La taille de police du terminal, lue par la webview en s'affichant.
///
/// Ensuite, c'est l'event qui la tient à jour : elle ne redemande jamais. Même contrat que
/// [`theme_mode`], parce que c'est la même sorte de préférence.
#[tauri::command]
pub fn terminal_font_size(state: tauri::State<'_, Arc<ThemeState>>) -> FontSize {
    state.font_size()
}

/// Le pas de taille de police demandé par la fenêtre de réglages — la **seconde surface** du
/// même état.
///
/// Elle demande un **pas**, comme le menu, et pour la même raison : les bornes et la valeur
/// courante sont à `FontSize`, et une fenêtre qui enverrait un nombre en deviendrait le
/// second détenteur ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
///
/// Rien n'est rendu à l'appelante : elle apprend la nouvelle taille par
/// [`TERMINAL_FONT_SIZE_EVENT`], que Tauri diffuse à **toutes** les fenêtres. C'est le même
/// chemin qu'un `Cmd++`, donc la fenêtre principale réajuste la grille de ses terminaux comme
/// elle le fait déjà — et les deux surfaces ne peuvent pas diverger.
///
/// Contrairement au thème, ce pas-ci n'a aucune coche de menu à corriger : les trois entrées
/// de « View » ne portent pas d'état, elles jouent un pas.
#[tauri::command]
pub fn step_terminal_font_size<R: Runtime>(app: AppHandle<R>, step: FontStep) {
    resize_terminal_font(&app, step);
}

/// Joue un pas de taille de police et l'annonce à la webview.
///
/// Rien n'est émis quand la taille n'a pas bougé — une borne atteinte, ou `Cmd+0` sur une
/// taille déjà par défaut : chaque annonce fait relire la grille à **tous** les terminaux
/// ouverts, et la faire pour rien serait un `SIGWINCH` gratuit dans chaque PTY.
pub fn resize_terminal_font<R: Runtime>(app: &AppHandle<R>, step: FontStep) {
    let Some(state) = app.try_state::<Arc<ThemeState>>() else {
        return;
    };
    let Some(size) = state.step_font(step) else {
        return;
    };

    let _ = app.emit(TERMINAL_FONT_SIZE_EVENT, size);
}

/// La largeur et le repli de la colonne, lus par la webview en s'affichant.
///
/// Ensuite, c'est l'event qui la tient à jour : elle ne redemande jamais. Même contrat que
/// [`theme_mode`] et [`terminal_font_size`], parce que c'est la même sorte de préférence.
#[tauri::command]
pub fn sidebar_column(state: tauri::State<'_, Arc<ThemeState>>) -> SidebarColumn {
    state.sidebar_column()
}

/// La largeur que la webview vient de régler en relâchant le bord.
///
/// **C'est le seul endroit où un nombre traverse la frontière dans ce sens**, et c'est assumé
/// : contrairement à la taille de police, les bornes de la colonne sont relatives à la fenêtre
/// — 10 % à 80 % —, et le backend ne connaît pas la fenêtre. La webview applique donc la règle
/// de mise en page, et annonce le résultat ; ce qui **survit** à la fermeture reste ici, et
/// nulle part ailleurs ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
///
/// Elle n'est appelée qu'au **relâchement**, jamais pendant le glissement : la largeur qui
/// suit le pointeur est un fait d'affichage, comme le compteur de durée de la ligne de statut,
/// et un aller-retour Tauri par image ferait écrire le fichier soixante fois par seconde.
#[tauri::command]
pub fn set_sidebar_column_width<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, Arc<ThemeState>>,
    width: i64,
) {
    announce(&app, state.set_sidebar_width(SidebarWidth::from(width)));
}

/// Replie ou déplie la colonne — le geste du séparateur, et le relâchement sous le plancher.
#[tauri::command]
pub fn set_sidebar_column_collapsed<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, Arc<ThemeState>>,
    collapsed: bool,
) {
    announce(&app, state.set_sidebar_collapsed(collapsed));
}

/// `⌘B` : replie la colonne, ou la rouvre à la largeur qu'elle avait.
///
/// La bascule est demandée **sans état** : la webview ne dit pas ce que la colonne devient,
/// elle demande qu'elle change. C'est ce qui laisse un seul détenteur au repli, et ce qui
/// empêche deux surfaces — le menu, la touche du séparateur — de se contredire.
#[tauri::command]
pub fn toggle_sidebar_column<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, Arc<ThemeState>>,
) {
    announce(&app, Some(state.toggle_sidebar_collapsed()));
}

/// Annonce la colonne, et seulement si elle a bougé.
///
/// Comme pour le thème et la taille : chaque annonce fait redessiner la colonne et relire sa
/// grille au terminal visible, et la faire pour rien serait un `SIGWINCH` gratuit.
fn announce<R: Runtime>(app: &AppHandle<R>, changed: Option<SidebarColumn>) {
    let Some(column) = changed else {
        return;
    };
    // Échouer à émettre signifie qu'il n'y a plus de webview à prévenir : rien à rattraper,
    // et surtout pas de panique dans une commande.
    let _ = app.emit(SIDEBAR_COLUMN_EVENT, column);
}

/// Nom de l'event qui porte la police du terminal. Contrat avec `src/app/terminal-font.ts`.
pub const TERMINAL_FONT_EVENT: &str = "ash://terminal-font";

/// Nom de l'event qui porte la densité de la sidebar. Contrat avec
/// `src/app/sidebar-density.ts`.
pub const SIDEBAR_DENSITY_EVENT: &str = "ash://sidebar-density";

/// La police du terminal, lue par la webview en s'affichant. Même contrat que
/// [`terminal_font_size`] : ensuite, c'est l'event qui la tient à jour.
#[tauri::command]
pub fn terminal_font(state: tauri::State<'_, Arc<ThemeState>>) -> TerminalFont {
    state.font()
}

/// Les familles monospace dans lesquelles la fenêtre de réglages fait choisir.
///
/// **Une lecture, et seulement une lecture** : rien n'est écrit, aucune autorisation macOS
/// n'est demandée, et le catalogue ne sort pas des trois dossiers de polices du système. La
/// liste est calculée une fois par session — installer une police demande de passer par le
/// Livre des polices, pas par Ash.
#[tauri::command]
pub fn monospace_fonts(catalog: tauri::State<'_, Arc<dyn FontCatalog>>) -> Vec<String> {
    catalog.monospace_families()
}

/// La police que la fenêtre de réglages choisit — la **seconde** préférence d'apparence qui
/// n'a qu'une surface, avec la densité.
///
/// Contrairement à la taille, ce qui traverse est une **valeur** et non un pas : le choix se
/// fait dans une liste que le backend a lui-même rendue, et il n'existe pas de « police
/// suivante ». Le nom est repris par [`TerminalFont`], qui est la seule porte d'entrée du
/// type — un nom vide ou démesuré ne devient jamais une préférence.
///
/// Rien n'est rendu à l'appelante : la nouvelle police revient par [`TERMINAL_FONT_EVENT`],
/// que Tauri diffuse à **toutes** les fenêtres. C'est ce qui fait relire sa grille à chaque
/// terminal ouvert, exactement comme un pas de taille.
#[tauri::command]
pub fn choose_terminal_font<R: Runtime>(app: AppHandle<R>, font: String) {
    let Some(chosen) = TerminalFont::new(&font) else {
        return;
    };
    let Some(state) = app.try_state::<Arc<ThemeState>>() else {
        return;
    };
    if !state.set_font(chosen.clone()) {
        return;
    }
    let _ = app.emit(TERMINAL_FONT_EVENT, chosen);
}

/// La densité de la sidebar, lue par la webview en s'affichant.
#[tauri::command]
pub fn sidebar_density(state: tauri::State<'_, Arc<ThemeState>>) -> SidebarDensity {
    state.density()
}

/// La densité que la fenêtre de réglages choisit.
///
/// Même chemin que la police, et pour la même raison : la sidebar de la fenêtre principale
/// l'apprend par l'annonce, pas par cette fenêtre-ci — les deux documents sont distincts, et
/// c'est le backend qui détient le choix
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[tauri::command]
pub fn choose_sidebar_density<R: Runtime>(app: AppHandle<R>, density: SidebarDensity) {
    let Some(state) = app.try_state::<Arc<ThemeState>>() else {
        return;
    };
    if !state.set_density(density) {
        return;
    }
    let _ = app.emit(SIDEBAR_DENSITY_EVENT, density);
}
