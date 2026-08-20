use super::bottom_panel::BottomPanel;
use super::density::SidebarDensity;
use super::font::TerminalFont;
use super::font_size::FontSize;
use super::mode::ThemeMode;
use super::sidebar_column::SidebarColumn;

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
///
/// **Le contrat vaut dans les deux sens**, et le second se rate facilement : un champ
/// inconnu se laisse tomber, donc un fichier écrit ici se relit par une version d'Ash qui
/// ignore `font_size` — revenir d'une branche à la précédente ne coûte pas le thème. C'est
/// exactement ce qu'un `#[serde(deny_unknown_fields)]` détruirait, en croyant bien faire :
/// il n'y en a nulle part dans ce dépôt, et les deux sens sont tenus par un test dans
/// `store.rs`.
///
/// `Copy` a disparu de la liste des dérivations le jour où la **police** s'y est ajoutée :
/// une famille est un nom, donc une `String`. Rien d'autre n'a changé — l'enregistrement
/// continue de voyager d'un bloc, et `state.rs` le clone là où il le copiait.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Appearance {
    pub mode: ThemeMode,
    #[serde(default)]
    pub font_size: FontSize,
    /// La famille du terminal (spec §9). `#[serde(default)]` pour la même raison que la
    /// taille : un fichier écrit avant qu'elle soit réglable se relit sans rien perdre.
    #[serde(default)]
    pub font: TerminalFont,
    /// La densité de la sidebar (spec §9), au même titre — c'est une préférence d'apparence,
    /// écrite dans le même fichier, relue au même moment.
    #[serde(default)]
    pub density: SidebarDensity,
    /// La largeur de la colonne de gauche et son repli (`⌘B`).
    ///
    /// **Ici, et surtout pas dans `~/.ash/state.json`** : ce fichier-là ne garde que les
    /// worktrees épinglés et les lignes repliées, et c'est ce qui porte la règle « rien
    /// d'autre ne survit à la fermeture » de la spec §3.1 — voir le test de
    /// `features::sidebar::store`. Une colonne large est une préférence d'**apparence**
    /// (spec §9), de la même nature que le thème et la taille de police : elle a donc la
    /// même adresse, et le même `#[serde(default)]` pour que les fichiers écrits avant
    /// qu'elle existe se relisent sans rien perdre.
    #[serde(default)]
    pub sidebar: SidebarColumn,
    /// Le panneau bas : sa hauteur, son ouverture, et la vue qu'il montre (spec §4.3).
    ///
    /// Sixième préférence du même fichier, et pour les mêmes raisons que la colonne de
    /// gauche : elle survit à la fermeture, elle décide de la place qu'a le terminal, et
    /// elle n'est pas de l'état de session — voir [`BottomPanel`]. `#[serde(default)]` pour
    /// que les fichiers écrits avant que le panneau existe se relisent sans rien perdre, et
    /// **sans ouvrir un panneau que personne n'a demandé**.
    #[serde(default)]
    pub panel: BottomPanel,
}
