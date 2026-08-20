/// Le panneau bas : sa hauteur, s'il est ouvert, et la vue qu'il montre (spec §4.3).
///
/// **Les trois ensemble**, pour la raison qui a réuni la largeur et le repli de la colonne
/// de gauche : ce sont les faces d'un même geste. Cliquer `worktrees` ouvre le panneau *et*
/// change sa vue ; recliquer `worktrees` le referme *sans* toucher à sa hauteur, qui repart
/// telle quelle au prochain clic. Trois états séparés auraient donné trois notions de
/// « panneau ouvert », et la première désynchronisation aurait rouvert un panneau vide à sa
/// hauteur par défaut.
///
/// C'est aussi ce qui traverse la frontière : la webview reçoit l'objet entier à chaque
/// changement, et le rend. Elle n'en détient rien
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — et surtout pas
/// « quelle vue est ouverte », qui décide de ce que le terminal a comme place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct BottomPanel {
    #[serde(default)]
    pub height: PanelHeight,
    /// Ouvert, le panneau prend sa hauteur au terminal ; refermé, il la lui **rend**
    /// (ADR-0003, reformulation du 2026-08-10). La hauteur, elle, ne bouge pas.
    #[serde(default)]
    pub open: bool,
    /// La vue montrée quand il est ouvert — et celle qu'il rouvrira quand il est fermé.
    #[serde(default)]
    pub view: PanelView,
}

/// Les vues que le panneau accueille (spec §4.3, ADR-0003).
///
/// Elles sont nommées ici, dans l'état, et pas seulement dans la webview : c'est le backend
/// qui décide laquelle est ouverte, donc c'est lui qui doit refuser un nom qu'il ne connaît
/// pas. Leur **contenu** est ailleurs et n'existe pas encore — le graphe (#27), le tableau
/// des worktrees (#28), les conflits (#30) et la fiche de branche (#31). Ce type ne sait
/// rien de git, et n'a pas à en savoir : il nomme des surfaces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum PanelView {
    /// Le graphe de commits (`⌘⌃G`) — la vue sur laquelle le panneau s'ouvre par défaut.
    #[default]
    Graph,
    /// Le tableau des worktrees (`⌘⌃W`).
    Worktrees,
    /// Les conflits d'un rebase ou d'un merge arrêté. **La seule vue sans raccourci** :
    /// `⌘⌃M` ouvre l'onglet de merge (spec §4.4), et cette vue-ci en est la porte à la
    /// souris — elle s'atteint par la barre du panneau.
    Conflicts,
    /// La fiche de branche (`⌘⌃I`).
    Branch,
}

/// La hauteur du panneau ouvert, en pixels.
///
/// **En pixels, et non en fraction**, pour la raison qui vaut déjà pour la colonne de gauche
/// ([`SidebarWidth`](super::SidebarWidth)) : c'est ce que la personne a réglé, et un panneau
/// posé à 220 px doit rester à 220 px quand la fenêtre grandit. Les bornes relatives — de
/// 15 % à 70 % de la **zone terminal** — dépendent d'une mise en page que seule la webview
/// connaît, et vivent dans `src/features/panel/layout.ts`.
///
/// Les deux bornes ci-dessous ne sont pas ces bornes-là : elles ne servent qu'à ce qu'un
/// `~/.ash/theme.json` édité à la main ne rende jamais un nombre absurde, et elles sont
/// délibérément **plus larges** que tout ce que le clamp relatif peut produire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(transparent)]
pub struct PanelHeight(u16);

impl PanelHeight {
    /// Ce sur quoi le panneau s'ouvre la première fois : de quoi montrer une dizaine de
    /// lignes de graphe sans manger la moitié du terminal.
    pub const DEFAULT: PanelHeight = PanelHeight(220);

    /// Sous 40 px, il ne reste plus un panneau mais un bandeau — et c'est `open` qui dit
    /// qu'il n'y en a pas.
    ///
    /// **Ce nombre ne tient hors du chemin du clamp relatif que parce que la fenêtre a une
    /// hauteur minimale** : `app.windows[0].minHeight` vaut 400 px dans
    /// `src-tauri/tauri.conf.json`, et le chrome qui entoure la zone terminal — bande de
    /// titre, barre d'onglets du panneau, ligne de statut — tient sous 120 px. Le plancher
    /// de mise en page, 15 % de ce qui reste, ne descend donc jamais sous 42 px. Le test
    /// `given_the_shortest_window_ash_allows_when_the_layout_hits_its_floor_then_this_type_leaves_it_alone`
    /// tient ce lien.
    const MIN: u16 = 40;

    /// Plus haut que 70 % d'un écran de 8 000 px : hors d'atteinte du clamp relatif, donc
    /// hors de son chemin.
    const MAX: u16 = 6000;

    pub fn pixels(self) -> u16 {
        self.0
    }

