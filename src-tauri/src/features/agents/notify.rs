//! Interrompre l'utilisateur quand Ash n'est pas devant lui (spec §8).
//!
//! C'est le seul endroit du produit qui a le droit de sortir de la fenêtre. Trois règles le
//! gouvernent, et elles sont ici — pures, donc éprouvées — plutôt que dans le fil qui poste :
//!
//! - **trois états, et trois seulement** : [`SwitchableState`] — `waiting`, `error` et
//!   `done`, les seuls qui aient une phrase à dire. `working` et `idle` n'en ont pas : un
//!   agent qui travaille n'a rien à annoncer, et aucun réglage ne peut lui en donner, parce
//!   qu'ils n'ont pas de variante. Des trois, **`done` est éteint par défaut** (spec §8), et
//!   l'allumer est un geste de l'utilisateur, tenu par [`super::NotificationChoices`] ;
//! - **seulement quand Ash n'est pas au premier plan.** L'utilisateur qui regarde la
//!   sidebar a déjà l'information : la doubler d'une bannière système serait du bruit ;
//! - **seulement sur un changement d'état.** L'état est *lu* trois fois par seconde par la
//!   boucle de sonde d'[ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md) ;
//!   accrocher la notification à la lecture en poserait trois par seconde. C'est
//!   [`super::AgentMachine`] qui distingue les deux — elle rend `Some(état)` quand il
//!   **change** — et [`super::Supervisor`] ne consulte que ce `Some`.
//!
//! **[`Notice::tab_id`] est l'onglet que le clic sélectionne.** Il voyage avec la bannière —
//! `features::notifications` le confie à macOS et le rend tel quel quand l'utilisateur
//! clique — et le composition root en fait une sélection. Rien de ce chemin n'attend :
//! macOS rappelle par un délégué, et aucun fil d'Ash n'est garé en attendant un geste qui,
//! le plus souvent, ne vient pas.
//!
//! **Rien ici ne sélectionne pourtant quoi que ce soit, et c'est structurel.** Le port ne
//! rend rien, ne prend aucun `AppHandle`, et n'a aucun moyen de changer l'onglet actif ni de
//! mettre la fenêtre au premier plan. La sélection a une seule source, et c'est le clic :
//! c'est la forme que prend l'interdiction de la spec §8 et d'
//! [ADR-0010](../../../../docs/adr/0010-sidebar-informe-terminal-agit.md) — Ash informe,
//! l'utilisateur agit.

use super::preferences::NotificationChoices;
use super::state::AgentState;

/// Un état qui **peut** interrompre l'utilisateur — c'est-à-dire un des trois interrupteurs.
///
/// Un type plutôt qu'une convention, parce que c'est lui qui tient la propriété de la
/// section : **un interrupteur commande une bannière, et une bannière a un interrupteur.**
/// Les trois listes qui devaient jusqu'ici s'accorder — la constante, les phrases de
/// [`words`], les champs de [`super::NotificationChoices`] — s'accordent maintenant sur des
/// variantes, et chacune de ces fonctions est **totale** : offrir un interrupteur qui ne
/// commande rien demanderait une variante sans phrase, et poser une bannière sans
/// interrupteur demanderait une phrase sans variante. Ni l'un ni l'autre ne compile.
///
/// Les deux états qui n'y sont pas n'ont rien à annoncer, et aucun réglage ne peut leur en
/// donner : un agent qui travaille, ou qui ne fait rien, n'interrompt personne.
///
/// **Ce type ne traverse pas la frontière Tauri** : ce que la fenêtre reçoit et renvoie est
/// l'un des cinq mots d'[`AgentState`], et [`TryFrom`] est le seul passage des cinq aux
/// trois.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchableState {
    Waiting,
    Error,
    Done,
}

impl From<SwitchableState> for AgentState {
    fn from(state: SwitchableState) -> Self {
        match state {
            SwitchableState::Waiting => AgentState::Waiting,
            SwitchableState::Error => AgentState::Error,
            SwitchableState::Done => AgentState::Done,
        }
    }
}

impl TryFrom<AgentState> for SwitchableState {
    type Error = NotInterrupting;

    /// Le seul rétrécissement des cinq états aux trois, et donc le seul endroit où décider
    /// qu'un état n'interrompt pas. Le `match` est exhaustif : un sixième état d'agent ne
    /// compilerait pas tant que personne n'aurait dit s'il a une bannière.
    fn try_from(state: AgentState) -> Result<Self, NotInterrupting> {
        match state {
            AgentState::Waiting => Ok(SwitchableState::Waiting),
            AgentState::Error => Ok(SwitchableState::Error),
            AgentState::Done => Ok(SwitchableState::Done),
            AgentState::Idle | AgentState::Working => Err(NotInterrupting),
        }
    }
}

