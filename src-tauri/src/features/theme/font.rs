use std::path::PathBuf;
use std::sync::OnceLock;

use super::sfnt;

/// La police du terminal, telle que l'utilisateur la choisit (spec §9, `[appearance]`).
///
/// Une **famille**, jamais un fichier ni une face : c'est ce que xterm.js met dans son
/// `fontFamily`, et c'est ce qu'un utilisateur reconnaît. Le type existe pour la même raison
/// que [`FontSize`](super::FontSize) : il est le seul chemin par lequel un nom entre, donc le
/// seul endroit où il puisse être borné. Un nom vide ferait un terminal sans police, et un nom
/// de trois kilo-octets écrit à la main dans `~/.ash/theme.json` traverserait la frontière
/// jusque dans une déclaration CSS.
///
/// **Ce type ne vérifie pas qu'une police est installée**, et c'est délibéré : le catalogue
/// est un effet système ([`FontCatalog`]), il change entre deux démarrages, et une préférence
/// qui se refuserait parce qu'une police a été désinstallée laisserait Ash sans police du
/// tout. Une famille absente retombe sur la face de repli du navigateur, comme partout
/// ailleurs sur le web.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(transparent)]
pub struct TerminalFont(String);

impl TerminalFont {
    /// Celle qu'Ash embarque, et sur laquelle il s'ouvre — voir `src/assets/fonts/`.
    ///
    /// C'est aussi la seule famille que le catalogue peut promettre : elle voyage dans le
    /// bundle, donc elle est disponible même sur une machine où rien n'est installé.
    pub const DEFAULT_FAMILY: &'static str = "JetBrains Mono";

    /// Le plafond : un nom de famille dépasse rarement la trentaine de caractères, et ce qui
    /// entre ici finit dans une déclaration CSS.
    const MAX_LEN: usize = 64;

    /// Un nom de famille, ou `None` s'il n'en est pas un.
    ///
    /// Les caractères de contrôle sont refusés plutôt que nettoyés : ils ne peuvent venir que
    /// d'un fichier abîmé ou d'un appel fabriqué, et une famille « réparée » en silence
    /// désignerait une police que personne n'a choisie.
    pub fn new(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        if trimmed.is_empty()
            || trimmed.chars().count() > Self::MAX_LEN
            || trimmed.chars().any(char::is_control)
        {
            return None;
        }
        Some(TerminalFont(trimmed.to_owned()))
    }

    pub fn family(&self) -> &str {
        &self.0
    }
}

impl Default for TerminalFont {
    fn default() -> Self {
        TerminalFont(Self::DEFAULT_FAMILY.to_owned())
    }
}

impl<'de> serde::Deserialize<'de> for TerminalFont {
    /// Relit une famille du fichier de préférence, **toujours** utilisable.
    ///
    /// Même tolérance que [`FontSize`](super::FontSize) : une valeur inutilisable retombe sur
    /// la famille par défaut au lieu de rendre tout le fichier illisible — et d'emporter le
    /// thème avec elle.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(TerminalFont::new(&raw).unwrap_or_default())
    }
}

/// Ce que la feature attend du système : les familles à largeur fixe qu'on peut choisir.
///
/// Un trait comme tous les effets système du dépôt. Sans lui, la section `appearance` ne
/// pourrait se vérifier que sur la machine qui lance les tests — c'est-à-dire pas du tout :
/// la liste dépend de ce que quelqu'un a installé un jour.
pub trait FontCatalog: Send + Sync {
    /// Les familles monospace proposées, triées et sans doublon.
    ///
    /// **Monospace, et pas « toutes »** : un terminal dont les cellules n'ont pas la même
    /// largeur n'aligne plus rien, et proposer les quatre cents faces d'un macOS neuf pour
    /// que sept d'entre elles conviennent est une liste dans laquelle on ne choisit pas.
    fn monospace_families(&self) -> Vec<String>;
}

