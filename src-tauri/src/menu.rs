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
//!   d'atteindre la webview : ce que le menu déclare, xterm.js ne le voit jamais, donc
//!   une entrée de menu et une frappe du terminal ne peuvent pas se disputer la même
//!   touche. C'est vrai de toute combinaison, y compris de celles qu'un shell utilise
//!   vraiment — d'où le soin pris à choisir les défauts (voir la famille git plus bas) ;
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
//! **La liste des raccourcis n'est plus ici — elle est dans `features::shortcuts`, et ce
//! menu s'en déduit.** C'est le déplacement de l'issue #22, et il ne dédouble rien : la
//! liste unique de #110 a changé de côté. [`descriptor`] ne porte plus qu'un **défaut** par
//! action ; la combinaison en vigueur est demandée aux liaisons, par [`item`], et le menu se
//! **refait** ([`rebuild`]) quand l'une d'elles change. Une combinaison recopiée en
//! TypeScript aurait fini par annoncer un raccourci que le menu ne déclare plus, et c'est
//! l'écran des réglages qu'on croit quand les deux ne disent pas la même chose.
//!
//! Il porte donc les commandes de la section `shortcuts` ([`menu_shortcuts`],
//! [`shortcut_bind`], et les quatre autres), comme il porte [`theme_set_mode`] : elles ont
//! toutes à corriger le menu — une coche, un accélérateur —, et une feature n'a pas à
//! connaître la forme d'un menu. La remarque de [`check_only`] vaut mot pour mot pour elles.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::menu::{
    AboutMetadata, CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::features::merge::MergeSurface;
use crate::features::settings::commands as settings;
use crate::features::shortcuts::{
    ActionBinding, Bindings, CapturePreview, Combination, ConflictChoice, KeyStroke, Listing,
    ShortcutError, ShortcutsReport,
};
use crate::features::theme::{commands as theme, FontStep, ThemeMode};

/// Nom de l'event qui porte l'action choisie. Contrat avec `src/app/menu.ts`.
const MENU_ACTION_EVENT: &str = "ash://menu-action";

/// Nom de l'event qui dit qu'une liaison a changé. Contrat avec `src/app/menu.ts`.
///
/// Il ne porte **rien** : ce n'est pas une valeur, c'est un signal. Chaque surface redemande
/// ce dont elle a besoin — le pied de la sidebar les glyphes d'une action, la fenêtre de
/// réglages l'instantané entier. Faire voyager la liste ferait de chaque abonné le détenteur
/// d'une copie ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)), et le pied de la
/// colonne n'a que faire des douze autres lignes.
const SHORTCUTS_CHANGED_EVENT: &str = "ash://shortcuts-changed";

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

/// Ce que le menu sait de l'instant — **et rien de plus**.
///
/// Une seule entrée d'Ash dépend d'autre chose que de sa liaison : « Resolve Conflicts »,
/// que la spec §4.4 veut active « seulement pendant un rebase ou un merge arrêté ». Les
/// trois formes possibles ne se valent pas :
///
/// - **l'éteindre** est la forme de macOS, celle que `validateMenuItem:` produit partout
///   ailleurs sur le système : l'entrée reste à sa place, grisée, et son équivalent clavier
///   ne s'allume pas. C'est celle-ci ;
/// - **la laisser allumée et refuser au moment du geste** annoncerait un raccourci qui ne
///   fait rien — exactement ce que l'issue #127 reproche à l'état d'avant, où le groupe git
///   était listé sans entrée de menu ;
/// - **ne pas la déclarer** tant que rien n'est arrêté ferait scintiller la barre de menus
///   au rythme des rebases, et ferait disparaître de la fenêtre de réglages une ligne que
///   l'utilisateur a peut-être rebindée.
///
/// **Ce que ça coûte, et il faut le dire** : le menu doit apprendre deux choses qu'il ne
/// savait pas — quel worktree est sous les yeux, et si quelque chose y est arrêté. Le second
/// est une lecture de `features::merge` ([`MergeSurface::reachable`]), donc la règle reste
/// chez qui la possède. Le premier ne se trouve **nulle part** dans le backend : la fenêtre
/// est seule à savoir quel onglet est actif, et c'est elle qui le rapporte
/// ([`menu_worktree_in_view`]). Elle ne décide de rien pour autant — elle nomme un worktree,
/// le backend en tire l'état de l'entrée. C'est la même forme que
/// [`shortcut_listening`] : la webview rapporte un fait, le menu en tire ce qu'il en fait.
#[derive(Default)]
pub struct MergeReach {
    /// Le worktree de l'onglet au premier plan. `None` : aucun onglet, ou un onglet hors de
    /// tout dépôt — l'entrée est alors éteinte, faute de worktree à résoudre.
    in_view: Mutex<Option<PathBuf>>,
    /// L'entrée est-elle allumée ? Retenu pour deux raisons : ne reposer l'état du menu que
    /// lorsqu'il change, et permettre à [`rebuild`] de reposer une entrée éteinte éteinte —
    /// un menu neuf ne sait rien de l'instant, comme les coches de thème.
    live: AtomicBool,
}

impl MergeReach {
    /// La fenêtre annonce le worktree qu'elle regarde.
    fn look_at(&self, worktree_root: Option<PathBuf>) {
        *self.locked() = worktree_root;
    }

    /// Le worktree regardé est-il celui dont on vient d'apprendre quelque chose ?
    ///
    /// C'est ce qui évite de rouvrir la question à chaque écriture de `.git` d'un dépôt qui
    /// n'est pas sous les yeux — la surveillance en observe autant qu'il y a d'onglets.
    fn watching(&self, worktree_root: &Path) -> bool {
        self.locked().as_deref() == Some(worktree_root)
    }

    /// L'entrée doit-elle être allumée ? — **la règle, sans Tauri ni menu**.
    ///
    /// `stopped` est la lecture de `features::merge` ; rien n'est en vue veut dire éteint,
    /// et c'est le défaut au démarrage, avant que le premier onglet ne soit né.
    fn decide(&self, stopped: impl Fn(&Path) -> bool) -> bool {
        self.locked().as_deref().is_some_and(stopped)
    }

    /// Retient le nouvel état, et dit s'il a changé — le menu ne se repose que dans ce cas.
    fn settle(&self, live: bool) -> bool {
        self.live.swap(live, Ordering::Relaxed) != live
    }

    /// L'état en vigueur, pour un menu qu'on reconstruit.
    fn live(&self) -> bool {
        self.live.load(Ordering::Relaxed)
    }

