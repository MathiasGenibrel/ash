use std::sync::{Arc, Mutex};

use super::appearance::Appearance;
use super::bottom_panel::{BottomPanel, PanelHeight, PanelView};
use super::density::SidebarDensity;
use super::font::TerminalFont;
use super::font_size::{FontSize, FontStep};
use super::mode::ThemeMode;
use super::sidebar_column::{SidebarColumn, SidebarWidth};
use super::status_bar::{StatusBarItem, StatusBarLayout, StatusBarSegment};
use super::store::ThemeStore;

/// L'apparence courante de la fenêtre — **la** source de vérité.
///
/// Elle vit ici, en Rust, et pas dans un `useState` de la webview : le frontend rend un
/// état, il ne le détient pas
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le jour où le
/// démon `ashd` de la même ADR ouvrira une seconde fenêtre, les deux liront le même mode
/// **et** la même taille de police.
pub struct ThemeState {
    current: Mutex<Appearance>,
    store: Arc<dyn ThemeStore>,
}

impl ThemeState {
    /// Repart des choix de la session précédente, ou du mode système et de la taille par
    /// défaut.
    pub fn restore(store: Arc<dyn ThemeStore>) -> Self {
        let current = store.load().unwrap_or_default();
        Self {
            current: Mutex::new(current),
            store,
        }
    }

    pub fn mode(&self) -> ThemeMode {
        self.locked().mode
    }

    pub fn font_size(&self) -> FontSize {
        self.locked().font_size
    }

    pub fn font(&self) -> TerminalFont {
        self.locked().font.clone()
    }

    pub fn density(&self) -> SidebarDensity {
        self.locked().density
    }

    /// Retient un nouveau thème. Rend `true` si quelque chose a changé.
    ///
    /// L'écriture peut échouer — disque plein, `~/.ash` non inscriptible — et ça ne remet
    /// pas le changement en cause : le thème s'applique tout de suite, il ne survivra
    /// simplement pas au redémarrage. Refuser la bascule pour cette raison serait
    /// incompréhensible.
    pub fn set_mode(&self, mode: ThemeMode) -> bool {
        self.change(|appearance| appearance.mode = mode).is_some()
    }

    /// Joue un pas de taille de police. Rend la nouvelle taille, ou `None` si rien n'a
    /// bougé — une borne atteinte, ou `Cmd+0` sur une taille déjà par défaut.
    ///
    /// Rendre la taille plutôt qu'un booléen évite au menu de la redemander juste après
    /// pour l'annoncer : c'est ici qu'elle est décidée, donc ici qu'on la connaît.
    pub fn step_font(&self, step: FontStep) -> Option<FontSize> {
        self.change(|appearance| appearance.font_size = appearance.font_size.stepped(step))
            .map(|appearance| appearance.font_size)
    }

    pub fn sidebar_column(&self) -> SidebarColumn {
        self.locked().sidebar
    }

    /// Retient la largeur que la webview vient de régler au glissement, et **déplie** la
    /// colonne.
    ///
    /// Les deux d'un coup, parce que c'est un seul geste : on ne redimensionne pas une
    /// colonne repliée, donc une largeur qui arrive dit aussi que la colonne est ouverte.
    /// Rend la colonne si quelque chose a bougé — un glissement qui se termine là où il a
    /// commencé n'a rien à annoncer.
    pub fn set_sidebar_width(&self, width: SidebarWidth) -> Option<SidebarColumn> {
        self.change(|appearance| {
            appearance.sidebar.width = width;
            appearance.sidebar.collapsed = false;
        })
        .map(|appearance| appearance.sidebar)
    }

    /// Replie ou déplie la colonne — `⌘B`, la touche du séparateur, ou un glissement relâché
    /// sous le plancher.
    ///
    /// La largeur n'est **pas** touchée : c'est ce qui fait qu'une colonne rouverte retrouve
    /// celle qu'elle avait avant d'être refermée.
    pub fn set_sidebar_collapsed(&self, collapsed: bool) -> Option<SidebarColumn> {
        self.change(|appearance| appearance.sidebar.collapsed = collapsed)
            .map(|appearance| appearance.sidebar)
    }