/// Cet état n'a rien à annoncer — il n'a donc ni bannière, ni interrupteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotInterrupting;

/// Les trois interrupteurs, dans l'ordre de la spec §8.
///
/// La liste est publique parce que c'est elle qui les **ordonne** : la fenêtre de réglages
/// dessine une ligne par élément, et cet ordre est une décision de cette feature, pas d'une
/// vue. Ce qu'elle contient, en revanche, ne se décide plus ici — c'est
/// [`SwitchableState`] tout entier.
///
/// Ce que l'utilisateur en laisse passer est l'autre moitié de la règle, et elle est dans
/// [`super::preferences`] : cette liste-ci dit ce qui est **possible**, elle ne dit pas ce
/// qui est allumé.
pub const SWITCHABLE_STATES: [SwitchableState; 3] = [
    SwitchableState::Waiting,
    SwitchableState::Error,
    SwitchableState::Done,
];

/// Ce qu'une notification porte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// L'onglet concerné — celui que le clic sur la bannière sélectionne.
    pub tab_id: String,
    /// L'état qui a justifié l'interruption — l'un des trois, par construction.
    pub state: SwitchableState,
    pub title: String,
    pub body: String,
}

/// L'effet système « prévenir l'utilisateur hors d'Ash ».
///
/// Un trait, pour la raison habituelle du dépôt : aucun test ne doit poser une vraie
/// bannière sur l'écran de qui lance `cargo test`. Il ne rend rien — une notification perdue
/// ne change aucun état — et c'est aussi ce qui garantit qu'aucune règle ne peut dépendre de
/// sa réussite.
pub trait Notifier: Send + Sync {
    fn post(&self, notice: Notice);
}

/// Faut-il interrompre l'utilisateur, et avec quel texte ?
///
/// `changed` est le `Some(état)` de la machine : un état **lu** n'entre jamais ici, et c'est
/// ce qui fait qu'un `waiting` qui dure ne notifie qu'une fois.
///
/// Les quatre règles sont ici, et une seule fois chacune : Ash est-il devant, l'état a-t-il
/// quelque chose à annoncer — c'est-à-dire est-il un [`SwitchableState`] —, l'utilisateur
/// laisse-t-il cet état l'interrompre, et le `changed` du seul appelant qui sache distinguer
/// un changement d'une lecture.
///
/// **L'ordre des trois premières compte**, et il ne se rediscute pas dans un réglage : le
/// premier plan passe avant tout, donc aucun interrupteur ne peut faire sortir une bannière
/// pendant que l'utilisateur regarde Ash.
///
/// **`choices` filtre, il n'ajoute rien** : un état sans variante n'arrive même pas jusqu'à
/// lui, quoi qu'un fichier de préférence raconte.
pub fn notice(
    tab_id: &str,
    changed: AgentState,
    focused: bool,
    choices: NotificationChoices,
) -> Option<Notice> {
    if focused {
        return None;
    }
    let announced = SwitchableState::try_from(changed).ok()?;
    if !choices.allows(announced) {
        return None;
    }
    let (title, body) = words(announced);
    Some(Notice {
        tab_id: tab_id.to_owned(),
        state: announced,
        title: title.to_owned(),
        body: body.to_owned(),
    })
}

