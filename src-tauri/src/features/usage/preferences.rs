//! L'interrupteur qui coupe tout appel sortant, et le fichier qui s'en souvient.
//!
//! [ADR-0016](../../../../docs/adr/0016-ash-sort-sur-le-reseau.md), condition 3, second
//! sens : « l'utilisateur doit pouvoir savoir qu'Ash appelle, et **le couper**. Un
//! interrupteur dans la fenêtre de réglages, détenu par la feature concernée et persisté
//! comme les trois de la spec §9. Il existe **dès la première fonctionnalité réseau**, pas
//! au jour où quelqu'un le demande. »
//!
//! C'est donc **le mécanisme de `features/agents/preferences.rs`**, à un booléen près : un
//! petit fichier dans le dossier privé d'Ash, relu au démarrage, tolérant à tout, derrière
//! un trait que la feature possède.
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | [`UsageStore`] | [`FileUsageStore`] — `~/.ash/usage.json` | `FakeStore` (ci-dessous) |
//!
//! **L'interrupteur est consulté sur le chemin qui agit**, jamais par l'écran : c'est
//! [`UsagePoller`](super::UsagePoller) qui le lit, juste avant de décider qu'un appel part.
//! Un filtre d'interface ne cacherait que le chiffre, pas le paquet — et c'est le paquet que
//! l'ADR donne à couper.
//!
//! ## Pourquoi allumé par défaut
//!
//! C'est la même réponse qu'ADR-0017 donne à `claude setup-token` : une fonctionnalité
//! périphérique qui demande un geste avant d'exister n'existe pour personne. L'appel est un
//! `GET` par minute, et seulement quand la fenêtre est devant.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Ce que l'utilisateur laisse Ash appeler.
///
/// Une `struct` d'un seul champ plutôt qu'un `bool` nu, et pour la raison qui a fait celle
/// des notifications : le jour où une seconde destination réseau existera, elle aura son
/// champ, et un fichier écrit par l'Ash d'avant se relira sans rien perdre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageChoices {
    /// Ash demande-t-il les quotas à l'hôte d'ADR-0016 ? Allumé par défaut — voir l'en-tête.
    #[serde(default = "calling")]
    pub polling: bool,
}

fn calling() -> bool {
    true
}

impl Default for UsageChoices {
    fn default() -> Self {
        Self { polling: true }
    }
}

/// Où l'interrupteur se garde d'une session à l'autre.
pub trait UsageStore: Send + Sync {
    /// Ce qui est gardé, ou `None` — première ouverture, fichier absent, fichier abîmé.
    fn load(&self) -> Option<UsageChoices>;
    fn save(&self, choices: UsageChoices) -> Result<(), std::io::Error>;
}

/// L'interrupteur dans `~/.ash/usage.json`, à côté de `theme.json` et `notifications.json`.
pub struct FileUsageStore {
    path: PathBuf,
}

impl FileUsageStore {
    #[must_use]
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// `~/.ash/usage.json`.
    #[must_use]
    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::at(home.join(".ash").join("usage.json"))
    }
}

impl UsageStore for FileUsageStore {
    /// **Tolérante à tout**, comme ses deux voisines : un fichier absent, tronqué, vide ou
    /// rempli d'autre chose rend `None`. Une préférence n'est jamais une raison d'empêcher
    /// une fenêtre d'ouvrir.
    fn load(&self) -> Option<UsageChoices> {
        decode(&std::fs::read_to_string(&self.path).ok()?)
    }

    fn save(&self, choices: UsageChoices) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, encode(choices))
    }
}

fn decode(content: &str) -> Option<UsageChoices> {
    serde_json::from_str::<UsageChoices>(content).ok()
}

fn encode(choices: UsageChoices) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&choices).unwrap_or_else(|_| String::from("{}"))
    )
}

/// Ce que l'utilisateur laisse sortir — **la** source de vérité.
pub struct UsagePreferences {
    current: Mutex<UsageChoices>,
    store: Arc<dyn UsageStore>,
}

impl UsagePreferences {
    /// Repart du choix de la session précédente, ou du défaut de l'en-tête.
    pub fn restore(store: Arc<dyn UsageStore>) -> Self {
        let current = store.load().unwrap_or_default();
        Self {
            current: Mutex::new(current),
            store,
        }
    }

    pub fn polling(&self) -> bool {
        self.locked().polling
    }

    /// Met l'interrupteur dans cette position. Rend `true` s'il a changé.
    ///
    /// L'écriture peut échouer — disque plein, `~/.ash` non inscriptible — et ça ne remet
    /// pas le choix en cause : il s'applique tout de suite, il ne survivra simplement pas au
    /// redémarrage. C'est la conduite des deux autres préférences du dépôt.
    pub fn set_polling(&self, enabled: bool) -> bool {
        let mut current = self.locked();
        if current.polling == enabled {
            return false;
        }
        current.polling = enabled;
        let after = *current;
        drop(current);
        let _ = self.store.save(after);
        true
    }

    /// Un verrou empoisonné veut dire qu'un fil a paniqué **ailleurs** en le tenant. Ce
    /// qu'il protège est un booléen : il est intact, et propager la panique ferait tomber
    /// le fil de fond pour un réglage.
    fn locked(&self) -> std::sync::MutexGuard<'_, UsageChoices> {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeStore {
        content: Mutex<Option<UsageChoices>>,
    }

    impl UsageStore for FakeStore {
        fn load(&self) -> Option<UsageChoices> {
            *self.content.lock().unwrap()
        }

        fn save(&self, choices: UsageChoices) -> Result<(), std::io::Error> {
            *self.content.lock().unwrap() = Some(choices);
            Ok(())
        }
    }

    #[test]
    fn given_a_user_who_cut_the_calls_in_a_previous_session_when_ash_starts_again_then_they_are_still_cut(
    ) {
        // Given — l'interrupteur d'ADR-0016 : couper les appels sortants et les voir
        // reprendre au prochain lancement reviendrait à ne pas l'avoir
        let store = Arc::new(FakeStore::default());
        let first = UsagePreferences::restore(Arc::clone(&store) as Arc<dyn UsageStore>);
        first.set_polling(false);

        // When
        let next = UsagePreferences::restore(store as Arc<dyn UsageStore>);

        // Then
        assert!(!next.polling());
    }

    #[test]
    fn given_a_preferences_file_that_says_nothing_understandable_when_it_is_read_then_ash_falls_back_to_calling(
    ) {
        // Given — un fichier tronqué par une coupure, vidé, ou édité à la main ; et le sens
        // qu'on ne peut pas jouer en revenant en arrière : un fichier écrit par un Ash qui
        // porte un interrupteur de plus
        let unreadable = ["", "{", "null", "\"polling\""];
        let from_a_later_ash = "{\"polling\":false,\"telemetry\":true}";

        // When
        let read: Vec<Option<UsageChoices>> = unreadable.iter().map(|c| decode(c)).collect();
        let survivor = decode(from_a_later_ash);

        // Then — un champ inconnu se laisse tomber sans emporter celui qu'on comprend
        assert_eq!(read, vec![None; unreadable.len()]);
        assert_eq!(survivor, Some(UsageChoices { polling: false }));
    }
}