    /// `⌘B` : l'inverse de ce que la colonne est **maintenant**.
    ///
    /// La bascule est calculée ici, sous le verrou, et non par l'appelante à partir d'un état
    /// qu'elle aurait lu juste avant : c'est ce qui empêche deux gestes rapprochés de se
    /// répondre le même état. Rend toujours la colonne — une bascule change toujours quelque
    /// chose.
    pub fn toggle_sidebar_collapsed(&self) -> SidebarColumn {
        let collapsed = !self.locked().sidebar.collapsed;
        self.set_sidebar_collapsed(collapsed)
            .unwrap_or_else(|| self.sidebar_column())
    }

    pub fn bottom_panel(&self) -> BottomPanel {
        self.locked().panel
    }

    /// Retient la hauteur que la webview vient de régler au glissement, et **ouvre** le
    /// panneau.
    ///
    /// Les deux d'un coup, comme pour la colonne de gauche : on ne redimensionne pas un
    /// panneau fermé, donc une hauteur qui arrive dit aussi qu'il est ouvert. Rend le panneau
    /// si quelque chose a bougé — un glissement qui se termine là où il a commencé n'a rien à
    /// annoncer, et chaque annonce refait la grille du terminal visible.
    pub fn set_panel_height(&self, height: PanelHeight) -> Option<BottomPanel> {
        self.change(|appearance| {
            appearance.panel.height = height;
            appearance.panel.open = true;
        })
        .map(|appearance| appearance.panel)
    }

    /// Demande une vue — `⌘⌃G`, `⌘⌃W`, `⌘⌃I`, ou le clic sur une entrée de la barre. La vue
    /// `conflicts` n'a pas de raccourci : `⌘⌃M` ouvre l'**onglet** de merge, pas cette vue.
    ///
    /// **La même vue redemandée referme le panneau**, et c'est la règle qui fait de ces
    /// gestes des bascules : `⌘⌃G` ouvre le graphe, `⌘⌃G` de nouveau rend la hauteur au
    /// terminal. Une autre vue, elle, remplace celle qui est montrée sans jamais refermer —
    /// personne ne demande `worktrees` pour se retrouver devant un terminal.
    ///
    /// La règle est **ici**, sous le verrou, et pas dans la webview qui l'a demandée : elle
    /// se lit sur l'état courant, et une webview qui l'appliquerait elle-même en deviendrait
    /// le second détenteur ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    /// C'est aussi ce qui empêche deux surfaces — un raccourci et un clic — de se contredire.
    ///
    /// La hauteur n'est **jamais** touchée : c'est ce qui fait qu'un panneau rouvert retrouve
    /// celle qu'il avait.
    pub fn show_panel_view(&self, view: PanelView) -> Option<BottomPanel> {
        let showing = {
            let panel = self.locked().panel;
            panel.open && panel.view == view
        };
        self.change(|appearance| {
            if showing {
                appearance.panel.open = false;
            } else {
                appearance.panel.view = view;
                appearance.panel.open = true;
            }
        })
        .map(|appearance| appearance.panel)
    }

    /// Referme le panneau — il **rend sa hauteur au terminal**
    /// ([ADR-0003](../../../../docs/adr/0003-zone-terminal-unique.md)), et la garde pour la
    /// prochaine ouverture.
    pub fn close_panel(&self) -> Option<BottomPanel> {
        self.change(|appearance| appearance.panel.open = false)
            .map(|appearance| appearance.panel)
    }

    /// Retient une police de terminal. Rend `true` si quelque chose a changé.
    ///
    /// **Rien ne vérifie ici qu'elle est installée**, et c'est le même raisonnement que pour
    /// une écriture qui échoue : le catalogue est un effet système qui change entre deux
    /// démarrages, et une préférence refusée parce qu'une police a été désinstallée laisserait
    /// le terminal sans police plutôt qu'avec une face de repli.
    pub fn set_font(&self, font: TerminalFont) -> bool {
        self.change(|appearance| appearance.font = font).is_some()
    }

    /// Retient une densité de sidebar. Rend `true` si quelque chose a changé.
    pub fn set_density(&self, density: SidebarDensity) -> bool {
        self.change(|appearance| appearance.density = density)
            .is_some()
    }