/// Les polices réellement installées, lues dans les dossiers de polices de macOS.
///
/// **Aucune dépendance, et aucun `unsafe`.** La voie évidente serait AppKit
/// (`NSFontManager`), mais elle demanderait de nommer une caisse de plus et d'ouvrir un
/// troisième module `unsafe` dans le crate pour peupler un menu déroulant. Le format SFNT
/// porte lui-même la réponse : sa table `post` a un drapeau `isFixedPitch`, et sa table
/// `name` porte le nom de famille. C'est de la **lecture de fichiers**, donc du code Rust
/// ordinaire, et le parsing se teste sans qu'aucune police soit installée (voir [`sfnt`]).
///
/// Ce qui est lu de chaque fichier tient en trois morceaux — l'en-tête, la table `post`
/// (32 octets) et la table `name` (quelques kilo-octets) : les fichiers de polices du
/// système pèsent plusieurs dizaines de méga-octets, et les lire entiers pour un drapeau
/// serait payer un catalogue au prix d'un scan de disque.
pub struct SystemFontCatalog {
    directories: Vec<PathBuf>,
    /// Le catalogue ne change pas pendant qu'Ash tourne — installer une police demande de
    /// passer par le Livre des polices. Le relire à chaque ouverture des réglages serait
    /// quelques centaines de fichiers ouverts pour la même réponse.
    cached: OnceLock<Vec<String>>,
}

impl SystemFontCatalog {
    /// Les trois emplacements de macOS, du système à l'utilisateur.
    pub fn on_this_mac() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut directories = vec![
            PathBuf::from("/System/Library/Fonts"),
            PathBuf::from("/Library/Fonts"),
        ];
        directories.extend(home.map(|home| home.join("Library").join("Fonts")));
        Self::in_directories(directories)
    }

    pub fn in_directories(directories: Vec<PathBuf>) -> Self {
        Self {
            directories,
            cached: OnceLock::new(),
        }
    }

    /// Les fichiers de police d'un dossier, et de ses sous-dossiers immédiats.
    ///
    /// Un seul cran de profondeur : `/System/Library/Fonts/Supplemental` existe, et rien de
    /// plus profond n'est un emplacement de polices sur macOS. Une descente sans fond ferait
    /// d'un dossier mal placé un parcours de disque.
    fn font_files(directory: &PathBuf) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Vec::new();
        };
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(
                    std::fs::read_dir(&path)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|nested| nested.path())
                        .filter(|nested| is_font_file(nested)),
                );
            } else if is_font_file(&path) {
                files.push(path);
            }
        }
        files
    }
}

fn is_font_file(path: &std::path::Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "ttf" | "ttc" | "otf" | "otc"
    )
}

/// macOS cache ses polices internes derrière un point — `.SF NS Mono`, `.LastResort`.
///
/// Le Livre des polices ne les montre pas, et le système ne garantit ni leur nom ni leur
/// présence d'une version à l'autre : les proposer donnerait un choix qui a l'air d'être
/// « SF Mono » sans en être, et qui peut disparaître à la prochaine mise à jour.
fn is_offered(family: &str) -> bool {
    !family.starts_with('.')
}