    /// La hauteur la plus proche de `pixels` qui reste une hauteur.
    ///
    /// Le paramètre est un `i64` pour que le débordement soit **ramené** et non refusé : un
    /// `-3` écrit à la main donnerait sinon un fichier illisible, et emporterait le thème
    /// avec lui.
    fn clamped(pixels: i64) -> Self {
        PanelHeight(pixels.clamp(i64::from(Self::MIN), i64::from(Self::MAX)) as u16)
    }
}

impl Default for PanelHeight {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'de> serde::Deserialize<'de> for PanelHeight {
    /// Relit une hauteur, **toujours** dans les bornes — le seul chemin par lequel une
    /// valeur arbitraire entre dans le type. Même raisonnement que
    /// [`SidebarWidth`](super::SidebarWidth).
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::clamped(i64::deserialize(deserializer)?))
    }
}

impl From<i64> for PanelHeight {
    /// La hauteur que la webview annonce après un glissement, ramenée dans les bornes
    /// plutôt que refusée par une erreur que personne ne lira.
    fn from(pixels: i64) -> Self {
        Self::clamped(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le plancher de mise en page, tel que `src/features/panel/layout.ts` le pose
    /// (`MIN_HEIGHT_FRACTION`). Recopié ici parce qu'aucun des deux côtés ne peut lire
    /// l'autre — et c'est justement ce que le test ci-dessous surveille.
    const LAYOUT_FLOOR_FRACTION: f64 = 0.15;

    /// Ce que le chrome prend à la fenêtre au-dessus et au-dessous de la zone terminal :
    /// bande de titre (38 px), barre d'onglets du panneau (26 px), ligne de statut (25 px),
    /// et de la marge pour ce qui s'y ajouterait. Un budget volontairement large : plus il
    /// l'est, plus le plancher qu'il laisse est bas, donc plus le test est sévère.
    const CHROME_BUDGET: f64 = 120.0;

    /// La configuration de la fenêtre, lue à la compilation : le test parle de la vraie
    /// hauteur minimale, pas d'un nombre recopié qui aurait vieilli.
    const TAURI_CONF: &str = include_str!("../../../tauri.conf.json");

    #[test]
    fn given_the_shortest_window_ash_allows_when_the_layout_hits_its_floor_then_this_type_leaves_it_alone(
    ) {
        // Given — les deux clamps ne se contredisent que si celui d'ici mord sur celui de la
        // webview, et le seul chemin par lequel ça arriverait est une fenêtre autorisée à
        // devenir très courte
        let conf: serde_json::Value =
            serde_json::from_str(TAURI_CONF).expect("tauri.conf.json est du JSON");
        let min_window = conf["app"]["windows"][0]["minHeight"]
            .as_f64()
            .expect("la fenêtre a une hauteur minimale");

        // When — le panneau poussé à son plancher de mise en page dans cette fenêtre-là
        let floor = (min_window - CHROME_BUDGET) * LAYOUT_FLOOR_FRACTION;

        // Then — la hauteur montrée est celle qui est gardée, sans quoi elles se
        // contrediraient jusqu'au redémarrage suivant
        assert_eq!(PanelHeight::from(floor as i64).pixels(), floor as u16);
    }

    #[test]
    fn given_a_preference_file_edited_by_hand_to_an_absurd_height_when_it_is_read_then_it_is_brought_back_into_range(
    ) {
        // Given — le fichier est lisible à l'œil nu, donc éditable de travers ; un `0` ne
        // doit pas ouvrir un panneau qu'on ne peut plus attraper
        let absurd = ["0", "-40", "99999"];

        // When
        let read: Vec<Option<PanelHeight>> = absurd
            .iter()
            .map(|raw| serde_json::from_str::<PanelHeight>(raw).ok())
            .collect();

        // Then
        assert_eq!(
            read,
            vec![
                Some(PanelHeight(PanelHeight::MIN)),
                Some(PanelHeight(PanelHeight::MIN)),
                Some(PanelHeight(PanelHeight::MAX)),
            ]
        );
    }

    #[test]
    fn given_a_panel_never_opened_when_ash_reads_a_file_that_predates_it_then_it_stays_closed() {
        // Given — un `theme.json` écrit avant que le panneau existe : le champ manque
        let older = r#"{ "mode": "dark" }"#;

        // When
        let appearance: super::super::appearance::Appearance =
            serde_json::from_str(older).expect("un fichier plus ancien reste lisible");
        let panel = appearance.panel;

        // Then — un panneau qui s'ouvrirait tout seul à la mise à jour prendrait sa hauteur
        // au terminal sans que personne ne l'ait demandé
        assert!(!panel.open);
        assert_eq!(panel.height, PanelHeight::DEFAULT);
        assert_eq!(panel.view, PanelView::Graph);
    }
}
