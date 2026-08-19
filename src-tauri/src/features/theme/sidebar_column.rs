/// La colonne de gauche : sa largeur, et son repli.
///
/// **Les deux ensemble, et pas séparément** : `⌘B` et la poignée agissent sur le même objet
/// — replier ne perd pas la largeur, la rouvrir la restitue. Deux états séparés auraient
/// donné deux notions de « colonne repliée », et le jour où l'une des deux aurait manqué le
/// geste de l'autre, la colonne se serait rouverte à 240 px.
///
/// C'est aussi ce qui traverse la frontière : la webview reçoit l'objet entier à chaque
/// changement, et le rend. Elle n'en détient rien
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct SidebarColumn {
    #[serde(default)]
    pub width: SidebarWidth,
    /// `⌘B`, ou un glissement relâché sous le plancher. La largeur, elle, ne bouge pas :
    /// c'est ce qui fait qu'une colonne rouverte retrouve celle qu'elle avait.
    #[serde(default)]
    pub collapsed: bool,
}

/// La largeur de la colonne dépliée, en pixels.
///
/// **En pixels, et non en fraction de fenêtre**, parce que c'est ce que la personne a réglé :
/// une colonne posée à 300 px doit rester à 300 px quand on agrandit la fenêtre, comme dans
/// tout logiciel à colonnes. Les bornes, elles, sont bien relatives — de 10 % à 80 % de la
/// fenêtre — mais ce sont des bornes de **mise en page**, qui dépendent d'un viewport que
/// seule la webview connaît : elles vivent donc dans `src/features/sidebar/resize.ts`, avec
/// la règle qui les applique à chaque rendu.
///
/// Les deux bornes ci-dessous ne sont pas ces bornes-là. Elles ne servent qu'à ce qu'un
/// `~/.ash/theme.json` édité à la main — ou écrit par une version d'Ash à venir — ne rende
/// jamais un nombre absurde, et elles sont délibérément **plus larges** que tout ce que le
/// clamp relatif peut produire : si elles mordaient sur lui, la largeur montrée et la largeur
/// gardée se contrediraient jusqu'au redémarrage suivant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(transparent)]
pub struct SidebarWidth(u16);

impl SidebarWidth {
    /// Ce sur quoi Ash s'ouvre : les 240 px des écrans de design, ceux qui étaient écrits en
    /// dur dans `src/features/sidebar/sidebar.css` avant que la colonne soit réglable.
    pub const DEFAULT: SidebarWidth = SidebarWidth(240);

    /// Plus étroit que le rail replié (46 px) ne veut plus rien dire : à ce compte-là, la
    /// colonne est repliée, et c'est `collapsed` qui le dit.
    ///
    /// **Ce nombre ne tient hors du chemin du clamp relatif que parce que la fenêtre a une
    /// largeur minimale** : `app.windows[0].minWidth` vaut 640 px dans
    /// `src-tauri/tauri.conf.json`, donc le plancher de mise en page — 10 % de la fenêtre —
    /// ne descend jamais sous 64 px. Abaisser cette largeur minimale sous 460 px ferait
    /// mordre la borne d'ici sur le clamp de la webview, et la largeur montrée cesserait
    /// d'être la largeur gardée. Le test
    /// `given_the_narrowest_window_ash_allows_when_the_layout_hits_its_floor_then_this_type_leaves_it_alone`
    /// tient ce lien.
    const MIN: u16 = 46;

    /// Plus large que 80 % d'un écran de 10 000 px : hors d'atteinte du clamp relatif, donc
    /// hors de son chemin.
    const MAX: u16 = 8000;

    pub fn pixels(self) -> u16 {
        self.0
    }

    /// La largeur la plus proche de `pixels` qui reste une largeur.
    ///
    /// Le paramètre est un `i64` pour que le débordement soit **ramené** et non refusé : un
    /// `-3` écrit à la main donnerait sinon un fichier illisible, et emporterait le thème
    /// avec lui.
    fn clamped(pixels: i64) -> Self {
        SidebarWidth(pixels.clamp(i64::from(Self::MIN), i64::from(Self::MAX)) as u16)
    }
}

