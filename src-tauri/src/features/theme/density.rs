/// La densité de la sidebar : combien de lignes tiennent dans la colonne (spec §9, `[ui]`).
///
/// Deux paliers, et pas un nombre de pixels : ce qui se règle est un **confort de lecture**,
/// pas une hauteur. Une hauteur réglable au pixel demanderait de reprendre l'aération de la
/// ligne de dépôt, de la ligne de worktree et de la ligne fille de sous-agent, qui ne sont
/// pas les mêmes — c'est un jeu de mesures cohérentes, et la feuille de style en tient deux.
///
/// **Les mesures elles-mêmes ne sont pas ici**, et c'est voulu : elles sont dans
/// `src/app/styles.css`, avec les deux palettes, parce qu'un pixel de retrait est du dessin.
/// Ce qui est détenu ici est le **choix**, comme le mode de thème
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum SidebarDensity {
    /// Le défaut : celui sur lequel Ash s'ouvre depuis toujours.
    #[default]
    Comfortable,
    /// Plus de worktrees à l'écran, sur une colonne qui en compte beaucoup.
    Compact,
}

impl SidebarDensity {
    pub const ALL: [SidebarDensity; 2] = [SidebarDensity::Comfortable, SidebarDensity::Compact];

    /// L'identifiant qui traverse la frontière — event, fichier de préférence, `data-density`.
    pub fn as_id(self) -> &'static str {
        match self {
            SidebarDensity::Comfortable => "comfortable",
            SidebarDensity::Compact => "compact",
        }
    }

    /// Relit un identifiant. Une densité inconnue n'en est pas une : il n'y a rien à deviner.
    pub fn from_id(id: &str) -> Option<Self> {
        SidebarDensity::ALL
            .into_iter()
            .find(|density| density.as_id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_density_when_its_identifier_is_read_back_then_it_names_the_same_density() {
        // Given — l'identifiant est le contrat avec `src/app/sidebar-density.ts`, avec le
        // `data-density` de la feuille de style et avec `~/.ash/theme.json` ; il traverse
        // sous forme de chaîne et rien ne le vérifie à la compilation
        let densities = SidebarDensity::ALL;

        // When
        let round_trip: Vec<Option<SidebarDensity>> = densities
            .iter()
            .map(|density| SidebarDensity::from_id(density.as_id()))
            .collect();

        // Then
        assert_eq!(round_trip, densities.map(Some).to_vec());
    }

    #[test]
    fn given_an_identifier_no_density_carries_when_it_is_read_then_nothing_is_guessed() {
        // Given / When — un fichier de préférence édité à la main, ou une version d'Ash plus
        // récente que celle qui le relit
        assert_eq!(SidebarDensity::from_id("cosy"), None);
    }
}