    /// Ce que la ligne de statut montre, et dans quel ordre (spec §4.2, vues 5c et 5e).
    pub fn status_bar(&self) -> StatusBarLayout {
        self.locked().status_bar.clone()
    }

    /// Coche ou décoche un segment de la ligne de statut, et rend la barre qui en résulte.
    ///
    /// La bascule est calculée **ici, sous le verrou**, et non par le menu à partir d'un
    /// état qu'il aurait lu en s'ouvrant : c'est la conduite de `toggle_sidebar_collapsed`,
    /// et pour la même raison — deux gestes rapprochés ne peuvent pas se répondre la même
    /// barre. Rend toujours une valeur : une bascule change toujours quelque chose.
    pub fn toggle_status_bar_segment(&self, segment: StatusBarSegment) -> StatusBarLayout {
        self.change(|appearance| appearance.status_bar = appearance.status_bar.toggled(segment))
            .map_or_else(|| self.status_bar(), |appearance| appearance.status_bar)
    }

    /// La disposition que le mode édition vient de composer — un glissement relâché, un
    /// spacer posé, un spacer jeté.
    ///
    /// C'est la webview qui **propose** la suite, et le backend qui la retient : le chemin
    /// de `set_bottom_panel_height`, et pour la même raison. Réordonner est de la
    /// manipulation directe, elle se mesure en pixels sous le pointeur, et faire un
    /// aller-retour Tauri par `dragenter` rendrait le glissement saccadé sans rien garantir
    /// de plus. Ce qui **survit** à la fermeture reste ici
    /// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et ce qui arrive
    /// est ramené à ses invariants par [`StatusBarLayout::normalized`] — la webview propose,
    /// elle ne dicte pas.
    ///
    /// Rend `None` quand un glissement se termine là où il a commencé : rien à annoncer, et
    /// rien à écrire sur le disque.
    pub fn set_status_bar_layout(&self, items: Vec<StatusBarItem>) -> Option<StatusBarLayout> {
        let proposed = StatusBarLayout::normalized(items);
        self.change(|appearance| appearance.status_bar = proposed)
            .map(|appearance| appearance.status_bar)
    }

    /// La barre par défaut — le `reset all` des raccourcis (spec §4.4), appliqué à la ligne
    /// de statut.
    ///
    /// C'est ce qui rend une barre vidée récupérable : le tiroir du mode édition l'offre, et
    /// il s'ouvre même quand il n'y a plus rien à quoi le geste puisse s'accrocher.
    pub fn reset_status_bar_layout(&self) -> StatusBarLayout {
        self.change(|appearance| appearance.status_bar = StatusBarLayout::reset())
            .map_or_else(|| self.status_bar(), |appearance| appearance.status_bar)
    }

    /// Applique un changement, le garde sur le disque, et rend la nouvelle apparence — ou
    /// `None` si le changement n'en était pas un.
    ///
    /// Un seul chemin d'écriture pour les deux préférences : le fichier s'écrit d'un bloc,
    /// donc chaque changement doit y emporter l'autre valeur telle qu'elle est.
    fn change(&self, apply: impl FnOnce(&mut Appearance)) -> Option<Appearance> {
        let mut current = self.locked();
        let before = current.clone();
        apply(&mut current);
        let after = current.clone();
        drop(current);

        if after == before {
            return None;
        }
        let _ = self.store.save(after.clone());
        Some(after)
    }

    /// Un verrou empoisonné veut dire qu'un fil a paniqué **ailleurs** en le tenant. La
    /// valeur qu'il protège est une préférence d'apparence : elle est intacte, et propager
    /// la panique éteindrait la fenêtre pour un thème et une taille de police.
    fn locked(&self) -> std::sync::MutexGuard<'_, Appearance> {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::theme::error::ThemeError;

    /// Le fichier de préférence, en mémoire.
    #[derive(Default)]
    struct FakeStore {
        content: Mutex<Option<Appearance>>,
        /// Un disque qui refuse d'écrire — plein, ou en lecture seule.
        read_only: bool,
    }

    impl ThemeStore for FakeStore {
        fn load(&self) -> Option<Appearance> {
            self.content.lock().unwrap().clone()
        }

