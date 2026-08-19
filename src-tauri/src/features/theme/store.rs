use std::path::PathBuf;

use super::appearance::Appearance;
use super::error::ThemeError;

/// Où les préférences d'apparence se gardent d'une session à l'autre.
///
/// Un trait, comme tous les effets système de ce dépôt : sans lui, vérifier qu'un choix
/// survit au redémarrage demanderait d'écrire dans le `$HOME` de qui lance les tests.
pub trait ThemeStore: Send + Sync {
    /// Ce qui est gardé, ou `None` — première ouverture, fichier absent, fichier abîmé.
    fn load(&self) -> Option<Appearance>;
    fn save(&self, appearance: Appearance) -> Result<(), ThemeError>;
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
    /// `None`, et Ash repart sur le mode système et la taille par défaut. Une préférence
    /// d'apparence n'est jamais une raison d'empêcher une fenêtre d'ouvrir.
    fn load(&self) -> Option<Appearance> {
        decode(&std::fs::read_to_string(&self.path).ok()?)
    }

    fn save(&self, appearance: Appearance) -> Result<(), ThemeError> {
        let io = |why: std::io::Error| ThemeError::Io {
            path: self.path.clone(),
            why: why.to_string(),
        };

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&self.path, encode(appearance)).map_err(io)
    }
}

/// Le contenu du fichier, ou `None` s'il ne dit rien qu'on comprenne.
fn decode(content: &str) -> Option<Appearance> {
    serde_json::from_str::<Appearance>(content).ok()
}

fn encode(appearance: Appearance) -> String {
    // `to_string` et non `to_string_pretty` : quelques dizaines d'octets sur une ligne,
    // terminés par un saut de ligne pour que le fichier reste lisible dans un terminal.
    format!(
        "{}\n",
        serde_json::to_string(&appearance).unwrap_or_else(|_| String::from("{}"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::font_size::{FontSize, FontStep};
    use super::super::mode::ThemeMode;
    use super::super::sidebar_column::{SidebarColumn, SidebarWidth};

    #[test]
    fn given_a_stored_choice_when_it_is_read_back_then_it_is_the_same_choice() {
        // Given — le fichier est le seul lien entre deux sessions ; sa forme est un
        // contrat avec la version d'Ash de demain
        let written = encode(Appearance {
            mode: ThemeMode::Dark,
            font_size: FontSize::DEFAULT.stepped(FontStep::Bigger),
            sidebar: SidebarColumn {
                width: SidebarWidth::from(300),
                collapsed: false,
            },
        });

        // When
        let read = decode(&written);

        // Then
        assert_eq!(
            read,
            Some(Appearance {
                mode: ThemeMode::Dark,
                font_size: FontSize::DEFAULT.stepped(FontStep::Bigger),
                sidebar: SidebarColumn {
                    width: SidebarWidth::from(300),
                    collapsed: false,
                },
            })
        );
    }

    #[test]
    fn given_the_appearance_preferences_when_they_are_written_then_the_file_holds_exactly_them() {
        // Given — l'autre moitié de la garantie que porte
        // `features::sidebar::store::given_a_pinned_and_collapsed_state_when_it_is_written_then_the_file_holds_nothing_else`
        // : ce qui survit à la fermeture est réparti entre **deux** fichiers, et chacun doit
        // dire ce qu'il contient. La largeur de la colonne est une préférence d'apparence,
        // donc elle est ici — et pas dans `~/.ash/state.json`, qui ne garde que les épingles
        // et les lignes repliées.
        let chosen = Appearance {
            mode: ThemeMode::Dark,
            font_size: FontSize::DEFAULT,
            sidebar: SidebarColumn {
                width: SidebarWidth::from(320),
                collapsed: true,
            },
        };

        // When
        let written = encode(chosen);

        // Then
        let parsed: serde_json::Value =
            serde_json::from_str(&written).expect("le fichier écrit est du JSON");
        let object = parsed.as_object().expect("le fichier écrit est un objet");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["font_size", "mode", "sidebar"]);
        assert_eq!(object["sidebar"]["width"], serde_json::json!(320));
        assert_eq!(object["sidebar"]["collapsed"], serde_json::json!(true));
    }

    #[test]
    fn given_a_preference_file_written_before_the_font_size_existed_when_it_is_read_then_the_theme_survives(
    ) {
        // Given — le fichier des versions d'Ash où la taille n'était pas réglable
        let previous_version = "{\"mode\":\"dark\"}";

        // When
        let read = decode(previous_version);

        // Then — la mise à jour d'Ash ne perd pas le thème, et ouvre à la taille par défaut
        assert_eq!(
            read,
            Some(Appearance {
                mode: ThemeMode::Dark,
                font_size: FontSize::DEFAULT,
                sidebar: SidebarColumn::default(),
            })
        );
    }

    #[test]
    fn given_a_preference_file_written_by_a_later_ash_when_it_is_read_then_both_preferences_survive(
    ) {
        // Given — l'autre sens de la migration, celui qu'on ne peut pas jouer en revenant
        // en arrière : un fichier portant une troisième préférence d'apparence, ou lu par
        // une version d'Ash qui ne connaît pas encore `font_size`. Revenir d'une version à
        // la précédente n'a rien d'hypothétique — il suffit de rebasculer de branche.
        let later_version = "{\"mode\":\"dark\",\"font_size\":15,\"cursor\":\"bar\"}";

        // When
        let read = decode(later_version);

        // Then — un champ qu'on ne connaît pas se laisse tomber ; il ne rend pas le fichier
        // illisible et ne remet pas le thème sur le système. C'est ce que `deny_unknown_fields`
        // détruirait, et c'est pour ça qu'il n'est nulle part dans ce dépôt.
        assert_eq!(
            read,
            Some(Appearance {
                mode: ThemeMode::Dark,
                font_size: FontSize::DEFAULT
                    .stepped(FontStep::Bigger)
                    .stepped(FontStep::Bigger),
                sidebar: SidebarColumn::default(),
            })
        );
    }

    #[test]
    fn given_a_preference_file_that_says_nothing_understandable_when_it_is_read_then_ash_falls_back_to_the_system(
    ) {
        // Given — un fichier tronqué par une coupure, vidé, ou édité à la main
        let broken = ["", "{", "{\"mode\":\"solarized\"}", "null", "[]"];

        // When
        let read: Vec<Option<Appearance>> = broken.iter().map(|c| decode(c)).collect();

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
        let chosen = Appearance {
            mode: ThemeMode::Light,
            font_size: FontSize::DEFAULT.stepped(FontStep::Smaller),
            sidebar: SidebarColumn {
                width: SidebarWidth::from(260),
                collapsed: true,
            },
        };

        // When
        store.save(chosen).unwrap();
        let next_session = FileThemeStore::at(path.clone()).load();

        // Then — la taille suit le même chemin que le thème, et survit au redémarrage
        assert_eq!(next_session, Some(chosen));
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