impl FontCatalog for SystemFontCatalog {
    fn monospace_families(&self) -> Vec<String> {
        self.cached
            .get_or_init(|| {
                // La famille embarquée est **toujours** proposée : elle voyage dans le
                // bundle d'Ash, donc la webview sait la peindre même si personne ne l'a
                // installée sur cette machine. Sans elle, la liste pourrait ne pas contenir
                // le choix par défaut, et la section montrerait une police qu'elle ne
                // propose pas.
                let mut families = vec![TerminalFont::DEFAULT_FAMILY.to_owned()];
                for directory in &self.directories {
                    for file in Self::font_files(directory) {
                        let Ok(mut handle) = std::fs::File::open(&file) else {
                            continue;
                        };
                        families.extend(
                            sfnt::monospace_family(&mut handle).filter(|family| is_offered(family)),
                        );
                    }
                }
                families.sort_by_key(|family| family.to_lowercase());
                families.dedup();
                families
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_family_name_with_room_around_it_when_it_is_taken_in_then_it_is_the_name_alone() {
        // Given — le nom arrive d'un menu déroulant, mais aussi d'un `theme.json` édité à la
        // main : c'est la seule porte d'entrée du type, donc la seule où le borner
        let taken = TerminalFont::new("  SF Mono  ");

        // Then
        assert_eq!(
            taken.map(|font| font.family().to_owned()),
            Some("SF Mono".to_owned())
        );
    }

    #[test]
    fn given_a_name_that_would_leave_the_terminal_without_a_font_when_it_is_taken_in_then_it_is_refused(
    ) {
        // Given — le vide, l'espace seul, un nom démesuré et un nom qui porte une nouvelle
        // ligne : les deux derniers finiraient tels quels dans une déclaration CSS
        let refused = ["", "   ", &"a".repeat(65), "Menlo\nregular"];

        // When
        let taken: Vec<Option<TerminalFont>> =
            refused.iter().map(|name| TerminalFont::new(name)).collect();

        // Then
        assert_eq!(taken, vec![None; refused.len()]);
    }

    #[test]
    fn given_a_preference_file_naming_a_font_that_cannot_be_used_when_it_is_read_then_ash_falls_back_instead_of_losing_the_file(
    ) {
        // Given — la même tolérance que la taille de police : un champ abîmé ne doit pas
        // emporter le thème, qui est écrit dans le même fichier
        let broken = "\"\"";

        // When
        let read: TerminalFont = serde_json::from_str(broken).unwrap();

        // Then
        assert_eq!(read, TerminalFont::default());
        assert_eq!(read.family(), TerminalFont::DEFAULT_FAMILY);
    }

    #[test]
    fn given_a_folder_of_fonts_where_only_some_are_fixed_pitch_when_the_catalog_is_read_then_it_offers_those_and_the_bundled_one(
    ) {
        // Given — un faux dossier de polices : deux fichiers à largeur fixe, un
        // proportionnel, et un fichier qui n'est pas une police du tout
        let directory = std::env::temp_dir().join(format!("ash-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("iosevka.ttf"),
            sfnt::tests::FontFileBuilder::new("Iosevka")
                .fixed_pitch(true)
                .build(),
        )
        .unwrap();
        std::fs::write(
            directory.join("menlo.ttc"),
            sfnt::tests::FontFileBuilder::new("Menlo")
                .fixed_pitch(true)
                .collection()
                .build(),
        )
        .unwrap();
        std::fs::write(
            directory.join("helvetica.ttf"),
            sfnt::tests::FontFileBuilder::new("Helvetica")
                .fixed_pitch(false)
                .build(),
        )
        .unwrap();
        std::fs::write(directory.join("readme.txt"), b"ceci n'est pas une police").unwrap();

        // When
        let catalog = SystemFontCatalog::in_directories(vec![directory.clone()]);
        let families = catalog.monospace_families();

        // Then — la famille embarquée est toujours là, la proportionnelle jamais
        assert_eq!(
            families,
            vec![
                "Iosevka".to_owned(),
                TerminalFont::DEFAULT_FAMILY.to_owned(),
                "Menlo".to_owned()
            ]
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn given_a_font_macos_keeps_for_itself_when_the_catalog_is_read_then_it_is_not_offered() {
        // Given — `/System/Library/Fonts/SFNSMono.ttf` se nomme `.SF NS Mono` : le Livre des
        // polices le cache, et le système ne promet ni son nom ni sa présence d'une version à
        // l'autre. Proposé, il ressemblerait à « SF Mono » sans en être
        let directory = std::env::temp_dir().join(format!("ash-private-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SFNSMono.ttf"),
            sfnt::tests::FontFileBuilder::new(".SF NS Mono").build(),
        )
        .unwrap();

        // When
        let families =
            SystemFontCatalog::in_directories(vec![directory.clone()]).monospace_families();

        // Then
        assert_eq!(families, vec![TerminalFont::DEFAULT_FAMILY.to_owned()]);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn given_no_font_folder_at_all_when_the_catalog_is_read_then_the_bundled_family_is_still_offered(
    ) {
        // Given — un `HOME` sans `Library/Fonts`, ou un sandbox qui refuse la lecture : une
        // liste vide ferait un menu déroulant qui ne propose même pas ce qui est en vigueur
        let catalog = SystemFontCatalog::in_directories(vec![PathBuf::from("/nowhere/at/all")]);

        // When / Then
        assert_eq!(
            catalog.monospace_families(),
            vec![TerminalFont::DEFAULT_FAMILY.to_owned()]
        );
    }
}
