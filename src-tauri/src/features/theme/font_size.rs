/// La taille de police du terminal, en points.
///
/// **C'est un réglage de l'application, pas d'onglet**, et c'est la décision de cette
/// feature — pas un défaut subi. [ADR-0003](../../../../docs/adr/0003-zone-terminal-unique.md)
/// dit qu'un onglet porte au plus un PTY ; elle ne dit pas qu'il porte une apparence. Un
/// émulateur de terminal traite la taille comme une préférence de confort de lecture, qui
/// suit l'utilisateur d'un onglet à l'autre : la régler par onglet obligerait à la régler
/// à chaque `Cmd+N`, et donnerait à la fenêtre de réglages une valeur qu'elle ne saurait
/// pas nommer — « la taille » n'existerait plus. Elle vit donc ici, à côté du mode de
/// thème, avec les autres préférences d'apparence de la fenêtre.
///
/// **Bornée, et par construction.** Les deux bornes ne sont pas un garde-fou d'interface
/// mais une propriété du type : rien ne peut fabriquer une taille hors de `MIN..=MAX`, pas
/// même un `~/.ash/theme.json` édité à la main — la relecture ramène dans l'intervalle
/// plutôt que de refuser le fichier. Un terminal à 2 points ne se répare pas au clavier,
/// puisqu'on ne voit plus ce qu'on tape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(transparent)]
pub struct FontSize(u8);

impl FontSize {
    /// Ce sur quoi Ash s'ouvre, et ce que `Cmd+0` ramène. C'est la valeur qui était écrite
    /// en dur dans `src/features/terminal/xterm-view.ts` avant que la taille soit réglable.
    pub const DEFAULT: FontSize = FontSize(13);

    /// Le plancher : plus bas, JetBrains Mono n'a plus assez de pixels pour distinguer un
    /// `l` d'un `1`, et l'utilisateur ne pourrait plus lire l'entrée de menu qui répare.
    const MIN: u8 = 8;

    /// Le plafond : au-delà, une fenêtre de largeur courante ne tient plus les 80 colonnes
    /// que la moitié des programmes en ligne de commande supposent.
    const MAX: u8 = 32;

    /// Un point à la fois : c'est ce que fait un émulateur de terminal, et ça laisse
    /// atteindre exactement la taille voulue au lieu de sauter par-dessus.
    const STEP: u8 = 1;

    pub fn points(self) -> u8 {
        self.0
    }

    /// La taille suivante, dans le sens demandé. Aux bornes, elle ne bouge plus.
    pub fn stepped(self, step: FontStep) -> Self {
        match step {
            FontStep::Bigger => Self::clamped(i64::from(self.0) + i64::from(Self::STEP)),
            FontStep::Smaller => Self::clamped(i64::from(self.0) - i64::from(Self::STEP)),
            FontStep::Default => Self::DEFAULT,
        }
    }

    /// La taille la plus proche de `points` qui reste lisible.
    ///
    /// Le paramètre est un `i64` et non un `u8` pour que le débordement soit **ramené** et
    /// non refusé : un `400` écrit à la main dans le fichier de préférence donne le plafond,
    /// là où un `u8` aurait fait échouer la lecture de tout le fichier — et emporté le
    /// choix de thème avec lui.
    fn clamped(points: i64) -> Self {
        FontSize(points.clamp(i64::from(Self::MIN), i64::from(Self::MAX)) as u8)
    }
}

impl Default for FontSize {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'de> serde::Deserialize<'de> for FontSize {
    /// Relit une taille du fichier de préférence, **toujours** dans les bornes.
    ///
    /// C'est le seul chemin par lequel une valeur arbitraire entre dans le type, donc le
    /// seul endroit où la borner ait un sens.
    ///
    /// **C'est l'inverse de ce que fait [`Command::parse`](crate::features::settings::Command),
    /// et la différence est voulue.** `Command` n'a délibérément pas de `Deserialize` : c'est
    /// une saisie de l'utilisateur *et* une frontière de sécurité — un chemin déguisé en nom
    /// de commande ferait exécuter un fichier désigné à la main — donc rien ne doit pouvoir
    /// en fabriquer un sans passer par la règle, et une saisie fautive se **refuse**, pour
    /// qu'on puisse la corriger.
    ///
    /// Une taille de police n'est ni l'un ni l'autre. Ce qu'on relit est un fichier
    /// qu'**Ash a écrit lui-même**, il n'y a personne devant l'écran pour lire un refus au
    /// démarrage, et une valeur hors bornes n'ouvre rien : le pire qu'un `400` puisse faire
    /// est de mal afficher. Refuser coûterait le choix de thème, écrit dans le même objet ;
    /// ramener dans l'intervalle ne coûte rien, et se voit — le terminal s'ouvre au plafond
    /// au lieu de la taille demandée. Le fichier, lui, n'est pas réécrit : la correction se
    /// rejoue à chaque démarrage tant que l'utilisateur n'a pas retouché sa préférence.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::clamped(i64::deserialize(deserializer)?))
    }
}