        fn save(&self, appearance: Appearance) -> Result<(), ThemeError> {
            if self.read_only {
                return Err(ThemeError::Io {
                    path: std::path::PathBuf::from("/dev/null/theme.json"),
                    why: "lecture seule".to_owned(),
                });
            }
            *self.content.lock().unwrap() = Some(appearance);
            Ok(())
        }
    }

    #[test]
    fn given_a_choice_made_in_a_previous_session_when_ash_starts_again_then_it_opens_on_that_theme()
    {
        // Given
        let store = Arc::new(FakeStore::default());
        ThemeState::restore(Arc::clone(&store) as Arc<dyn ThemeStore>).set_mode(ThemeMode::Dark);

        // When — la session suivante
        let next = ThemeState::restore(store as Arc<dyn ThemeStore>);

        // Then
        assert_eq!(next.mode(), ThemeMode::Dark);
    }

    #[test]
    fn given_no_choice_ever_made_when_ash_starts_then_it_follows_the_system() {
        // Given / When — première ouverture sur une machine
        let state = ThemeState::restore(Arc::new(FakeStore::default()));

        // Then
        assert_eq!(state.mode(), ThemeMode::System);
    }

    #[test]
    fn given_a_preference_that_cannot_be_written_when_the_user_picks_a_theme_then_it_still_applies()
    {
        // Given — `~/.ash` non inscriptible : refuser la bascule pour cette raison serait
        // incompréhensible pour qui vient de cliquer « Dark »
        let state = ThemeState::restore(Arc::new(FakeStore {
            read_only: true,
            ..FakeStore::default()
        }));

        // When
        let changed = state.set_mode(ThemeMode::Dark);

        // Then
        assert!(changed);
        assert_eq!(state.mode(), ThemeMode::Dark);
    }

    #[test]
    fn given_the_theme_already_in_use_when_it_is_chosen_again_then_nothing_changes() {
        // Given — l'entrée de menu déjà cochée reste cliquable
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        state.set_mode(ThemeMode::Light);

        // When
        let changed = state.set_mode(ThemeMode::Light);

        // Then — sans ça, chaque clic réécrirait le fichier et réémettrait un event
        assert!(!changed);
    }

    #[test]
    fn given_a_font_size_chosen_in_a_previous_session_when_ash_starts_again_then_it_opens_at_that_size(
    ) {
        // Given — la taille suit le chemin du thème : même fichier, même relecture
        let store = Arc::new(FakeStore::default());
        ThemeState::restore(Arc::clone(&store) as Arc<dyn ThemeStore>).step_font(FontStep::Bigger);

        // When — la session suivante
        let next = ThemeState::restore(store as Arc<dyn ThemeStore>);

        // Then
        assert_eq!(
            next.font_size(),
            FontSize::DEFAULT.stepped(FontStep::Bigger)
        );
    }

    #[test]
    fn given_a_theme_and_a_font_size_when_one_of_them_changes_then_the_other_is_kept_on_disk() {
        // Given — les deux préférences partagent un fichier qui s'écrit d'un bloc : c'est
        // exactement là qu'une écriture peut effacer l'autre valeur
        let store = Arc::new(FakeStore::default());
        let state = ThemeState::restore(Arc::clone(&store) as Arc<dyn ThemeStore>);
        state.set_mode(ThemeMode::Dark);

        // When
        state.step_font(FontStep::Smaller);

        // Then
        let next = ThemeState::restore(store as Arc<dyn ThemeStore>);
        assert_eq!(next.mode(), ThemeMode::Dark);
        assert_eq!(
            next.font_size(),
            FontSize::DEFAULT.stepped(FontStep::Smaller)
        );
    }

    #[test]
    fn given_a_column_the_user_has_widened_when_it_is_collapsed_and_reopened_then_it_is_that_wide_again(
    ) {
        // Given — une colonne posée au tiers de la fenêtre
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        state.set_sidebar_width(SidebarWidth::from(420));

        // When — `⌘B`, puis `⌘B` de nouveau
        state.toggle_sidebar_collapsed();
        let reopened = state.toggle_sidebar_collapsed();

        // Then — replier ne perd pas la largeur, sans quoi chaque `⌘B` ramènerait 240 px
        assert!(!reopened.collapsed);
        assert_eq!(reopened.width, SidebarWidth::from(420));
    }