/// Ce que la bannière dit, pour chacun des trois états qui interrompent.
///
/// **Totale** : chaque interrupteur a sa phrase, et c'est le type qui l'exige.
///
/// Le texte ne nomme pas encore l'onglet : cette feature ne connaît d'un onglet que son
/// identifiant, qui n'est pas un nom à montrer. Le jour où la notification saura dire
/// « ash · sidebar », c'est ici que le nom entrera — et pas dans le composition root.
fn words(state: SwitchableState) -> (&'static str, &'static str) {
    match state {
        SwitchableState::Waiting => (
            "an agent is waiting",
            "it asked a question, and nothing moves until you answer.",
        ),
        SwitchableState::Error => (
            "an agent stopped on an error",
            "it left without finishing its work.",
        ),
        // `done` a une phrase, et pourtant il ne dérange personne par défaut : c'est
        // l'interrupteur éteint de la spec §8 qui le retient, pas l'absence de texte. Écrire
        // la phrase est ce qui rend le réglage possible ; la laisser sous l'interrupteur est
        // ce qui garde la règle.
        SwitchableState::Done => (
            "an agent finished",
            "it declared the end of its work, and its tab is idle again.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : les interrupteurs de la spec §8, tels qu'Ash sort de sa boîte.
    fn by_default() -> NotificationChoices {
        NotificationChoices::default()
    }

    #[test]
    fn given_ash_behind_another_window_when_an_agent_asks_a_question_then_the_user_is_interrupted()
    {
        // Given — le critère de sortie du jalon : un agent en `waiting` doit être vu en
        // moins de dix secondes, même hors d'Ash. C'est la seule raison d'être de tout ce
        // module.
        let focused = false;

        // When
        let posted = notice("01J0TAB", AgentState::Waiting, focused, by_default());

        // Then
        assert_eq!(
            posted.map(|notice| notice.tab_id),
            Some("01J0TAB".to_owned())
        );
    }

    #[test]
    fn given_ash_in_the_foreground_when_an_agent_asks_a_question_then_nothing_interrupts_anyone() {
        // Given — la sidebar montre déjà l'état sous les yeux de l'utilisateur (spec §8) :
        // une bannière système par-dessus serait du bruit, et le bruit est ce qui fait
        // couper les notifications.
        let focused = true;

        // When
        let posted = notice("01J0TAB", AgentState::Waiting, focused, by_default());

        // Then
        assert_eq!(posted, None);
    }

    #[test]
    fn given_an_agent_that_finished_its_work_when_it_declares_done_then_it_does_not_notify() {
        // Given — « `done` ne notifie pas en v1 » (spec §8). Un travail fini n'attend rien
        // de l'utilisateur : l'interrompre pour ça userait la seule interruption qui compte.
        // Cette règle est un refus, donc rien ne l'attraperait si elle disparaissait — et
        // depuis que `done` a une phrase, il n'y a plus que l'interrupteur pour la tenir.
        let states = [AgentState::Done, AgentState::Working, AgentState::Idle];

        // When
        let posted: Vec<_> = states
            .iter()
            .filter_map(|state| notice("01J0TAB", *state, false, by_default()))
            .collect();

        // Then
        assert_eq!(posted, vec![]);
    }

    #[test]
    fn given_a_user_who_turned_waiting_off_when_an_agent_asks_a_question_then_no_banner_is_posted()
    {
        // Given — l'interrupteur de la spec §9 doit **couper** la bannière, pas la cacher
        // après coup : une notification filtrée par l'écran serait déjà passée devant les
        // yeux de l'utilisateur, et c'est exactement ce qu'il a demandé à ne plus voir
        let muted = by_default().with(AgentState::Waiting, false);

        // When
        let posted = notice("01J0TAB", AgentState::Waiting, false, muted);

        // Then
        assert_eq!(posted, None);
    }

    #[test]
    fn given_a_user_who_turned_done_on_when_an_agent_declares_the_end_of_its_work_then_a_banner_says_so(
    ) {
        // Given — le symétrique, et la seule façon de changer le défaut : sans lui,
        // l'interrupteur `done` de la fenêtre serait un bouton qui ne commande rien
        let asked_for_it = by_default().with(AgentState::Done, true);

        // When
        let posted = notice("01J0TAB", AgentState::Done, false, asked_for_it);

        // Then
        let posted = posted.expect("done notifies once it is turned on");
        assert_eq!(posted.state, SwitchableState::Done);
        assert_eq!(posted.title, "an agent finished");
    }

    #[test]
    fn given_an_agent_that_died_without_declaring_its_end_when_it_turns_to_error_then_the_user_is_told(
    ) {
        // Given — le second des deux états de la spec §8. Un agent parti en panne pendant
        // qu'on regarde ailleurs est exactement ce qu'on ne veut pas découvrir une heure
        // plus tard.
        // When
        let posted = notice("01J0TAB", AgentState::Error, false, by_default());

        // Then
        let posted = posted.expect("error notifies");
        assert_eq!(posted.state, SwitchableState::Error);
        assert_eq!(posted.title, "an agent stopped on an error");
    }

    #[test]
    fn given_the_five_states_when_the_settings_window_offers_a_switch_then_it_offers_exactly_those_a_banner_could_announce(
    ) {
        // Given — la fenêtre de réglages dessine un interrupteur par `SWITCHABLE_STATES`
        // (spec §8, dernière puce ; spec §9, `[notifications]`) pendant que la bannière, elle,
        // obéit à `words`. Depuis que les deux sont indexées par `SwitchableState`, un
        // interrupteur sans bannière ne compile plus ; ce qui reste à vérifier est le pont
        // avec les cinq états du contrat — que `TryFrom` en laisse passer exactement trois,
        // et pas un `working` qui offrirait une bannière permanente.
        let states = [
            AgentState::Idle,
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Error,
            AgentState::Done,
        ];
        // Tout allumé : cette liste-ci décrit ce qui est **possible**, pas ce qui est choisi.
        let everything = NotificationChoices {
            waiting: true,
            error: true,
            done: true,
        };

        // When
        let with_something_to_say: Vec<AgentState> = states
            .into_iter()
            .filter(|state| notice("01J0TAB", *state, false, everything).is_some())
            .collect();

        // Then
        assert_eq!(
            with_something_to_say,
            SWITCHABLE_STATES.map(AgentState::from).to_vec(),
            "chaque interrupteur commande une bannière, et réciproquement"
        );
    }
}
