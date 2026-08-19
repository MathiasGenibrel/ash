//! Le fichier de liaisons, en mémoire — l'adaptateur de test du port [`BindingStore`].
//!
//! Il est ici plutôt que dans le module de tests de `bindings.rs` parce qu'il a deux
//! lecteurs : les tests de la feature, et ceux de `src-tauri/src/menu.rs`, qui vérifient que
//! le menu réel dérive bien de ses liaisons. Le second n'a pas à connaître l'intérieur du
//! premier.

use std::sync::Mutex;

use super::error::ShortcutError;
use super::store::{BindingStore, StoredBindings};

#[derive(Default)]
pub struct FakeBindingStore {
    content: Mutex<Option<StoredBindings>>,
    /// Un disque qui refuse d'écrire — plein, ou en lecture seule.
    read_only: bool,
}

impl FakeBindingStore {
    pub fn read_only() -> Self {
        Self {
            read_only: true,
            ..Self::default()
        }
    }
}

impl BindingStore for FakeBindingStore {
    fn load(&self) -> Option<StoredBindings> {
        self.locked().clone()
    }

    fn save(&self, bindings: &StoredBindings) -> Result<(), ShortcutError> {
        if self.read_only {
            return Err(ShortcutError::Io {
                path: std::path::PathBuf::from("/dev/null/shortcuts.json"),
                why: "lecture seule".to_owned(),
            });
        }
        *self.locked() = Some(bindings.clone());
        Ok(())
    }
}

impl FakeBindingStore {
    fn locked(&self) -> std::sync::MutexGuard<'_, Option<StoredBindings>> {
        self.content
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
