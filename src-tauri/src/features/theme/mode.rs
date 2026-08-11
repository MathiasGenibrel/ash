/// Le thème de la fenêtre, tel que l'utilisateur le choisit.
///
/// Trois modes, et le troisième n'est pas une palette : *système* est l'**absence** de
/// choix, donc celui de macOS. C'est la webview qui le résout, parce qu'elle est seule à
/// savoir de quelle humeur est le système et à l'apprendre quand il change — voir
/// `src/app/theme.ts`. Ici, on ne détient que le choix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    /// Le défaut : Ash suit macOS tant qu'on ne lui a rien demandé d'autre.
    #[default]
    System,
}

impl ThemeMode {
    /// Les trois modes, dans l'ordre du menu.
    pub const ALL: [ThemeMode; 3] = [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System];

    /// L'identifiant qui traverse la frontière — event, fichier de préférence, menu.
    pub fn as_id(self) -> &'static str {
        match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
            ThemeMode::System => "system",
        }
    }

    /// L'entrée de menu correspondante.
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
            ThemeMode::System => "System",
        }
    }

    /// Relit un identifiant. Un mode inconnu n'en est pas un : il n'y a rien à deviner.
    pub fn from_id(id: &str) -> Option<Self> {
        ThemeMode::ALL.into_iter().find(|mode| mode.as_id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_theme_mode_when_its_identifier_is_read_back_then_it_names_the_same_mode() {
        // Given — l'identifiant est le contrat avec `src/app/theme.ts`, avec le menu natif
        // et avec `~/.ash/theme.json` ; il traverse sous forme de chaîne et rien ne le
        // vérifie à la compilation
        let modes = ThemeMode::ALL;

        // When
        let round_trip: Vec<Option<ThemeMode>> = modes
            .iter()
            .map(|mode| ThemeMode::from_id(mode.as_id()))
            .collect();

        // Then
        assert_eq!(round_trip, modes.map(Some).to_vec());
    }

    #[test]
    fn given_an_identifier_no_mode_carries_when_it_is_read_then_nothing_is_guessed() {
        // Given / When — un fichier de préférence édité à la main, ou une version d'Ash
        // plus ancienne que celle qui l'a écrit
        let unknown = ThemeMode::from_id("solarized");

        // Then
        assert_eq!(unknown, None);
    }
}