impl Default for SidebarWidth {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'de> serde::Deserialize<'de> for SidebarWidth {
    /// Relit une largeur, **toujours** dans les bornes — le seul chemin par lequel une valeur
    /// arbitraire entre dans le type, donc le seul endroit où la borner ait un sens. Même
    /// raisonnement que [`FontSize`](super::FontSize), et pour la même raison : ce qu'on
    /// relit est un fichier qu'Ash a écrit lui-même, personne n'est devant l'écran au
    /// démarrage pour lire un refus, et une largeur absurde n'ouvre rien.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::clamped(i64::deserialize(deserializer)?))
    }
}

impl From<i64> for SidebarWidth {
    /// La largeur que la webview annonce après un glissement.
    ///
    /// Elle arrive en `i64` et non en `u16` pour la raison qui vaut pour le fichier : une
    /// webview qui enverrait un nombre hors bornes — un glissement sur un écran qu'on n'a pas
    /// prévu, un appel bricolé — doit être **ramenée**, pas refusée par Tauri avec une erreur
    /// que personne ne lira.
    fn from(pixels: i64) -> Self {
        Self::clamped(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le plancher de mise en page, tel que `src/features/sidebar/resize.ts` le pose
    /// (`MIN_WIDTH_FRACTION`). Recopié ici parce qu'aucun des deux côtés ne peut lire l'autre
    /// — et c'est justement ce que le test ci-dessous surveille : le commentaire de
    /// `MIN_WIDTH_FRACTION` renvoie ici pour qu'on n'en change pas un sans l'autre.
    const LAYOUT_FLOOR_FRACTION: f64 = 0.10;

    /// La configuration de la fenêtre, lue à la compilation : le test parle de la vraie
    /// largeur minimale, pas d'un nombre recopié qui aurait vieilli.
    const TAURI_CONF: &str = include_str!("../../../tauri.conf.json");

    #[test]
    fn given_the_narrowest_window_ash_allows_when_the_layout_hits_its_floor_then_this_type_leaves_it_alone(
    ) {
        // Given — les deux clamps ne se contredisent que si celui d'ici mord sur celui de la
        // webview, et le seul chemin par lequel ça arriverait est une fenêtre autorisée à
        // devenir très étroite
        let conf: serde_json::Value =
            serde_json::from_str(TAURI_CONF).expect("tauri.conf.json est du JSON");
        let min_window = conf["app"]["windows"][0]["minWidth"]
            .as_f64()
            .expect("la fenêtre a une largeur minimale");

        // When — la colonne poussée à son plancher de mise en page dans cette fenêtre-là
        let floor = min_window * LAYOUT_FLOOR_FRACTION;

        // Then — la largeur montrée est celle qui est gardée, sans quoi elles se
        // contrediraient jusqu'au redémarrage suivant
        assert_eq!(SidebarWidth::from(floor as i64).pixels(), floor as u16);
    }

    #[test]
    fn given_a_preference_file_edited_by_hand_to_an_absurd_width_when_it_is_read_then_it_is_brought_back_into_range(
    ) {
        // Given — le fichier est lisible à l'œil nu, donc éditable de travers ; un `0` ne
        // doit pas ouvrir Ash sur une colonne qu'on ne peut plus attraper
        let absurd = ["0", "-40", "99999"];

        // When
        let read: Vec<Option<SidebarWidth>> = absurd
            .iter()
            .map(|raw| serde_json::from_str::<SidebarWidth>(raw).ok())
            .collect();

        // Then
        assert_eq!(
            read,
            vec![
                Some(SidebarWidth(SidebarWidth::MIN)),
                Some(SidebarWidth(SidebarWidth::MIN)),
                Some(SidebarWidth(SidebarWidth::MAX)),
            ]
        );
    }

    #[test]
    fn given_a_width_the_layout_can_really_produce_when_it_is_announced_then_it_is_kept_as_is() {
        // Given — les bornes d'ici ne doivent jamais mordre sur celles de la mise en page,
        // sinon la largeur montrée et la largeur gardée se contrediraient
        let dragged = [46_i64, 240, 1600, 3000];

        // When
        let kept: Vec<u16> = dragged
            .iter()
            .map(|pixels| SidebarWidth::from(*pixels).pixels())
            .collect();

        // Then
        assert_eq!(kept, vec![46, 240, 1600, 3000]);
    }
}