    /// Un verrou empoisonné veut dire qu'un fil a paniqué **ailleurs** en le tenant. Ce
    /// qu'il protège est un chemin : le propager éteindrait la fenêtre pour une entrée de
    /// menu. Même conduite que `features::shortcuts::Bindings`.
    fn locked(&self) -> std::sync::MutexGuard<'_, Option<PathBuf>> {
        self.in_view
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Construit le menu de l'application.
///
/// `theme_mode` est le mode retenu de la session précédente : le menu est construit avant
/// que la webview n'existe, et une coche posée après coup serait une seconde source de
/// vérité. `bindings` est la même chose pour les touches — les liaisons sont relues avant,
/// et **c'est d'elles que chaque accélérateur vient**.
///
/// `merge_live` est la même chose pour la seule entrée qui s'éteint : un menu neuf ne sait
/// rien de l'instant, et la reconstruire allumée rendrait `⌘⌃M` actif sur un worktree
/// tranquille jusqu'au prochain changement d'onglet (voir [`MergeReach`]).
///
/// Elle est appelée deux fois dans la vie du processus : au démarrage, et à chaque
/// changement de liaison — voir [`rebuild`].
pub fn build<R: Runtime>(
    app: &AppHandle<R>,
    theme_mode: ThemeMode,
    bindings: &Bindings,
    merge_live: bool,
) -> tauri::Result<Menu<R>> {
    // `Cmd+,` ouvre les réglages : c'est le raccourci que macOS attend dans le menu
    // applicatif, et le seul endroit où un utilisateur va le chercher. Il est écrit
    // `Cmd+Comma` parce que l'analyseur d'accélérateurs de Tauri lit des **noms** de
    // touches, pas des caractères — voir [`descriptor`], où il est déclaré.
    let settings_item = item(app, Action::OpenSettings, bindings)?;

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
    let new_tab = item(app, Action::NewTab, bindings)?;
    let new_home_tab = item(app, Action::NewHomeTab, bindings)?;
    let close_tab = item(app, Action::CloseTab, bindings)?;

    // `Ctrl+Tab` / `Ctrl+Shift+Tab` : la convention des navigateurs et d'iTerm2 pour
    // circuler, là où `Cmd+1`…`Cmd+9` s'arrête à neuf et ne dit rien du « suivant ».
    //
    // **Ces deux accélérateurs-là ne sont pas joués par le menu**, contrairement à tous
    // les autres de ce module — voir la note d'en-tête. Ils figurent ici pour être vus
    // (⌃⇥ dans le menu) et cliquables à la souris ; la touche, elle, est captée par
    // `src/app/shortcuts.ts`. Les garder déclarés fait aussi que le jour où `muda`
    // corrigera son équivalent clavier, le chemin natif reprendra la main tout seul, sans
    // double déclenchement : un accélérateur capté par AppKit n'atteint jamais la webview.
    let next_tab = item(app, Action::NextTab, bindings)?;
    let previous_tab = item(app, Action::PreviousTab, bindings)?;
    let clear = item(app, Action::ClearScrollback, bindings)?;

    // Les neuf entrées existent en permanence, même quand il y a moins d'onglets : une
    // action qui ne désigne personne est ignorée côté webview. Les activer et les
    // désactiver au fil des ouvertures ferait vivre l'état des onglets à deux endroits.
    let select: Vec<MenuItem<R>> = (1..=DIRECT_TABS)
        .map(|position| item(app, Action::SelectTab(position), bindings))
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
    let toggle_sidebar = item(app, Action::ToggleSidebar, bindings)?;
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
        .map(|step| item(app, Action::ResizeFont(step), bindings))
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
                shown.default.as_deref(),
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

    // Le sous-menu « Git » — les cinq surfaces de la spec §7, et les cinq raccourcis de la
    // §4.4. Elles n'y arrivent qu'aujourd'hui parce qu'un accélérateur se déclare **avec**
    // sa surface : les déclarer plus tôt aurait mis dans la fenêtre de réglages cinq lignes
    // que rien n'aurait jouées (issue #127).
    //
    // Elles sont aussi ce qui rend ces gestes atteignables à la souris, comme la spec §4.4
    // l'exige de « toutes ces actions » : sans entrée de menu, `⌘⌃I` serait le seul chemin
    // vers la fiche de branche pour qui ne connaît pas la barre du panneau bas.
    let branches = item(app, Action::ToggleBranches, bindings)?;
    let graph = item(app, Action::ToggleGraph, bindings)?;
    let worktrees = item(app, Action::ToggleWorktrees, bindings)?;
    // **La seule entrée d'Ash qui naisse peut-être éteinte** — voir [`MergeReach`]. Elle
    // s'éteint ici et pas dans [`item`] : quatorze entrées n'ont pas à porter le paramètre
    // d'une quinzième, et l'exception se lit à l'endroit qui la connaît.
    let merge = item(app, Action::OpenMerge, bindings)?;
    merge.set_enabled(merge_live)?;
    let branch_card = item(app, Action::ToggleBranchCard, bindings)?;
    let git_separator = PredefinedMenuItem::separator(app)?;
    let git = Submenu::with_items(
        app,
        "Git",
        true,
        &[
            &branches,
            &git_separator,
            &graph,
            &worktrees,
            &branch_card,
            &git_separator,
            &merge,
        ],
    )?;

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

    // **L'ordre de cette ligne est celui que la fenêtre de réglages montre** : elle ne trie
    // pas, elle groupe dans l'ordre reçu, pour qu'on retrouve un raccourci là où on l'a vu
    // dans le menu. `Action::every` le suit, et un test le tient.
    Menu::with_items(app, &[&application, &edit, &view, &terminal, &git, &window])
}

/// Ce que le menu affiche d'une action, et où il la range.
///
/// C'est la table des **défauts** du module, et elle est **exhaustive par construction** :
/// le `match` de [`descriptor`] couvre tout [`Action`], donc une action de plus ne compile
/// pas tant qu'elle n'a pas dit son libellé, son groupe et son raccourci d'origine.
///
/// Ce qu'elle ne dit plus, depuis l'issue #22 : la combinaison **en vigueur**. Celle-là est
/// aux liaisons (`features::shortcuts`), qui partent d'ici et gardent les écarts.
struct Descriptor {
    /// Le sous-menu où l'action vit, en un mot — et le groupe sous lequel les réglages la
    /// listent. Les réglages ne le traduisent pas : un titre de groupe écrit là-bas
    /// finirait par ranger sous « terminal » une entrée que le menu a déplacée.
    group: &'static str,
    /// Le libellé, dans la capitalisation de macOS pour un menu.
    label: Cow<'static, str>,
    /// L'accélérateur **d'origine**, ou `None` pour une entrée sans raccourci. C'est lui que
    /// `back to default` et `reset all` rendent, et à quoi `n changed` compare.
    default: Option<Cow<'static, str>>,
    /// Comment la section `shortcuts` montre l'action.
    listing: Listing,
}

/// Le libellé, le groupe et le raccourci d'origine d'une action — **la** table du module.
///
/// Les noms de touches sont ceux de l'analyseur de Tauri, qui lit des **noms** et non des
/// caractères : d'où `Cmd+Comma`, `Cmd+Minus`, et `Cmd+NumpadAdd` pour le `+` du clavier
/// principal (voir la note au-dessus des entrées de taille de police dans [`build`]).
/// `Combination::glyphs` est ce qui les rend tels que macOS les écrit.
fn descriptor(action: Action) -> Descriptor {
    let listing = match action {
        // Un thème n'a pas de raccourci, donc pas de ligne : en inventer un prendrait une
        // touche au shell pour rien.
        Action::ChooseTheme(_) => Listing::Hidden,
        // Les neuf positions se lisent en **une** ligne, d'un bout à l'autre — la spec §4.4
        // les écrit ainsi, et neuf lignes identiques à un rang près feraient perdre les
        // autres raccourcis dans la liste. Elle ne se capture pas, et **rien ne lui prend sa
        // combinaison** : une capture qui viserait `⌘1` se voit refuser, avec le nom de la
        // famille qui la tient (issue #137, et `Listing::Family` pour le pourquoi).
        Action::SelectTab(1) => Listing::Family {
            through: Action::SelectTab(DIRECT_TABS).id(),
        },
        Action::SelectTab(_) => Listing::Hidden,
        _ => Listing::Row,
    };

    let (group, label, default) = match action {
        Action::OpenSettings => ("application", "Settings…", Some("Cmd+Comma")),
        Action::NewTab => ("terminal", "New Tab", Some("Cmd+T")),
        Action::NewHomeTab => ("terminal", "New Tab at ~", Some("Cmd+Shift+T")),
        Action::CloseTab => ("terminal", "Close Tab", Some("Cmd+W")),
        Action::NextTab => ("terminal", "Select Next Tab", Some("Ctrl+Tab")),
        Action::PreviousTab => ("terminal", "Select Previous Tab", Some("Ctrl+Shift+Tab")),
        // **`⌘K`, et ce défaut ne retire rien à personne** (issue #159). Il a longtemps été
        // laissé vide au motif inverse — « la touche appartient au shell, et un accélérateur
        // de menu est consommé par `performKeyEquivalent:` avant la webview, donc la poser
        // ici serait la lui retirer ». La seconde moitié de ce raisonnement était fausse, et
        // le résultat était un trou : la spec §4.4 promettait un effacement que rien ne
        // produisait, et personne ne récupérait la touche pour autant.
        //
        // Ce que `⌘` fait dans un terminal : **rien**. Le modificateur Command n'a aucune
        // représentation dans l'encodage d'entrée d'un terminal, contrairement à `Ctrl`, qui
        // produit un caractère de contrôle, et à `Alt`/`Meta`, qui préfixe par `ESC`. Aucun
        // `⌘` quoi que ce soit ne descend dans un PTY, jamais — Terminal.app et iTerm2
        // implémentent leur `⌘K` eux-mêmes, comme une action applicative, et iTerm2 va
        // jusqu'à proposer de **mapper** explicitement des combinaisons `⌘` vers des
        // séquences d'échappement : une option qui n'aurait aucun sens si elles y
        // descendaient toutes seules.
        //
        // Ce qui appartient vraiment au shell, c'est `⌃K` — couper la fin de ligne —, et
        // c'est une **autre** combinaison, que macOS n'apparie pas avec celle-ci. Ash ne la
        // déclare nulle part.
        //
        // Reste vrai, et sert ailleurs : un accélérateur de menu est bien consommé avant la
        // webview (deuxième point de l'en-tête de ce module). C'est ce qui rend `⌘K`
        // atteignable ici, et ce qui obligerait à réfléchir si le défaut portait `Ctrl`.
        // Comme n'importe quel autre, il reste **réglable** : la fenêtre de réglages peut
        // l'effacer ou le donner à une autre action.
        Action::ClearScrollback => ("terminal", "Clear Scrollback", Some("Cmd+K")),
        Action::ToggleSidebar => ("view", "Toggle Sidebar", Some("Cmd+B")),
        // Les deux seules entrées dont le libellé et l'accélérateur se calculent : la
        // position d'onglet est dans les deux, et `DIRECT_TABS` décide combien il y en a.
        Action::SelectTab(position) => {
            return Descriptor {
                group: "terminal",
                label: Cow::Owned(format!("Tab {position}")),
                default: Some(Cow::Owned(format!("Cmd+{position}"))),
                listing,
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
        Action::ChooseTheme(mode) => ("view", mode.label(), None),
        // **La famille git de la spec §4.4, et les cinq lettres ne sont pas au hasard** :
        // `⌘⌃` parce que ces combinaisons sont libres sur macOS et qu'aucune n'est réclamée
        // par un terminal — `⌃B` seul, que tmux prend pour préfixe, n'a rien à voir avec
        // `⌘⌃B`, qui porte `Cmd` et ne descend donc dans aucun PTY. La mnémonique est
        // **B**ranches, **G**raph, **W**orktrees, **M**erge, **I**nfo.
        //
        // Ce qu'on retire au shell en les posant est exactement rien, et pour la même raison
        // que `⌘K` (voir `ClearScrollback`) : un accélérateur de menu est bien consommé par
        // `performKeyEquivalent:` avant la webview, mais aucune combinaison portant `Cmd` ne
        // descend dans une séquence de terminal — macOS n'en fait rien descendre. La
        // question ne se pose que pour un défaut qui porterait `Ctrl` seul, et aucun n'en
        // porte.
        //
        // Les trois combinaisons `⌘⌃` que **macOS** prend — `⌘⌃F`, `⌘⌃D`, `⌘⌃Espace` — sont
        // dans `features::shortcuts::reserved`, et le test
        // `given_the_combinations_ash_will_never_receive_…` de ce module confronte chaque
        // défaut déclaré ici à cette table. Aucune des cinq n'en fait partie.
        Action::ToggleBranches => ("git", "Branches…", Some("Cmd+Ctrl+B")),
        Action::ToggleGraph => ("git", "Commit Graph", Some("Cmd+Ctrl+G")),
        // `⌘⌃W`, et `⌘W` ferme toujours un onglet : deux combinaisons distinctes, que macOS
        // apparie sur leurs modificateurs — elles ne se disputent rien.
        Action::ToggleWorktrees => ("git", "Worktrees", Some("Cmd+Ctrl+W")),
        Action::OpenMerge => ("git", "Resolve Conflicts", Some("Cmd+Ctrl+M")),
        Action::ToggleBranchCard => ("git", "Branch Card", Some("Cmd+Ctrl+I")),
    };

    Descriptor {
        group,
        label: Cow::Borrowed(label),
        default: default.map(Cow::Borrowed),
        listing,
    }
}

/// Toutes les actions du menu, telles que les liaisons les reçoivent.
///
/// C'est le **seul** point de contact entre le menu et `features::shortcuts` : la feature
/// ne sait pas ce qu'une action fait, et le menu ne sait pas comment une liaison est gardée.
/// Un défaut que `Combination::parse` refuserait est laissé sans raccourci plutôt que de
/// paniquer au démarrage — la table est en dur au-dessus, donc un tel refus serait un bug de
/// ce fichier, et un menu amputé d'une touche vaut mieux qu'une application qui n'ouvre pas.
pub fn action_bindings() -> Vec<ActionBinding> {
    Action::every()
        .into_iter()
        .map(|action| {
            let shown = descriptor(action);
            ActionBinding {
                action: action.id(),
                group: shown.group.to_owned(),
                label: shown.label.into_owned(),
                default: shown
                    .default
                    .and_then(|written| Combination::parse(&written).ok()),
                listing: shown.listing,
            }
        })
        .collect()
}

/// Une entrée de menu : son libellé vient de [`descriptor`], **sa touche des liaisons**.
///
/// Elle naît allumée, sans exception : la seule entrée d'Ash qui s'éteigne — « Resolve
/// Conflicts », quand rien n'est arrêté dans le worktree sous les yeux (voir
/// [`MergeReach`]) — s'éteint chez [`build`], juste après avoir été construite. Le fait est
/// alors écrit une fois, à l'endroit qui le connaît, plutôt que passé par quatorze appels
/// qui n'en ont rien à faire.
fn item<R: Runtime>(
    app: &AppHandle<R>,
    action: Action,
    bindings: &Bindings,
) -> tauri::Result<MenuItem<R>> {
    let shown = descriptor(action);
    let id = action.id();
    let accelerator = bindings.accelerator(&id);
    MenuItem::with_id(app, id, shown.label.as_ref(), true, accelerator.as_deref())
}

/// Refait le menu applicatif à partir des liaisons en vigueur.
///
/// **C'est le seul chemin qui tienne pour les deux sens d'un changement.**
/// `MenuItem::set_accelerator` existe dans `muda` 0.19.3, et il aurait suffi pour *poser*
/// une touche ; mais son implémentation macOS (`MenuChild::set_key_accelerator`) n'écrit
/// dans le `NSMenuItem` que si le nouvel accélérateur est `Some` — passer `None` met à jour
/// le champ Rust et **laisse la touche sur l'entrée**. Un `⌫` n'aurait donc rien retiré du
/// menu : l'écran aurait dit « aucun raccourci » pendant que la touche continuait de jouer
/// l'action, et c'est exactement le mensonge que la liste unique existe pour éviter.
/// `AppHandle::set_menu` repose le menu entier, sur le fil principal, et les deux sens
/// marchent.
///
/// Les coches de thème sont reposées avec, parce qu'un menu neuf ne sait rien du mode en
/// cours — c'est la même raison qui fait passer `theme_mode` à [`build`] au démarrage.
///
/// Un échec n'est pas propagé : le raccourci est déjà retenu par les liaisons, et la seule
/// conséquence d'un menu non refait est qu'il reste sur l'ancienne touche jusqu'au
/// redémarrage. Éteindre l'application pour ça serait pire.
fn rebuild<R: Runtime>(app: &AppHandle<R>) {
    let Some(bindings) = app.try_state::<Arc<Bindings>>() else {
        return;
    };
    let mode = app
        .try_state::<Arc<crate::features::theme::ThemeState>>()
        .map(|theme| theme.mode())
        .unwrap_or_default();
    // Comme les coches de thème : un menu neuf ne sait rien de l'instant, et une entrée
    // « Resolve Conflicts » reposée allumée sur un worktree tranquille annoncerait un `⌘⌃M`
    // sans effet jusqu'au prochain changement d'onglet.
    if let Ok(menu) = build(app, mode, bindings.inner().as_ref(), merge_live(app)) {
        let _ = app.set_menu(menu);
    }
}

/// La fenêtre annonce le worktree qu'elle regarde — l'onglet actif a changé.
///
/// Elle ne décide de rien : elle **nomme** un worktree, et le backend en tire l'état de
/// l'entrée « Resolve Conflicts » (voir [`MergeReach`]). `None` quand l'onglet actif n'est
/// dans aucun worktree, ou qu'il n'y a plus d'onglet du tout.
///
/// **Synchrone, comme [`theme_set_mode`] et pour la même raison** : Tauri exécute une
/// commande sans `async` sur le fil principal, et c'est le seul fil depuis lequel un
/// `NSMenu` se modifie.
#[tauri::command]
pub fn menu_worktree_in_view<R: Runtime>(app: AppHandle<R>, worktree_root: Option<String>) {
    let Some(reach) = app.try_state::<Arc<MergeReach>>() else {
        return;
    };
    reach.look_at(worktree_root.map(PathBuf::from));
    settle_merge_entry(&app);
}

/// L'état git d'un worktree surveillé a changé — le menu rouvre la question s'il le regarde.
///
/// Appelée par le composition root depuis la surveillance de `.git`
/// ([ADR-0011](../../docs/adr/0011-surveillance-de-fichiers.md)), et **pas** par la fenêtre :
/// un rebase commence le plus souvent dans le terminal qu'on a sous les yeux, sans qu'aucun
/// onglet ne change. Sans ce chemin, `⌘⌃M` resterait éteint jusqu'à ce qu'on change d'onglet
/// et qu'on revienne.
///
/// Le travail est **reporté sur le fil principal** : cette fonction est appelée depuis le
/// fil de FSEvents, et un `NSMenuItem` ne se modifie que sur le fil de l'interface. Le
/// report a un second effet, voulu : la lecture de `features::merge` n'a pas lieu pendant
/// que la surveillance tient ses propres verrous.
pub fn worktree_changed<R: Runtime>(app: &AppHandle<R>, worktree_root: &Path) {
    let Some(reach) = app.try_state::<Arc<MergeReach>>() else {
        return;
    };
    if !reach.watching(worktree_root) {
        return;
    }
    let deferred = app.clone();
    let _ = app.run_on_main_thread(move || settle_merge_entry(&deferred));
}

/// L'état en vigueur de « Resolve Conflicts » — allumée, ou éteinte.
///
/// **Retenu, jamais relu** : les deux appelants reposent un menu (l'un le reconstruit,
/// l'autre sort d'une capture de combinaison), et aucun des deux n'est un moment où la
/// question se rouvre. Refaire ici la lecture de `features::merge` donnerait une seconde
/// réponse là où [`settle_merge_entry`] en a déjà écrit une, et les deux pourraient
/// diverger le temps d'un rebase.
///
/// `false` avant que le composition root n'ait posé le [`MergeReach`] : au démarrage, aucun
/// onglet n'est encore né, donc rien n'est arrêté sous des yeux qui ne regardent rien.
fn merge_live<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<Arc<MergeReach>>()
        .is_some_and(|reach| reach.live())
}

/// Allume ou éteint « Resolve Conflicts » selon ce qui est arrêté là où l'on regarde.
///
/// **La règle n'est pas ici** : elle est dans `features::merge`
/// ([`MergeSurface::reachable`]), qui est aussi ce que l'ouverture consulte. Ce module ne
/// fait que la poser sur une entrée de menu, et seulement quand la réponse a changé — un
/// `set_enabled` par écriture de `.git` reposerait l'entrée trois fois par `git add`.
fn settle_merge_entry<R: Runtime>(app: &AppHandle<R>) {
    let (Some(reach), Some(merges)) = (
        app.try_state::<Arc<MergeReach>>(),
        app.try_state::<Arc<MergeSurface>>(),
    ) else {
        return;
    };
    let live = reach.decide(|root| merges.reachable(root));
    if !reach.settle(live) {
        return;
    }
    let Some(menu) = app.menu() else {
        return;
    };
    let id = Action::OpenMerge.id();
    walk(&menu.items().unwrap_or_default(), &mut |entry| {
        if entry.id().as_ref() == id {
            let _ = entry.set_enabled(live);
        }
    });
}

/// Éteint — ou rallume — les entrées d'Ash pendant qu'une combinaison se capture.
///
/// **Sans elle, la capture ne peut pas capturer grand-chose.** Sur macOS, un accélérateur de
/// menu est consommé par `performKeyEquivalent:` avant d'atteindre la webview : c'est
/// l'argument même du menu natif (voir l'en-tête), et il se retourne contre le bloc de
/// capture. `⌘W` frappé pendant une capture fermerait la fenêtre de réglages au lieu d'être
/// lu, `⌘T` ouvrirait un onglet — donc échanger `⌘T` et `⌘W` serait tout simplement
/// impossible.
///
/// Une entrée **désactivée** ne répond pas à son équivalent clavier : la touche traverse
/// alors jusqu'à la webview, qui la rapporte. C'est le seul geste de ce module qui touche à
/// l'état d'affichage du menu plutôt qu'à ses touches, et il est transitoire par
/// construction.
///
/// Deux limites, à savoir plutôt qu'à découvrir :
///
/// - les entrées **prédéfinies** de macOS (Quitter, Copier, Réduire…) gardent leurs touches.
///   Les éteindre priverait la fenêtre du copier-coller au moment où l'on tape, et `⌘Q` n'est
///   pas une combinaison qu'Ash a à reprendre ;
/// - un filet de sécurité rallume tout si la fenêtre de réglages disparaît pendant une
///   capture — sans lui, un menu resterait éteint jusqu'au prochain changement de liaison.
#[tauri::command]
pub fn shortcut_listening<R: Runtime>(app: AppHandle<R>, active: bool) {
    enable_actions(&app, !active);
    if !active {
        return;
    }
    // Le filet : fermer la fenêtre au clic pendant une capture ne laisse pas un menu éteint.
    // Il est reposé à chaque ouverture de capture — c'est idempotent, et ça vaut mieux que
    // de faire porter cette précaution à `features::settings`, qui n'a rien à savoir d'un
    // menu.
    if let Some(window) = app.get_webview_window(settings::SETTINGS_WINDOW) {
        let restoring = app.clone();
        window.on_window_event(move |event| {
            if matches!(
                event,
                tauri::WindowEvent::Destroyed | tauri::WindowEvent::CloseRequested { .. }
            ) {
                enable_actions(&restoring, true);
            }
        });
    }
}

/// Allume ou éteint toutes les entrées qu'Ash traite lui-même, et elles seules.
///
/// **Rallumer n'est pas symétrique d'éteindre**, et c'est « Resolve Conflicts » qui le dit :
/// elle est la seule dont l'état ne dépend pas d'une capture en cours mais du worktree sous
/// les yeux (voir [`MergeReach`]). Rallumer tout la rallumerait aussi, et `⌘⌃M` répondrait
/// sur un worktree tranquille jusqu'au prochain changement d'onglet. Elle est donc reposée
/// après coup, à partir de l'état retenu — pas d'une seconde lecture, qui pourrait diverger.
fn enable_actions<R: Runtime>(app: &AppHandle<R>, enabled: bool) {
    let Some(menu) = app.menu() else {
        return;
    };
    let merge = Action::OpenMerge.id();
    let live = enabled && merge_live(app);
    walk(&menu.items().unwrap_or_default(), &mut |item| {
        let id = item.id();
        if Action::from_id(id.as_ref()).is_some() {
            let _ = item.set_enabled(if id.as_ref() == merge { live } else { enabled });
        }
    });
}

/// Parcourt l'arbre du menu et joue `visit` sur chaque entrée ordinaire.
///
/// Un menu natif est un arbre, et `Menu::items` n'en rend que le premier niveau — la même
/// raison qui fait descendre [`find_check`].
fn walk<R: Runtime>(items: &[MenuItemKind<R>], visit: &mut impl FnMut(&MenuItem<R>)) {
    for item in items {
        match item {
            MenuItemKind::MenuItem(entry) => visit(entry),
            MenuItemKind::Submenu(submenu) => walk(&submenu.items().unwrap_or_default(), visit),
            _ => {}
        }
    }
}

/// La section `shortcuts` de la fenêtre de réglages, d'un bloc (spec §4.4).
///
/// Elle **lit** les liaisons, elle n'en tient pas une copie : c'est le critère de l'issue
/// #110, et il n'a pas changé quand les raccourcis sont devenus réglables — seule la
/// personne qui détient la liste a changé.
///
/// La liste couvre désormais le tableau de la spec §4.4 en entier, groupe git compris — et
/// **aucune ligne n'a été ajoutée à la fenêtre de réglages pour ça** (issue #32). Les cinq
/// combinaisons sont apparues le jour où [`build`] a déclaré le sous-menu « Git », parce que
/// c'est de là que la liste vient : un accélérateur se déclare avec sa surface, et la
/// section `shortcuts` le lit ensuite, comme le reste (issue #127).
#[tauri::command]
pub fn menu_shortcuts(bindings: tauri::State<'_, Arc<Bindings>>) -> ShortcutsReport {
    bindings.report()
}

/// L'action à qui appartient une frappe que le menu natif n'a **pas** consommée.
///
/// Deux entrées sont dans ce cas, et deux seulement — `⌃⇥` et `⌃⇧⇥` : `muda` leur donne un
/// équivalent clavier qu'AppKit ne reconnaît jamais (voir l'en-tête de ce module). La webview
/// les capte donc elle-même, et vient demander ici **à qui elles appartiennent** plutôt que
/// de le savoir : sans ça, une liaison déplacée laissait l'ancienne touche répondre encore,
/// et la webview aurait eu à tenir la seconde liste que tout ce travail évite.
///
/// Elle rend un identifiant d'action — celui-là même que `ash://menu-action` porte, et que
/// `src/app/menu.ts` sait déjà traduire.
#[tauri::command]
pub fn shortcut_owner(
    bindings: tauri::State<'_, Arc<Bindings>>,
    stroke: KeyStroke,
) -> Option<String> {
    bindings.owner(&stroke)
}

/// La combinaison en vigueur d'une action, écrite comme macOS l'écrit — vide s'il n'y en a
/// aucune.
///
/// L'autre sens de la même question : ce qu'une surface **affiche** d'un raccourci. Le pied
/// de la sidebar annonce `⌘T` parce qu'il le demande ici ; l'écrire dans le TypeScript en
/// ferait un mensonge au premier rebinding, et une seconde liste au second.
#[tauri::command]
pub fn shortcut_keys(bindings: tauri::State<'_, Arc<Bindings>>, action: String) -> String {
    bindings.keys(&action)
}

/// Ce que le bloc de capture montre pendant qu'on tape — sans rien retenir.
///
/// Elle ne touche à rien : c'est `⏎`, donc [`shortcut_bind`], qui pose. Ash ne valide rien à
/// la place de l'utilisateur ([ADR-0015](../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)),
/// et une combinaison posée à la frappe aurait rendu `esc` incapable d'annuler quoi que ce
/// soit.
#[tauri::command]
pub fn shortcut_preview(
    bindings: tauri::State<'_, Arc<Bindings>>,
    stroke: KeyStroke,
) -> CapturePreview {
    bindings.preview(&stroke)
}

/// Pose la combinaison confirmée par `⏎` — ou ouvre le bloc de conflit qu'elle produirait.
///
/// Les cinq commandes qui suivent partagent la même forme, et c'est voulu : elles rendent
/// **l'instantané entier**, et refont le menu quand quelque chose a bougé. La fenêtre
/// redessine à partir de ce qu'elle reçoit, elle ne modifie jamais une liste locale
/// ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[tauri::command]
pub fn shortcut_bind<R: Runtime>(
    app: AppHandle<R>,
    bindings: tauri::State<'_, Arc<Bindings>>,
    action: String,
    stroke: KeyStroke,
) -> Result<ShortcutsReport, String> {
    let rebound = bindings.bind(&action, &stroke).map_err(refusal)?;
    Ok(replay(&app, &bindings, rebound.0))
}

/// Retire le raccourci d'une ligne — le `⌫` du bloc de capture.
#[tauri::command]
pub fn shortcut_clear<R: Runtime>(
    app: AppHandle<R>,
    bindings: tauri::State<'_, Arc<Bindings>>,
    action: String,
) -> Result<ShortcutsReport, String> {
    let rebound = bindings.clear(&action).map_err(refusal)?;
    Ok(replay(&app, &bindings, rebound.0))
}

/// Rend son défaut à une ligne — l'icône de retour des lignes changées.
#[tauri::command]
pub fn shortcut_reset<R: Runtime>(
    app: AppHandle<R>,
    bindings: tauri::State<'_, Arc<Bindings>>,
    action: String,
) -> Result<ShortcutsReport, String> {
    let rebound = bindings.reset(&action).map_err(refusal)?;
    Ok(replay(&app, &bindings, rebound.0))
}

/// `reset all` — toutes les lignes reprennent leur défaut.
#[tauri::command]
pub fn shortcut_reset_all<R: Runtime>(
    app: AppHandle<R>,
    bindings: tauri::State<'_, Arc<Bindings>>,
) -> ShortcutsReport {
    let rebound = bindings.reset_all();
    replay(&app, &bindings, rebound.0)
}

/// Referme un conflit par l'une de ses deux issues nommées.
#[tauri::command]
pub fn shortcut_resolve<R: Runtime>(
    app: AppHandle<R>,
    bindings: tauri::State<'_, Arc<Bindings>>,
    choice: ConflictChoice,
) -> ShortcutsReport {
    let rebound = bindings.resolve(choice);
    replay(&app, &bindings, rebound.0)
}

/// Refait le menu si besoin, puis rend l'instantané.
///
/// Le menu **d'abord** : la fenêtre de réglages redessinera de toute façon, alors qu'un
/// menu laissé en arrière garderait la touche jusqu'au prochain changement.
fn replay<R: Runtime>(app: &AppHandle<R>, bindings: &Bindings, rebound: bool) -> ShortcutsReport {
    if rebound {
        rebuild(app);
        // Les surfaces qui **affichent** un raccourci en dérivent aussi : le pied de la
        // sidebar vit dans l'autre fenêtre, et rien ne l'aurait prévenu. L'échec d'émission
        // signifie qu'il n'y a plus de webview à prévenir — rien à rattraper.
        let _ = app.emit(SHORTCUTS_CHANGED_EVENT, ());
    }
    bindings.report()
}

/// Un refus, tel que la fenêtre de réglages le reçoit.
///
/// Une commande Tauri rend son erreur en chaîne ; celle-ci est déjà écrite pour être lue —
/// voir `ShortcutError`.
fn refusal(why: ShortcutError) -> String {
    why.to_string()
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
/// - les actions d'onglet partent vers **une** webview, celle qui détient les onglets, et
///   seulement quand c'est elle qu'on regarde ;
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
/// Trois règles, et une seule ne regarde pas le focus :
///
/// 1. le thème, la taille de police et l'ouverture des réglages sont des préférences de
///    **l'application** : elles ne visent aucune surface, donc elles se jouent en Rust
///    quelle que soit la fenêtre devant. C'est ce qui laisse la section `appearance` des
///    réglages repeindre les deux fenêtres et déplacer la coche du menu (#110) ;
/// 2. `CloseTab` appartient à la fenêtre du **premier plan** — la principale y ferme son
///    onglet actif, une autre se ferme elle-même, et un premier plan inconnu ne ferme rien ;
/// 3. **toutes** les autres actions d'onglet (`Cmd+T`, `Cmd+⇧T`, `Cmd+K`, `Cmd+B`, `Ctrl+⇥`,
///    `Cmd+1`…`Cmd+9`) ne partent que si la fenêtre à onglets est celle qu'on regarde.
///
/// La troisième règle est la correction de #116, et elle remplace celle de #107, qui ne
/// rendait sensible au focus que `CloseTab` au motif qu'elle seule détruit quelque chose.
/// Le critère n'est pas « est-ce destructeur » : `Cmd+K` efface un scrollback hors de vue et
/// l'utilisateur ne le découvre qu'en revenant, `Cmd+T` ouvre un onglet dans une fenêtre à
/// laquelle il ne pensait pas. Le critère est « la surface visée est-elle celle qu'on
/// regarde » — dès qu'Ash pose une surface par-dessus, c'est elle qui a le regard, et le
/// geste y reste sans effet comme partout ailleurs sur macOS.
///
/// **Aucun bras `_`** : chaque action est nommée, donc une action de plus ne compile pas
/// tant qu'elle n'a pas dit si elle vise une surface ou l'application. C'est la raison qui a
/// fait naître [`Backend`], et elle vaut au moins autant ici — un bras muet aurait
/// silencieusement rendu la prochaine action insensible au premier plan, sans rien casser.
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
        Action::NewTab
        | Action::NewHomeTab
        | Action::NextTab
        | Action::PreviousTab
        | Action::ClearScrollback
        | Action::ToggleSidebar
        // Les cinq surfaces git vivent dans la fenêtre à onglets — la popup de branches
        // s'ancre sur sa ligne de statut, les trois vues sur son panneau bas, et l'onglet de
        // merge **est** un de ses onglets. Elles suivent donc la règle de #116 comme les
        // actions d'onglet : le geste ne part que si c'est cette fenêtre-là qu'on regarde.
        | Action::ToggleBranches
        | Action::ToggleGraph
        | Action::ToggleWorktrees
        | Action::OpenMerge
        | Action::ToggleBranchCard
        | Action::SelectTab(_) => match focused {
            Some(MAIN_WINDOW) => Route::Webview(MAIN_WINDOW),
            // Une autre fenêtre devant, ou aucune : la surface à onglets n'est pas celle
            // qu'on regarde, et une fenêtre de plus n'y changera rien — elle tombe dans ce
            // bras d'elle-même.
            Some(_) | None => Route::Nowhere,
        },
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
    /// La popup de branches — `⌘⌃B` (spec §7.1).
    ToggleBranches,
    /// Le graphe de commits dans le panneau bas — `⌘⌃G` (spec §7.2).
    ToggleGraph,
    /// Le tableau des worktrees dans le panneau bas — `⌘⌃W` (spec §7.3).
    ToggleWorktrees,
    /// L'onglet de merge — `⌘⌃M` (spec §7.4). **La seule entrée d'Ash qui s'éteigne**, et
    /// la seule qui dépende d'autre chose que de sa liaison : voir [`MergeReach`].
    OpenMerge,
    /// La fiche de branche dans le panneau bas — `⌘⌃I` (spec §7.5, ADR-0013).
    ToggleBranchCard,
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
        every.extend([
            Action::ToggleBranches,
            Action::ToggleGraph,
            Action::ToggleWorktrees,
            Action::OpenMerge,
            Action::ToggleBranchCard,
        ]);
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
            Action::ToggleBranches => "git:branches".to_owned(),
            Action::ToggleGraph => "git:graph".to_owned(),
            Action::ToggleWorktrees => "git:worktrees".to_owned(),
            Action::OpenMerge => "git:merge".to_owned(),
            Action::ToggleBranchCard => "git:branch-card".to_owned(),
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
            "git:branches" => Some(Action::ToggleBranches),
            "git:graph" => Some(Action::ToggleGraph),
            "git:worktrees" => Some(Action::ToggleWorktrees),
            "git:merge" => Some(Action::OpenMerge),
            "git:branch-card" => Some(Action::ToggleBranchCard),
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

    /// Les actions qui visent la surface à onglets — tout sauf `CloseTab`, dont le premier
    /// plan décide autrement, et tout sauf les trois préférences de l'application.
    fn tab_actions() -> Vec<Action> {
        let mut actions = vec![
            Action::NewTab,
            Action::NewHomeTab,
            Action::NextTab,
            Action::PreviousTab,
            Action::ClearScrollback,
            Action::ToggleSidebar,
        ];
        actions.extend((1..=DIRECT_TABS).map(Action::SelectTab));
        actions
    }

    #[test]
    fn given_the_settings_window_in_front_when_a_tab_action_is_asked_then_nothing_is_played() {
        // Given — les réglages devant, la fenêtre à onglets derrière. C'est la correction de
        // #116 : le critère n'est pas « est-ce destructeur » mais « la surface visée est-elle
        // celle qu'on regarde », et un `Cmd+T` frappé ici ne veut pas dire « ouvre un onglet
        // dans la fenêtre que je ne regarde pas »
        let focused = Some("settings");

        // When
        let destinations: Vec<Route<'_>> = tab_actions()
            .into_iter()
            .map(|action| route(action, focused))
            .collect();

        // Then — aucun onglet ouvert, aucun scrollback effacé, aucune sélection changée
        assert_eq!(destinations, vec![Route::Nowhere; tab_actions().len()]);
    }

    #[test]
    fn given_no_window_in_front_when_a_tab_action_is_asked_then_nothing_is_played() {
        // Given — toutes les fenêtres réduites, le menu applicatif reste atteignable
        let focused = None;

        // When
        let destinations: Vec<Route<'_>> = tab_actions()
            .into_iter()
            .map(|action| route(action, focused))
            .collect();

        // Then
        assert_eq!(destinations, vec![Route::Nowhere; tab_actions().len()]);
    }

    #[test]
    fn given_the_main_window_in_front_when_a_tab_action_is_asked_then_it_reaches_the_tabs() {
        // Given — la fenêtre à onglets est celle qu'on regarde
        let focused = Some(MAIN_WINDOW);

        // When
        let destinations: Vec<Route<'_>> = tab_actions()
            .into_iter()
            .map(|action| route(action, focused))
            .collect();

        // Then — tout fonctionne comme avant, et par un envoi **ciblé** : `emit`
        // diffuserait à toutes les webviews
        assert_eq!(
            destinations,
            vec![Route::Webview(MAIN_WINDOW); tab_actions().len()]
        );
    }

    #[test]
    fn given_the_settings_window_in_front_when_the_font_is_resized_then_it_is_still_played_in_rust()
    {
        // Given — la taille de police est une préférence de l'application, comme le thème et
        // l'ouverture des réglages : elle ne vise aucune surface
        let focused = Some("settings");

        // When
        let destinations = [
            route(Action::ResizeFont(FontStep::Bigger), focused),
            route(Action::OpenSettings, focused),
        ];

        // Then — #116 ne retire le focus qu'aux actions qui visent une surface ; retirer
        // celles-ci casserait la section `appearance` des réglages (#110)
        assert_eq!(
            destinations,
            [
                Route::Backend(Backend::ResizeFont(FontStep::Bigger)),
                Route::Backend(Backend::OpenSettings),
            ]
        );
    }

    /// Les liaisons du menu réel, sur un fichier en mémoire.
    fn menu_bindings() -> Bindings {
        Bindings::restore(
            Arc::new(crate::features::shortcuts::FakeBindingStore::default()),
            action_bindings(),
        )
    }

    #[test]
    fn given_the_menu_table_when_the_settings_window_asks_for_the_shortcuts_then_each_one_carries_the_combination_the_menu_declares(
    ) {
        // Given / When — c'est le critère de l'issue #110, et il tient toujours après #22 :
        // la liste est **lue** depuis les liaisons, qui partent des défauts de ce module.
        // Une combinaison recopiée en TypeScript aurait fini par annoncer un raccourci que
        // le menu ne déclare plus, et c'est l'écran qu'on croit
        let listed = menu_bindings().report();

        // Then — chaque ligne est rangée sous un groupe, et une action qu'aucun raccourci ne
        // vise (les trois thèmes) n'y figure pas. `Clear Scrollback` y porte son défaut
        // depuis #159 ; une ligne peut encore se retrouver sans combinaison, mais parce que
        // l'utilisateur l'a effacée — `bindings.rs` tient ce cas-là
        assert!(listed.rows.iter().all(|row| !row.group.is_empty()));
        assert!(listed
            .rows
            .iter()
            .any(|row| row.label == "New Tab" && row.keys == "⌘T"));
        assert!(listed
            .rows
            .iter()
            .any(|row| row.label == "Clear Scrollback" && row.keys == "⌘K"));
        assert!(!listed.rows.iter().any(|row| row.label == "Light"));
    }

    #[test]
    fn given_the_menu_bar_order_when_the_shortcuts_are_listed_then_the_groups_come_in_that_order() {
        // Given / When — la fenêtre de réglages ne trie pas : elle groupe dans l'ordre reçu,
        // pour qu'on retrouve un raccourci là où on l'a vu dans le menu. C'est donc ici que
        // l'ordre est décidé, et il est celui que `build` assemble — Application, View, Terminal
        let listed = menu_bindings().report();

        // Then
        let mut groups: Vec<&str> = Vec::new();
        for row in &listed.rows {
            if groups.last() != Some(&row.group.as_str()) {
                groups.push(&row.group);
            }
        }
        assert_eq!(groups, ["application", "view", "terminal", "git"]);
    }

    #[test]
    fn given_the_five_git_surfaces_when_the_settings_window_asks_for_the_shortcuts_then_it_reads_them_from_the_menu(
    ) {
        // Given — c'est le critère de l'issue #127 : le groupe git de la spec §4.4 est
        // apparu dans la fenêtre de réglages **sans qu'une ligne y soit ajoutée à la main**,
        // le jour où `build` a déclaré le sous-menu « Git ». Rien côté TypeScript ne nomme
        // ces combinaisons — la section groupe ce que le backend lui envoie
        let listed = menu_bindings().report();

        // When
        let git: Vec<(String, String)> = listed
            .rows
            .iter()
            .filter(|row| row.group == "git")
            .map(|row| (row.label.clone(), row.keys.clone()))
            .collect();

        // Then — les cinq de la spec, dans l'ordre du menu, avec la mnémonique
        // **B**ranches, **G**raph, **W**orktrees, **M**erge, **I**nfo
        assert_eq!(
            git,
            [
                ("Branches…", "⌃⌘B"),
                ("Commit Graph", "⌃⌘G"),
                ("Worktrees", "⌃⌘W"),
                ("Resolve Conflicts", "⌃⌘M"),
                ("Branch Card", "⌃⌘I"),
            ]
            .map(|(label, keys)| (label.to_owned(), keys.to_owned()))
        );
    }

    #[test]
    fn given_a_git_shortcut_rebound_by_hand_onto_a_combination_macos_takes_when_its_row_is_read_then_it_says_who_takes_it(
    ) {
        // Given — `~/.ash/shortcuts.json` est un fichier de l'utilisateur, éditable à la
        // main : rien n'empêche d'y poser `⌘⌃D` sur la popup de branches. Ash ne l'**interdit
        // pas** — c'est la règle de `reserved.rs`, et elle tient parce qu'un panneau des
        // Réglages Système peut libérer `⌘⌃D` pendant qu'Ash tourne — mais il ne se tait pas.
        // C'est bien le fichier qu'on relit ici, et pas une capture : les deux chemins ne
        // sont pas le même code, et c'est celui-là que personne ne voit passer
        use crate::features::shortcuts::BindingStore;
        let edited = Arc::new(crate::features::shortcuts::FakeBindingStore::default());
        edited
            .save(&crate::features::shortcuts::StoredBindings {
                bindings: [(
                    Action::ToggleBranches.id(),
                    Some(Combination::parse("Cmd+Ctrl+D").expect("la table écrit cette touche")),
                )]
                .into_iter()
                .collect(),
            })
            .expect("le magasin de test écrit");
        let bindings = Bindings::restore(edited, action_bindings());

        // When
        let row = bindings
            .report()
            .rows
            .into_iter()
            .find(|row| row.action == Action::ToggleBranches.id())
            .expect("la popup de branches a sa ligne");

        // Then — la ligne porte la combinaison **et** sa raison, celle-là même que le bloc
        // de capture affiche : annoncer ne ferme rien, se taire aurait laissé un raccourci
        // que macOS mange sans que rien ne le dise
        assert_eq!(row.keys, "⌃⌘D");
        assert_eq!(
            row.reservation.map(|taken| taken.note),
            Some("is reserved by macOS (look up) — ash will never receive it".to_owned())
        );
    }

    #[test]
    fn given_nothing_stopped_where_one_looks_when_the_merge_entry_is_settled_then_it_stays_dark() {
        // Given — la condition de `⌘⌃M` (spec §4.4) : « seulement pendant un rebase ou un
        // merge arrêté ». Trois situations, une seule règle
        let reach = MergeReach::default();
        let stopped = |root: &Path| root == Path::new("/wt/ash-rebase");

        // When — aucun onglet, un worktree tranquille, puis un rebase arrêté
        let mut live = vec![reach.decide(stopped)];
        reach.look_at(Some(PathBuf::from("/dev/ash")));
        live.push(reach.decide(stopped));
        reach.look_at(Some(PathBuf::from("/wt/ash-rebase")));
        live.push(reach.decide(stopped));

        // Then — un menu sans worktree sous les yeux est éteint, et il ne s'allume que là
        // où l'ouverture aboutirait (`MergeSurface::reachable`)
        assert_eq!(live, [false, false, true]);
    }

    #[test]
    fn given_a_merge_entry_already_dark_when_the_same_answer_comes_back_then_the_menu_is_left_alone(
    ) {
        // Given — la surveillance de `.git` annonce à chaque écriture du dossier, et un
        // `git add` en produit plusieurs. Reposer l'entrée à chaque fois toucherait au menu
        // natif pour rien, sur le fil qui dessine la fenêtre
        let reach = MergeReach::default();
        reach.look_at(Some(PathBuf::from("/wt/ash-rebase")));

        // When
        let changed = [reach.settle(true), reach.settle(true), reach.settle(false)];

        // Then
        assert_eq!(changed, [true, false, true]);
    }

    #[test]
    fn given_the_nine_tab_positions_when_they_are_listed_then_they_are_one_line_read_from_both_ends(
    ) {
        // Given / When — la spec §4.4 les écrit `Cmd+1 … Cmd+9`, et neuf lignes identiques
        // à un rang près feraient perdre les huit autres raccourcis dans la liste
        let listed = menu_bindings().report();
        let positions: Vec<&crate::features::shortcuts::ShortcutRow> = listed
            .rows
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
    fn given_every_action_the_menu_declares_when_its_default_is_read_then_none_of_them_is_lost_on_the_way(
    ) {
        // Given — les défauts sont écrits en chaînes, et `Combination::parse` peut les
        // refuser. Une faute de frappe dans `Cmd+Shift+T` ne casserait rien à la
        // compilation : l'entrée s'afficherait simplement sans touche, et personne ne le
        // verrait avant d'essayer le raccourci
        let declared = action_bindings();

        // When
        let lost: Vec<String> = declared
            .iter()
            .filter(|binding| {
                binding.default.is_none() && !binding.action.starts_with("view:theme:")
            })
            .map(|binding| binding.action.clone())
            .collect();

        // Then — les trois thèmes sont les seules actions sans raccourci d'origine, et elles
        // le sont **exprès** : un thème se change une fois par saison. `Clear Scrollback`
        // faisait exception jusqu'à #159 ; il ne la fait plus, et l'exemption est partie avec
        assert_eq!(lost, Vec::<String>::new());
    }

    #[test]
    fn given_a_fresh_install_when_the_menu_is_built_then_cmd_k_carries_clear_scrollback_and_it_alone(
    ) {
        // Given — l'inverse de ce que ce test verrouillait jusqu'à #159, et il faut dire
        // pourquoi : `⌘K` avait été laissé libre « pour le shell », alors que `⌘` n'atteint
        // aucun PTY — le modificateur Command n'a aucune représentation dans l'encodage
        // d'entrée d'un terminal, là où `Ctrl` produit un caractère de contrôle et `Alt`
        // préfixe par `ESC`. Personne ne recevait donc la touche, et la spec §4.4 promettait
        // un effacement que rien ne produisait. Ce que le shell garde, c'est `⌃K` — une
        // autre combinaison, qu'Ash ne déclare nulle part.
        //
        // La seconde assertion tient l'invariant « une combinaison, une action » sur ce
        // cas-ci : `⌘K` posé deux fois ferait gagner silencieusement le dernier écrit
        let bindings = menu_bindings();
        let taken = Combination::parse("Cmd+K").unwrap().accelerator();

        // When
        let carried: Vec<String> = action_bindings()
            .iter()
            .filter(|declared| bindings.accelerator(&declared.action) == Some(taken.clone()))
            .map(|declared| declared.action.clone())
            .collect();

        // Then
        assert_eq!(carried, vec![Action::ClearScrollback.id()]);
    }

    #[test]
    fn given_the_combinations_ash_will_never_receive_when_the_defaults_are_read_then_none_of_them_starts_on_one(
    ) {
        // Given — la table embarquée de `reserved.rs` dit ce qu'Ash ne recevra pas : ce que
        // macOS prend, et ce que le terminal avale. Un **défaut** posé là-dessus serait un
        // raccourci annoncé par l'écran, affiché par le menu, et sans effet — la sorte de
        // mensonge que cette tranche entière existe pour empêcher. Un utilisateur, lui, reste
        // libre d'en poser un : il l'aura lu au moment de le capturer
        let declared = action_bindings();

        // When
        let ineffective: Vec<String> = declared
            .iter()
            .filter(|binding| {
                binding
                    .default
                    .as_ref()
                    .and_then(crate::features::shortcuts::reservation)
                    .is_some()
            })
            .map(|binding| binding.action.clone())
            .collect();

        // Then
        assert_eq!(ineffective, Vec::<String>::new());
    }

    #[test]
    fn given_the_defaults_of_the_whole_menu_when_they_are_compared_then_no_two_actions_start_on_the_same_combination(
    ) {
        // Given — une entrée de menu ajoutée sur une touche déjà prise ne casserait rien
        // non plus : macOS laisse gagner la dernière entrée posée, en silence. C'est ce que
        // le bloc de conflit interdit à l'utilisateur ; les défauts n'y ont pas droit non plus
        let declared = action_bindings();

        // When
        let mut taken: Vec<String> = declared
            .iter()
            .filter_map(|binding| binding.default.as_ref().map(|one| one.accelerator()))
            .collect();
        let count = taken.len();
        taken.sort();
        taken.dedup();

        // Then
        assert_eq!(taken.len(), count);
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
