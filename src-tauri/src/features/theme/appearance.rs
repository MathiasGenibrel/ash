use super::font_size::FontSize;
use super::mode::ThemeMode;

/// Les préférences d'apparence de la fenêtre, ensemble.
///
/// Un seul enregistrement plutôt qu'une valeur par préférence, parce qu'il n'y a qu'un
/// fichier et qu'il s'écrit d'un bloc : deux états séparés qui écriraient chacun leur
/// moitié de `~/.ash/theme.json` finiraient par se marcher dessus, et la dernière
/// écriture effacerait l'autre.
///
/// C'est aussi ce que le fichier **contient** : sa forme est le contrat avec la version
/// d'Ash de demain. `font_size` est `#[serde(default)]` pour que les fichiers écrits avant
/// que la taille soit réglable se relisent sans rien perdre — un champ absent vaut la
/// taille par défaut, pas un fichier illisible. `mode`, lui, reste obligatoire : c'est ce
/// qui distingue un fichier de préférence d'un fichier qui ne dit rien qu'on comprenne, et
/// un fichier incompréhensible retombe sur les défauts sans en inventer la moitié.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Appearance {
    pub mode: ThemeMode,
    #[serde(default)]
    pub font_size: FontSize,
}
