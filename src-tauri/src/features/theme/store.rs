use std::path::PathBuf;

use super::error::ThemeError;
use super::mode::ThemeMode;

/// Où le choix de thème se garde d'une session à l'autre.
///
/// Un trait, comme tous les effets système de ce dépôt : sans lui, vérifier qu'un choix
/// survit au redémarrage demanderait d'écrire dans le `$HOME` de qui lance les tests.
pub trait ThemeStore: Send + Sync {
    /// Le choix gardé, ou `None` — première ouverture, fichier absent, fichier abîmé.
    fn load(&self) -> Option<ThemeMode>;
    fn save(&self, mode: ThemeMode) -> Result<(), ThemeError>;
}

/// Ce que le fichier contient. Un objet, et pas une chaîne nue : le jour où une seconde
/// préférence d'apparence s'y ajoute (#22), le fichier n'a pas à changer de forme.
#[derive(serde::Serialize, serde::Deserialize)]
struct Stored {
    mode: ThemeMode,
}

/// Le choix dans `~/.ash/theme.json`.
///
/// `~/.ash` existe déjà — c'est là que vit le socket d'events. Un fichier JSON de trente
/// octets y est moins cher qu'une dépendance de préférences, et lisible à l'œil nu.
pub struct FileThemeStore {
    path: PathBuf,
}

impl FileThemeStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// `~/.ash/theme.json`.
    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::at(home.join(".ash").join("theme.json"))
    }
}

impl ThemeStore for FileThemeStore {
    /// **Tolérante à tout.** Un fichier absent, tronqué, vide ou rempli d'autre chose rend
    /// `None`, et Ash repart sur le mode système. Une préférence d'apparence n'est jamais
    /// une raison d'empêcher une fenêtre d'ouvrir.
    fn load(&self) -> Option<ThemeMode> {
        decode(&std::fs::read_to_string(&self.path).ok()?)
    }

    fn save(&self, mode: ThemeMode) -> Result<(), ThemeError> {
        let io = |why: std::io::Error| ThemeError::Io {
            path: self.path.clone(),
            why: why.to_string(),
        };

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&self.path, encode(mode)).map_err(io)
    }
}

/// Le contenu du fichier, ou `None` s'il ne dit rien qu'on comprenne.
fn decode(content: &str) -> Option<ThemeMode> {
    serde_json::from_str::<Stored>(content)
        .ok()
        .map(|stored| stored.mode)
}

fn encode(mode: ThemeMode) -> String {
    // `to_string` et non `to_string_pretty` : trente octets sur une ligne, terminés par un
    // saut de ligne pour que le fichier reste lisible dans un terminal.
    format!(
        "{}\n",
        serde_json::to_string(&Stored { mode }).unwrap_or_else(|_| String::from("{}"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_stored_choice_when_it_is_read_back_then_it_is_the_same_choice() {
        // Given — le fichier est le seul lien entre deux sessions ; sa forme est un
        // contrat avec la version d'Ash de demain
        let written = encode(ThemeMode::Dark);

        // When
        let read = decode(&written);

        // Then
        assert_eq!(read, Some(ThemeMode::Dark));
    }

    #[test]
    fn given_a_preference_file_that_says_nothing_understandable_when_it_is_read_then_ash_falls_back_to_the_system(
    ) {
        // Given — un fichier tronqué par une coupure, vidé, ou édité à la main
        let broken = ["", "{", "{\"mode\":\"solarized\"}", "null", "[]"];

        // When
        let read: Vec<Option<ThemeMode>> = broken.iter().map(|c| decode(c)).collect();

        // Then — une préférence d'apparence n'empêche jamais une fenêtre d'ouvrir
        assert_eq!(read, vec![None; broken.len()]);
    }

    #[test]
    fn given_a_choice_saved_to_disk_when_a_new_session_loads_it_then_it_survived_the_restart() {
        // Given
        let path = std::env::temp_dir()
            .join(format!("ash-theme-{}", std::process::id()))
            .join("theme.json");
        let store = FileThemeStore::at(path.clone());

        // When
        store.save(ThemeMode::Light).unwrap();
        let next_session = FileThemeStore::at(path.clone()).load();

        // Then
        assert_eq!(next_session, Some(ThemeMode::Light));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn given_no_preference_file_at_all_when_it_is_loaded_then_nothing_is_invented() {
        // Given — la première ouverture d'Ash sur une machine
        let store = FileThemeStore::at(std::env::temp_dir().join("ash-theme-absent/theme.json"));

        // When / Then
        assert_eq!(store.load(), None);
    }
}