    #[test]
    fn given_a_column_resized_in_a_previous_session_when_ash_starts_again_then_it_opens_that_wide()
    {
        // Given — la largeur suit le chemin du thème : même fichier, même relecture
        let store = Arc::new(FakeStore::default());
        let state = ThemeState::restore(Arc::clone(&store) as Arc<dyn ThemeStore>);
        state.set_sidebar_width(SidebarWidth::from(310));
        state.set_sidebar_collapsed(true);

        // When — la session suivante
        let next = ThemeState::restore(store as Arc<dyn ThemeStore>);

        // Then — la largeur **et** le repli, parce que `⌘B` et la poignée sont un seul état
        assert_eq!(
            next.sidebar_column(),
            SidebarColumn {
                width: SidebarWidth::from(310),
                collapsed: true,
            }
        );
    }

    #[test]
    fn given_a_collapsed_column_when_a_drag_sets_a_width_then_the_column_is_open_again() {
        // Given — la colonne repliée par `⌘B`, puis rouverte à la poignée
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        state.set_sidebar_collapsed(true);

        // When
        let announced = state.set_sidebar_width(SidebarWidth::from(280));

        // Then — on ne redimensionne pas une colonne refermée : une largeur qui arrive dit
        // aussi qu'elle est ouverte
        assert_eq!(
            announced,
            Some(SidebarColumn {
                width: SidebarWidth::from(280),
                collapsed: false,
            })
        );
    }

    #[test]
    fn given_a_drag_that_ends_where_it_started_when_the_width_is_announced_then_nothing_is_emitted()
    {
        // Given — attraper le bord, bouger, revenir : c'est le geste le plus courant après
        // une hésitation
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        state.set_sidebar_width(SidebarWidth::from(300));

        // When
        let announced = state.set_sidebar_width(SidebarWidth::from(300));

        // Then — sans ça, chaque relâchement réécrirait le fichier et referait la grille de
        // tous les terminaux ouverts
        assert_eq!(announced, None);
    }

    #[test]
    fn given_the_panel_showing_the_graph_when_the_graph_is_asked_for_again_then_it_closes_and_keeps_its_height(
    ) {
        // Given — `⌘⌃G` a ouvert le graphe, sur une hauteur que l'utilisateur a réglée
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        state.set_panel_height(PanelHeight::from(300));

        // When — `⌘⌃G` de nouveau
        let announced = state.show_panel_view(PanelView::Graph);

        // Then — le panneau rend sa hauteur au terminal (ADR-0003) sans l'oublier
        assert_eq!(
            announced,
            Some(BottomPanel {
                height: PanelHeight::from(300),
                open: false,
                view: PanelView::Graph,
            })
        );
    }

    #[test]
    fn given_the_panel_showing_the_graph_when_another_view_is_asked_for_then_it_switches_without_closing(
    ) {
        // Given — personne ne demande `worktrees` pour se retrouver devant un terminal
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        state.show_panel_view(PanelView::Graph);

        // When
        let announced = state.show_panel_view(PanelView::Worktrees);

        // Then
        assert_eq!(
            announced.map(|panel| (panel.open, panel.view)),
            Some((true, PanelView::Worktrees))
        );
    }

    #[test]
    fn given_a_closed_panel_when_the_view_it_last_showed_is_asked_for_then_it_opens_on_it() {
        // Given — refermé sur `worktrees` : c'est là qu'il doit rouvrir, pas sur le défaut
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        state.show_panel_view(PanelView::Worktrees);
        state.close_panel();

        // When
        let announced = state.show_panel_view(PanelView::Worktrees);

        // Then — sans ça, la bascule refermerait un panneau déjà fermé et rien ne s'ouvrirait
        assert_eq!(
            announced.map(|panel| (panel.open, panel.view)),
            Some((true, PanelView::Worktrees))
        );
    }