/// Ce qu'une entrée de menu demande à la taille de police.
///
/// Un pas, et non une taille : le menu ne connaît ni les bornes ni la valeur courante —
/// c'est [`FontSize`] qui les tient, et lui seul.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStep {
    /// `Cmd++`
    Bigger,
    /// `Cmd+-`
    Smaller,
    /// `Cmd+0` — revenir à [`FontSize::DEFAULT`].
    Default,
}

impl FontStep {
    /// Les trois pas, dans l'ordre du menu.
    pub const ALL: [FontStep; 3] = [FontStep::Bigger, FontStep::Smaller, FontStep::Default];

    /// L'identifiant d'entrée de menu, côté `menu.rs`.
    pub fn as_id(self) -> &'static str {
        match self {
            FontStep::Bigger => "bigger",
            FontStep::Smaller => "smaller",
            FontStep::Default => "default",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        FontStep::ALL.into_iter().find(|step| step.as_id() == id)
    }

    /// L'entrée de menu correspondante — les noms que macOS emploie dans « View ».
    pub fn label(self) -> &'static str {
        match self {
            FontStep::Bigger => "Bigger",
            FontStep::Smaller => "Smaller",
            FontStep::Default => "Actual Size",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_the_smallest_readable_size_when_the_user_keeps_shrinking_then_the_terminal_stays_readable(
    ) {
        // Given — un `Cmd+-` maintenu enfoncé, ce qui est exactement comme on découvre
        // qu'un réglage n'a pas de plancher
        let mut size = FontSize::DEFAULT;

        // When
        for _ in 0..50 {
            size = size.stepped(FontStep::Smaller);
        }

        // Then — on doit encore pouvoir lire ce qu'on tape pour revenir en arrière
        assert_eq!(size.points(), FontSize::MIN);
    }

    #[test]
    fn given_the_largest_size_when_the_user_keeps_growing_then_it_stops_at_the_ceiling() {
        // Given
        let mut size = FontSize::DEFAULT;

        // When
        for _ in 0..50 {
            size = size.stepped(FontStep::Bigger);
        }

        // Then — au-delà, une fenêtre courante ne tiendrait plus 80 colonnes
        assert_eq!(size.points(), FontSize::MAX);
    }

    #[test]
    fn given_a_size_the_user_has_changed_when_the_default_step_is_asked_then_it_is_thirteen_again()
    {
        // Given
        let grown = FontSize::DEFAULT
            .stepped(FontStep::Bigger)
            .stepped(FontStep::Bigger);

        // When — `Cmd+0`
        let reset = grown.stepped(FontStep::Default);

        // Then
        assert_eq!(reset.points(), 13);
    }

    #[test]
    fn given_one_step_up_then_one_step_down_when_they_are_played_then_the_size_comes_back() {
        // Given / When — un aller-retour doit être neutre, sinon `Cmd++` puis `Cmd+-`
        // laisserait dériver la taille
        let size = FontSize::DEFAULT
            .stepped(FontStep::Bigger)
            .stepped(FontStep::Smaller);

        // Then
        assert_eq!(size, FontSize::DEFAULT);
    }

    #[test]
    fn given_a_preference_file_edited_by_hand_to_an_absurd_size_when_it_is_read_then_it_is_brought_back_into_range(
    ) {
        // Given — le fichier est éditable à l'œil nu, donc éditable de travers ; un `0` ou
        // un `400` ne doit pas ouvrir Ash sur un terminal qu'on ne peut plus lire
        let absurd = ["0", "-12", "400", "100000"];

        // When
        let read: Vec<Option<FontSize>> = absurd
            .iter()
            .map(|raw| serde_json::from_str::<FontSize>(raw).ok())
            .collect();

        // Then
        assert_eq!(
            read,
            vec![
                Some(FontSize(FontSize::MIN)),
                Some(FontSize(FontSize::MIN)),
                Some(FontSize(FontSize::MAX)),
                Some(FontSize(FontSize::MAX)),
            ]
        );
    }

    #[test]
    fn given_a_font_step_identifier_when_it_is_read_back_then_it_names_the_same_step() {
        // Given — l'identifiant traverse le menu natif sous forme de chaîne, et rien ne le
        // vérifie à la compilation
        let steps = FontStep::ALL;

        // When
        let round_trip: Vec<Option<FontStep>> = steps
            .iter()
            .map(|step| FontStep::from_id(step.as_id()))
            .collect();

        // Then
        assert_eq!(round_trip, steps.map(Some).to_vec());
    }
}
