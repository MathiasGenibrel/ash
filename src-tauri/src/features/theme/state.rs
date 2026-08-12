use std::sync::{Arc, Mutex};

use super::appearance::Appearance;
use super::font_size::{FontSize, FontStep};
use super::mode::ThemeMode;
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

    /// Applique un changement, le garde sur le disque, et rend la nouvelle apparence — ou
    /// `None` si le changement n'en était pas un.
    ///
    /// Un seul chemin d'écriture pour les deux préférences : le fichier s'écrit d'un bloc,
    /// donc chaque changement doit y emporter l'autre valeur telle qu'elle est.
    fn change(&self, apply: impl FnOnce(&mut Appearance)) -> Option<Appearance> {
        let mut current = self.locked();
        let before = *current;
        apply(&mut current);
        let after = *current;
        drop(current);

        if after == before {
            return None;
        }
        let _ = self.store.save(after);
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
            *self.content.lock().unwrap()
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
