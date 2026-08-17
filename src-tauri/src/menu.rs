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
//!
//! **Ce module a donc deux lecteurs, et un seul jeu de valeurs pour les deux.** Le menu
//! natif se construit à partir de [`descriptor`], et la section `shortcuts` de la fenêtre de
//! réglages lit la même table par [`menu_shortcuts`] : une combinaison recopiée en
//! TypeScript aurait fini par annoncer un raccourci que le menu ne déclare plus, et c'est
//! l'écran des réglages qu'on croit quand les deux ne disent pas la même chose.
//!
//! Il porte aussi [`theme_set_mode`], la **seconde surface** du choix de thème. Elle est ici
//! et non dans `features::theme` parce qu'un choix venu de la fenêtre de réglages doit
//! corriger la coche du menu, et qu'une feature n'a pas à connaître la forme d'un menu — la
//! remarque de [`check_only`] vaut mot pour mot pour elle.

use std::borrow::Cow;

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
///
/// Ce nom-là n'est pas seulement une valeur par défaut de bibliothèque : c'est le contrat
/// avec `src-tauri/capabilities/default.json`, qui accorde ses permissions à `"main"` et à
/// elle seule — comme `settings.json` le fait pour
/// [`crate::features::settings::commands::SETTINGS_WINDOW`]. Renommer la fenêtre sans y
/// toucher lui ôterait toutes ses permissions, donc l'écart ne peut pas passer inaperçu.
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
    // touches, pas des caractères — voir [`descriptor`], où il est déclaré.
    let settings_item = item(app, Action::OpenSettings)?;

    // Le menu applicatif porte le nom du binaire courant, pas un littéral : en debug il dit
    // « Ash-dev », et c'est souvent le seul endroit où l'on voit d'un coup d'œil laquelle
    // des deux instances a le clavier (voir [`crate::APP_NAME`]).
    let application = Submenu::with_items(
        app,
        crate::APP_NAME,
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
    let new_tab = item(app, Action::NewTab)?;
    let new_home_tab = item(app, Action::NewHomeTab)?;
    let close_tab = item(app, Action::CloseTab)?;

    // `Ctrl+Tab` / `Ctrl+Shift+Tab` : la convention des navigateurs et d'iTerm2 pour
    // circuler, là où `Cmd+1`…`Cmd+9` s'arrête à neuf et ne dit rien du « suivant ».
    //
    // **Ces deux accélérateurs-là ne sont pas joués par le menu**, contrairement à tous
    // les autres de ce module — voir la note d'en-tête. Ils figurent ici pour être vus
    // (⌃⇥ dans le menu) et cliquables à la souris ; la touche, elle, est captée par
    // `src/app/shortcuts.ts`. Les garder déclarés fait aussi que le jour où `muda`
    // corrigera son équivalent clavier, le chemin natif reprendra la main tout seul, sans
    // double déclenchement : un accélérateur capté par AppKit n'atteint jamais la webview.
    let next_tab = item(app, Action::NextTab)?;
    let previous_tab = item(app, Action::PreviousTab)?;
    let clear = item(app, Action::ClearScrollback)?;

    // Les neuf entrées existent en permanence, même quand il y a moins d'onglets : une
    // action qui ne désigne personne est ignorée côté webview. Les activer et les
    // désactiver au fil des ouvertures ferait vivre l'état des onglets à deux endroits.
    let select: Vec<MenuItem<R>> = (1..=DIRECT_TABS)
        .map(|position| item(app, Action::SelectTab(position)))
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
    let toggle_sidebar = item(app, Action::ToggleSidebar)?;
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
        .map(|step| item(app, Action::ResizeFont(step)))
        .collect::<tauri::Result<_>>()?;

    // Les trois thèmes, en coches exclusives. Ce n'est plus le seul point d'entrée du choix
    // — la section `appearance` de la fenêtre de réglages en est le second, par
    // [`theme_set_mode`] — mais c'est toujours le **même état** qui est choisi, et ces coches
    // le suivent d'où qu'il change ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    // Pas d'accélérateur — un thème se change une fois par saison, pas une fois par heure,
    // et chaque raccourci pris ici est un raccourci perdu pour le shell.
    let themes: Vec<CheckMenuItem<R>> = ThemeMode::ALL
        .into_iter()
        .map(|mode| {
            let shown = descriptor(Action::ChooseTheme(mode));
            CheckMenuItem::with_id(
                app,
                Action::ChooseTheme(mode).id(),
                shown.label.as_ref(),
                true,
                mode == theme_mode,
                shown.accelerator.as_deref(),
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

/// Ce que le menu affiche d'une action, et où il la range.
///
/// C'est la table unique du module, et elle est **exhaustive par construction** : le `match`
/// de [`descriptor`] couvre tout [`Action`], donc une action de plus ne compile pas tant
/// qu'elle n'a pas dit son libellé, son groupe et son accélérateur. C'est ce qui laisse la
/// fenêtre de réglages lire les raccourcis au lieu d'en tenir une seconde liste.
struct Descriptor {
    /// Le sous-menu où l'action vit, en un mot — et le groupe sous lequel les réglages la
    /// listent. Les réglages ne le traduisent pas : un titre de groupe écrit là-bas
    /// finirait par ranger sous « terminal » une entrée que le menu a déplacée.
    group: &'static str,
    /// Le libellé, dans la capitalisation de macOS pour un menu.
    label: Cow<'static, str>,
    /// L'accélérateur au format de `muda`, ou `None` pour une entrée sans raccourci.
    accelerator: Option<Cow<'static, str>>,
}

/// Le libellé, le groupe et l'accélérateur d'une action — **la** table du module.
///
/// Les noms de touches sont ceux de l'analyseur de Tauri, qui lit des **noms** et non des
/// caractères : d'où `Cmd+Comma`, `Cmd+Minus`, et `Cmd+NumpadAdd` pour le `+` du clavier
/// principal (voir la note au-dessus des entrées de taille de police dans [`build`]).
/// [`keys`] est ce qui les rend tels que macOS les écrit.
fn descriptor(action: Action) -> Descriptor {
    let (group, label, accelerator) = match action {
        Action::OpenSettings => ("application", "Settings…", Some("Cmd+Comma")),
        Action::NewTab => ("terminal", "New Tab", Some("Cmd+T")),
        Action::NewHomeTab => ("terminal", "New Tab at ~", Some("Cmd+Shift+T")),
        Action::CloseTab => ("terminal", "Close Tab", Some("Cmd+W")),
        Action::NextTab => ("terminal", "Select Next Tab", Some("Ctrl+Tab")),
        Action::PreviousTab => ("terminal", "Select Previous Tab", Some("Ctrl+Shift+Tab")),
        Action::ClearScrollback => ("terminal", "Clear Scrollback", Some("Cmd+K")),
        Action::ToggleSidebar => ("view", "Toggle Sidebar", Some("Cmd+B")),
        // Les deux seules entrées dont le libellé et l'accélérateur se calculent : la
        // position d'onglet est dans les deux, et `DIRECT_TABS` décide combien il y en a.
        Action::SelectTab(position) => {
            return Descriptor {
                group: "terminal",
                label: Cow::Owned(format!("Tab {position}")),
                accelerator: Some(Cow::Owned(format!("Cmd+{position}"))),
            }
        }
        Action::ResizeFont(step) => (
            "view",
            step.label(),
            Some(match step {
                FontStep::Bigger => "Cmd+NumpadAdd",
                FontStep::Smaller => "Cmd+Minus",
                FontStep::Default => "Cmd+0",
            }),
        ),
        // Pas d'accélérateur, donc pas de ligne dans la section `shortcuts` : ce n'en est
        // pas un, et en inventer un prendrait une touche au shell pour rien.
        Action::ChooseTheme(mode) => ("view", mode.label(), None),
    };

    Descriptor {
        group,
        label: Cow::Borrowed(label),
        accelerator: accelerator.map(Cow::Borrowed),
    }
}

/// Une entrée de menu, telle que [`descriptor`] la décrit.
fn item<R: Runtime>(app: &AppHandle<R>, action: Action) -> tauri::Result<MenuItem<R>> {
    let shown = descriptor(action);
    MenuItem::with_id(
        app,
        action.id(),
        shown.label.as_ref(),
        true,
        shown.accelerator.as_deref(),
    )
}

/// Un raccourci tel que le menu le déclare — une ligne de la section `shortcuts` (spec §4.4).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Shortcut {
    /// Le sous-menu où l'action vit : c'est ce qui **groupe** la liste.
    pub group: String,
    pub label: String,
    /// La combinaison, écrite comme macOS l'écrit — `⇧⌘T`, `⌃⇥`, `⌘+`.
    pub keys: String,
}

/// Les raccourcis que **cette** application déclare, pour la fenêtre de réglages (spec §4.4).
///
/// La liste est celle du menu, jamais une copie : c'est le sens de la remarque d'en-tête sur
/// les deux lecteurs de ce module. Elle est donc plus courte que le tableau de la spec — le
/// groupe git (`Cmd+Ctrl+B`, `G`, `W`, `M`, `I`) n'a pas encore d'entrée de menu, donc pas
/// encore d'effet, et annoncer un raccourci qui ne fait rien serait exactement le mensonge
/// que la lecture depuis la source évite.
///
/// **Elle se déduit de [`Action::every`], et n'est pas une seconde liste** : une action de plus
/// dans le menu apparaît donc ici sans qu'on y pense, là où une table à tenir à la main aurait
/// laissé un raccourci déclaré et pourtant invisible dans l'écran qui prétend les montrer tous.
/// Ce qui la raccourcit tient en deux règles, pas en une sélection :
///
/// - une action sans accélérateur n'a pas de ligne — c'est [`shortcut`] qui le dit, et ça vaut
///   pour les trois thèmes ;
/// - la famille des positions d'onglet n'en a qu'**une**, composée de ses deux extrémités
///   (`Tab 1 … Tab 9`, `⌘1 … ⌘9`) : la spec §4.4 l'écrit ainsi, et neuf lignes identiques à un
///   rang près feraient perdre les huit autres raccourcis dans la liste. Les trois pas de
///   taille de police, eux, gardent une ligne chacun — ils n'ont pas le même effet.
#[tauri::command]
pub fn menu_shortcuts() -> Vec<Shortcut> {
    Action::every()
        .into_iter()
        .filter(|action| !matches!(action, Action::SelectTab(position) if *position != 1))
        .filter_map(shortcut)
        .collect()
}

/// Une ligne de la liste, ou `None` quand l'action n'a pas de raccourci.
fn shortcut(action: Action) -> Option<Shortcut> {
    let shown = descriptor(action);
    let written = keys(shown.accelerator.as_deref()?);

    // La famille des positions se lit d'un bout à l'autre : « Tab 1 … Tab 9 », « ⌘1 … ⌘9 ».
    let family = match action {
        Action::SelectTab(_) => Some(descriptor(Action::SelectTab(DIRECT_TABS))),
        _ => None,
    };

    Some(match family {
        Some(last) => Shortcut {
            group: shown.group.to_owned(),
            label: format!("{} … {}", shown.label, last.label),
            keys: format!("{written} … {}", keys(last.accelerator.as_deref()?)),
        },
        None => Shortcut {
            group: shown.group.to_owned(),
            label: shown.label.into_owned(),
            keys: written,
        },
    })
}

/// Un accélérateur de `muda`, écrit comme macOS l'écrit dans un menu.
///
/// Deux raisons de le faire ici plutôt que dans la fenêtre de réglages. La première est que
/// l'ordre des modificateurs est **celui de macOS** — `⌃⌥⇧⌘` — et non celui dans lequel
/// l'accélérateur les déclare : `Cmd+Shift+T` s'écrit `⇧⌘T`. La seconde est `NumpadAdd`, qui
/// n'est pas le pavé numérique mais le seul nom que l'analyseur de `muda` traduise en `+`
/// (voir la note de [`build`]) : cette bizarrerie est documentée ici, et une table de
/// traduction en TypeScript en serait la seconde copie.
fn keys(accelerator: &str) -> String {
    // Le dernier segment est la touche ; ce qui précède sont les modificateurs. Un
    // accélérateur sans modificateur n'a pas de `+`, et vaut alors la touche seule.
    let (modifiers, key) = match accelerator.rsplit_once('+') {
        Some((modifiers, key)) => (modifiers, key),
        None => ("", accelerator),
    };

    let mut written = String::new();
    for (name, glyph) in [("Ctrl", "⌃"), ("Alt", "⌥"), ("Shift", "⇧"), ("Cmd", "⌘")] {
        if modifiers.split('+').any(|declared| declared == name) {
            written.push_str(glyph);
        }
    }
    written.push_str(&key_glyph(key));
    written
}

/// La touche d'un accélérateur, telle qu'un menu macOS la montre.
fn key_glyph(key: &str) -> Cow<'_, str> {
    match key {
        "Tab" => Cow::Borrowed("⇥"),
        "Comma" => Cow::Borrowed(","),
        "Minus" => Cow::Borrowed("-"),
        "NumpadAdd" => Cow::Borrowed("+"),
        // Une lettre se montre en capitale, un chiffre reste lui-même.
        other => Cow::Owned(other.to_uppercase()),
    }
}

/// Le choix de thème venu de la fenêtre de réglages — la **seconde surface** du même état.
///
/// Elle passe par ici et non par `features::theme` pour une raison qui est tout l'objet du
/// critère : les trois coches du menu natif doivent suivre un choix fait ailleurs, et une
/// feature n'a pas à connaître la forme d'un menu. Le corps est donc exactement celui que
/// [`dispatch`] joue sur `Route::Backend(Backend::ChooseTheme(_))`, [`choose_theme`] — un
/// seul détenteur de l'état,
/// `features::theme::ThemeState` ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)),
/// et deux façons de le lui demander.
///
/// Rien n'est rendu à l'appelant : la fenêtre de réglages apprend le nouveau mode par
/// `ash://theme-mode`, que Tauri diffuse à **toutes** les fenêtres — c'est le même chemin
/// qu'un choix fait dans le menu, donc les deux surfaces ne peuvent pas diverger.
///
/// **Elle est synchrone, et ce n'est pas un détail** : Tauri exécute une commande sans `async`
/// sur le fil principal, et c'est le seul fil depuis lequel un `NSMenu` se modifie. La rendre
/// `async` la ferait partir sur le pool, où la coche serait posée hors du fil de l'interface.
#[tauri::command]
pub fn theme_set_mode<R: Runtime>(app: AppHandle<R>, mode: ThemeMode) {
    choose_theme(&app, mode);
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
        // Par [`choose_theme`], et jamais par `theme::choose` seul : c'est là que les trois
        // coches du menu sont remises d'équerre, et c'est aussi le chemin que prend le choix
        // venu de la fenêtre de réglages ([`theme_set_mode`]).
        Route::Backend(Backend::ChooseTheme(mode)) => choose_theme(app, mode),
        // La taille de police est un **état**, comme le thème : elle est retenue par
        // `features::theme` — donc gardée d'une session à l'autre — avant d'être annoncée à
        // la webview, qui n'a plus qu'à réajuster ses grilles
        // ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
        Route::Backend(Backend::ResizeFont(step)) => theme::resize_terminal_font(app, step),
        // Une fenêtre est un objet du backend, comme le thème : l'ouvrir depuis la webview
        // demanderait à la fenêtre principale d'exister pour que la seconde puisse naître.
        Route::Backend(Backend::OpenSettings) => settings::open(app),
        // L'échec d'émission signifie qu'il n'y a plus de webview à prévenir : rien à
        // rattraper, et surtout pas de panique dans un gestionnaire d'event.
        Route::Webview(label) => {
            let _ = app.emit_to(label, MENU_ACTION_EVENT, action.id());
        }
        // Fermer, et non cacher : la fenêtre de réglages est construite à l'exécution, donc
        // `settings::open` la refait à la demande suivante — c'est la décision de
        // `features::settings::commands::open`, et rien n'a à porter un état « ouverte ».
        //
        // La fenêtre est retrouvée **par le label que `route` a rendu**, et non reprise dans
        // `focused` : ce gestionnaire obéit à la destination, il ne la redécide pas. Une
        // fenêtre disparue entre la lecture du focus et ici ne ferme rien.
        Route::CloseWindow(label) => {
            if let Some(window) = app.get_webview_window(label) {
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
        Action::ChooseTheme(mode) => Route::Backend(Backend::ChooseTheme(mode)),
        Action::ResizeFont(step) => Route::Backend(Backend::ResizeFont(step)),
        Action::OpenSettings => Route::Backend(Backend::OpenSettings),
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
    Backend(Backend),
    /// Émise à **une** webview, nommée par son label — jamais diffusée.
    Webview(&'a str),
    /// Ferme la fenêtre nommée par son label — celle du premier plan.
    CloseWindow(&'a str),
    /// Personne ne joue cette action : la surface qu'elle viserait n'est pas devant.
    Nowhere,
}

/// Ce que le backend a à jouer, quand [`route`] le désigne.
///
/// C'est le sous-ensemble d'[`Action`] qui ne part pas vers une webview, redit comme un type
/// à part **pour que le compilateur le tienne** : la version précédente rendait un
/// `Route::Backend` nu, et `dispatch` rejouait un `match action` avec un bras muet — une
/// quatrième action retenue en Rust y aurait été silencieusement ignorée, et rien n'aurait
/// échoué. Ici, [`route`] doit nommer ce qu'elle demande, et `dispatch` doit le traiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    /// Le thème, retenu par `features::theme` puis annoncé aux deux fenêtres.
    ChooseTheme(ThemeMode),
    /// La taille de police du terminal — un réglage de l'application, pas de l'onglet.
    ResizeFont(FontStep),
    /// L'ouverture de la fenêtre de réglages, ou son retour au premier plan.
    OpenSettings,
}

/// Retient un thème, d'où qu'il vienne — le menu ou la fenêtre de réglages.
///
/// Les deux surfaces passent par ici pour que la coche du menu ne puisse pas rester en
/// arrière : l'oubli n'aurait aucun symptôme visible avant qu'on ouvre le menu, et le menu
/// est justement l'endroit où l'on va vérifier ce qui est choisi.
fn choose_theme<R: Runtime>(app: &AppHandle<R>, mode: ThemeMode) {
    theme::choose(app, mode);
    // **Toujours**, et pas seulement quand le mode a changé : un `CheckMenuItem` bascule sa
    // propre coche au clic. Cliquer l'entrée déjà cochée la décocherait donc, et le menu
    // n'aurait plus aucun mode coché.
    check_only(app, mode);
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
    /// Toutes les actions du module, **dans l'ordre où le menu les montre**.
    ///
    /// C'est la seule énumération de l'espace des actions, et elle a deux lecteurs : la liste
    /// de raccourcis de la fenêtre de réglages ([`menu_shortcuts`]) et le test d'aller-retour
    /// des identifiants. Deux tables à tenir à la main auraient divergé à la première action
    /// ajoutée, et la divergence n'aurait eu aucun symptôme avant qu'on ouvre l'écran.
    ///
    /// Les trois familles — positions d'onglet, pas de taille, thèmes — ne sont pas recopiées
    /// non plus : elles viennent de `DIRECT_TABS`, de `FontStep::ALL` et de `ThemeMode::ALL`,
    /// donc un quatrième thème entre ici tout seul. Ce qui reste écrit à la main, ce sont les
    /// huit variantes sans donnée, et c'est le seul endroit du module où elles le sont.
    ///
    /// **L'ordre est celui de la barre de menus**, tel que [`build`] l'assemble — Application,
    /// View, Terminal —, et non l'ordre de déclaration de l'énumération : la fenêtre de réglages
    /// ne trie pas, précisément pour qu'on retrouve un raccourci là où on l'a vu dans le menu.
    fn every() -> Vec<Action> {
        let mut every = vec![Action::OpenSettings, Action::ToggleSidebar];
        every.extend(FontStep::ALL.map(Action::ResizeFont));
        every.extend(ThemeMode::ALL.map(Action::ChooseTheme));
        every.extend([
            Action::NewTab,
            Action::NewHomeTab,
            Action::CloseTab,
            Action::NextTab,
            Action::PreviousTab,
            Action::ClearScrollback,
        ]);
        every.extend((1..=DIRECT_TABS).map(Action::SelectTab));
        every
    }

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
        // la frontière sous forme de chaîne et rien ne le vérifie à la compilation. La liste
        // est celle du module, pas une copie : une action recensée ici et nulle part ailleurs
        // n'aurait rien prouvé de ce que la fenêtre de réglages lit.
        let actions = Action::every();

        // When
        let round_trip: Vec<Option<Action>> =
            actions.iter().map(|a| Action::from_id(&a.id())).collect();

        // Then
        let expected: Vec<Option<Action>> = actions.iter().copied().map(Some).collect();
        assert_eq!(round_trip, expected);
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
        assert_eq!(
            destination,
            Route::Backend(Backend::ChooseTheme(ThemeMode::Dark))
        );
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
    fn given_the_accelerators_the_menu_declares_when_they_are_written_for_the_screen_then_they_read_as_macos_writes_them(
    ) {
        // Given — les noms que l'analyseur de Tauri exige, et les deux pièges : l'ordre des
        // modificateurs est celui de macOS et non celui de la déclaration, et `NumpadAdd`
        // n'est pas le pavé numérique mais le seul nom que `muda` traduise en `+`
        let declared = [
            "Cmd+T",
            "Cmd+Shift+T",
            "Ctrl+Tab",
            "Ctrl+Shift+Tab",
            "Cmd+Comma",
            "Cmd+NumpadAdd",
            "Cmd+Minus",
            "Cmd+0",
        ];

        // When
        let written: Vec<String> = declared.iter().map(|a| keys(a)).collect();

        // Then
        assert_eq!(written, ["⌘T", "⇧⌘T", "⌃⇥", "⌃⇧⇥", "⌘,", "⌘+", "⌘-", "⌘0"]);
    }

    #[test]
    fn given_the_menu_table_when_the_settings_window_asks_for_the_shortcuts_then_each_one_carries_the_combination_the_menu_declares(
    ) {
        // Given / When — c'est le critère de l'issue #110 : la liste est **lue** depuis la
        // table du menu. Une combinaison recopiée en TypeScript aurait fini par annoncer un
        // raccourci que le menu ne déclare plus, et c'est l'écran qu'on croit
        let listed = menu_shortcuts();

        // Then — chaque ligne a un groupe et une combinaison, et une entrée sans
        // accélérateur (les trois thèmes) n'y figure pas
        assert!(listed
            .iter()
            .all(|row| !row.group.is_empty() && !row.keys.is_empty()));
        assert!(listed
            .iter()
            .any(|row| row.label == "New Tab" && row.keys == "⌘T"));
        assert!(!listed.iter().any(|row| row.label == "Light"));
    }

    #[test]
    fn given_the_menu_bar_order_when_the_shortcuts_are_listed_then_the_groups_come_in_that_order() {
        // Given / When — la fenêtre de réglages ne trie pas : elle groupe dans l'ordre reçu,
        // pour qu'on retrouve un raccourci là où on l'a vu dans le menu. C'est donc ici que
        // l'ordre est décidé, et il est celui que `build` assemble — Application, View, Terminal
        let listed = menu_shortcuts();

        // Then
        let mut groups: Vec<&str> = Vec::new();
        for row in &listed {
            if groups.last() != Some(&row.group.as_str()) {
                groups.push(&row.group);
            }
        }
        assert_eq!(groups, ["application", "view", "terminal"]);
    }

    #[test]
    fn given_the_nine_tab_positions_when_they_are_listed_then_they_are_one_line_read_from_both_ends(
    ) {
        // Given / When — la spec §4.4 les écrit `Cmd+1 … Cmd+9`, et neuf lignes identiques
        // à un rang près feraient perdre les huit autres raccourcis dans la liste
        let listed = menu_shortcuts();
        let positions: Vec<&Shortcut> = listed
            .iter()
            .filter(|row| row.label.starts_with("Tab "))
            .collect();

        // Then — les bornes viennent de `DIRECT_TABS`, pas d'une chaîne écrite à la main
        assert_eq!(positions.len(), 1);
        assert_eq!(
            positions.first().map(|row| row.keys.as_str()),
            Some("⌘1 … ⌘9")
        );
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
