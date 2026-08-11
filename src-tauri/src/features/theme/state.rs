use std::sync::{Arc, Mutex};

use super::mode::ThemeMode;
use super::store::ThemeStore;

/// Le mode de thème courant — **la** source de vérité.
///
/// Il vit ici, en Rust, et pas dans un `useState` de la webview : le frontend rend un
/// état, il ne le détient pas
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le jour où le
/// démon `ashd` de la même ADR ouvrira une seconde fenêtre, les deux liront le même mode.
pub struct ThemeState {
    mode: Mutex<ThemeMode>,
    store: Arc<dyn ThemeStore>,
}

impl ThemeState {
    /// Repart du choix de la session précédente, ou du mode système.
    pub fn restore(store: Arc<dyn ThemeStore>) -> Self {
        let mode = store.load().unwrap_or_default();
        Self {
            mode: Mutex::new(mode),
            store,
        }
    }

    pub fn mode(&self) -> ThemeMode {
        *self.locked()
    }

    /// Retient un nouveau choix. Rend `true` si quelque chose a changé.
    ///
    /// L'écriture peut échouer — disque plein, `~/.ash` non inscriptible — et ça ne remet
    /// pas le changement en cause : le thème s'applique tout de suite, il ne survivra
    /// simplement pas au redémarrage. Refuser la bascule pour cette raison serait
    /// incompréhensible.
    pub fn set(&self, mode: ThemeMode) -> bool {
        let mut current = self.locked();
        if *current == mode {
            return false;
        }
        *current = mode;
        drop(current);

        let _ = self.store.save(mode);
        true
    }

    /// Un verrou empoisonné veut dire qu'un fil a paniqué **ailleurs** en le tenant. La
    /// valeur qu'il protège est un mode de thème : elle est intacte, et propager la
    /// panique éteindrait la fenêtre pour une préférence d'apparence.
    fn locked(&self) -> std::sync::MutexGuard<'_, ThemeMode> {
        self.mode
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
        content: Mutex<Option<ThemeMode>>,
        /// Un disque qui refuse d'écrire — plein, ou en lecture seule.
        read_only: bool,
    }

    impl ThemeStore for FakeStore {
        fn load(&self) -> Option<ThemeMode> {
            *self.content.lock().unwrap()
        }

        fn save(&self, mode: ThemeMode) -> Result<(), ThemeError> {
            if self.read_only {
                return Err(ThemeError::Io {
                    path: std::path::PathBuf::from("/dev/null/theme.json"),
                    why: "lecture seule".to_owned(),
                });
            }
            *self.content.lock().unwrap() = Some(mode);
            Ok(())
        }
    }

    #[test]
    fn given_a_choice_made_in_a_previous_session_when_ash_starts_again_then_it_opens_on_that_theme()
    {
        // Given
        let store = Arc::new(FakeStore::default());
        ThemeState::restore(Arc::clone(&store) as Arc<dyn ThemeStore>).set(ThemeMode::Dark);

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
        let changed = state.set(ThemeMode::Dark);

        // Then
        assert!(changed);
        assert_eq!(state.mode(), ThemeMode::Dark);
    }

    #[test]
    fn given_the_theme_already_in_use_when_it_is_chosen_again_then_nothing_changes() {
        // Given — l'entrée de menu déjà cochée reste cliquable
        let state = ThemeState::restore(Arc::new(FakeStore::default()));
        state.set(ThemeMode::Light);

        // When
        let changed = state.set(ThemeMode::Light);

        // Then — sans ça, chaque clic réécrirait le fichier et réémettrait un event
        assert!(!changed);
    }
}