    #[test]
    fn given_a_panel_the_user_has_resized_when_ash_starts_again_then_it_opens_at_that_height() {
        // Given — la hauteur suit le chemin du thème : même fichier, même relecture
        let store = Arc::new(FakeStore::default());
        let state = ThemeState::restore(Arc::clone(&store) as Arc<dyn ThemeStore>);
        state.set_panel_height(PanelHeight::from(340));
        state.show_panel_view(PanelView::Conflicts);

        // When — la session suivante
        let next = ThemeState::restore(store as Arc<dyn ThemeStore>);

        // Then — les trois faces d'un même geste, gardées ensemble
        assert_eq!(
            next.bottom_panel(),
            BottomPanel {
                height: PanelHeight::from(340),
                open: true,
                view: PanelView::Conflicts,
            }
        );
    }

    #[test]
    fn given_a_drag_that_ends_where_it_started_when_the_panel_height_is_announced_then_nothing_is_emitted(
    ) {
        // Given — attraper le bord, bouger, revenir : le geste le plus courant après une
        // hésitation
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        state.set_panel_height(PanelHeight::from(260));

        // When
        let announced = state.set_panel_height(PanelHeight::from(260));

        // Then — sans ça, chaque relâchement réécrirait le fichier et referait la grille du
        // terminal visible, donc un `SIGWINCH` gratuit sous la TUI qui y tourne
        assert_eq!(announced, None);
    }

    #[test]
    fn given_an_already_closed_panel_when_it_is_closed_again_then_nothing_is_announced() {
        // Given — `Échap` sur un panneau déjà fermé, ou une webview qui répète son geste
        let state = ThemeState::restore(Arc::new(FakeStore::default()));

        // When / Then
        assert_eq!(state.close_panel(), None);
    }

    #[test]
    fn given_a_font_chosen_in_a_previous_session_when_ash_starts_again_then_it_opens_on_that_font()
    {
        // Given — la police suit le chemin du thème : même fichier, même relecture. C'est le
        // critère « le choix survit au redémarrage » (#22)
        let store = Arc::new(FakeStore::default());
        let chosen = TerminalFont::new("SF Mono").unwrap();
        ThemeState::restore(Arc::clone(&store) as Arc<dyn ThemeStore>).set_font(chosen.clone());

        // When — la session suivante
        let next = ThemeState::restore(store as Arc<dyn ThemeStore>);

        // Then
        assert_eq!(next.font(), chosen);
    }

    #[test]
    fn given_a_font_that_is_not_installed_anymore_when_it_is_chosen_then_it_is_still_retained() {
        // Given — le catalogue est un effet système : une police désinstallée entre deux
        // démarrages ne doit pas laisser le terminal sans préférence du tout
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        let gone = TerminalFont::new("Une police désinstallée").unwrap();

        // When
        let changed = state.set_font(gone.clone());

        // Then — la webview retombera sur sa face de repli, et le choix reste lisible
        assert!(changed);
        assert_eq!(state.font(), gone);
    }

    #[test]
    fn given_a_density_chosen_in_a_previous_session_when_ash_starts_again_then_the_sidebar_opens_on_it(
    ) {
        // Given — quatrième préférence du même fichier, même règle
        let store = Arc::new(FakeStore::default());
        ThemeState::restore(Arc::clone(&store) as Arc<dyn ThemeStore>)
            .set_density(SidebarDensity::Compact);

        // When
        let next = ThemeState::restore(store as Arc<dyn ThemeStore>);

        // Then
        assert_eq!(next.density(), SidebarDensity::Compact);
    }

    #[test]
    fn given_the_density_already_in_use_when_it_is_chosen_again_then_nothing_is_announced() {
        // Given — le segment actif reste cliquable, et chaque annonce fait repeindre la
        // sidebar de toutes les fenêtres
        let state = ThemeState::restore(Arc::new(FakeStore::default()));

        // When / Then
        assert!(!state.set_density(SidebarDensity::Comfortable));
    }

    #[test]
    fn given_a_font_size_already_at_its_floor_when_the_user_asks_for_smaller_then_nothing_is_announced(
    ) {
        // Given — `Cmd+-` maintenu enfoncé sur la plus petite taille lisible
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        while state.step_font(FontStep::Smaller).is_some() {}

        // When
        let announced = state.step_font(FontStep::Smaller);

        // Then — sans ça, chaque frappe réécrirait le fichier et ferait repeindre tous les
        // terminaux pour une taille qui n'a pas bougé
        assert_eq!(announced, None);
    }
}
