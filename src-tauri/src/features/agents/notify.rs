//! Interrompre l'utilisateur quand Ash n'est pas devant lui (spec §8).
//!
//! C'est le seul endroit du produit qui a le droit de sortir de la fenêtre. Trois règles le
//! gouvernent, et elles sont ici — pures, donc éprouvées — plutôt que dans le fil qui poste :
//!
//! - **trois états, et trois seulement** : `waiting`, `error` et `done` — les seuls qui aient
//!   une phrase à dire. `working` et `idle` n'en ont pas : un agent qui travaille n'a rien à
//!   annoncer, et aucun réglage ne peut lui en donner. Des trois, **`done` est éteint par
//!   défaut** (spec §8), et l'allumer est un geste de l'utilisateur, tenu par
//!   [`super::NotificationChoices`] ;
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

/// Les états qui **peuvent** interrompre l'utilisateur, dans l'ordre de la spec §8.
///
/// C'est-à-dire, exactement, les trois interrupteurs que la fenêtre de réglages montre : la
/// liste est publique parce que c'est elle qui les fait exister. Ce qu'Ash sait notifier est
/// une décision de cette feature, pas une liste recopiée dans une vue.
///
/// **Elle décrit [`words`], elle ne la double pas.** C'est le fait d'avoir un texte à dire
/// qui décide qu'un état puisse interrompre, et rien d'autre : une liste qui déciderait
/// *aussi* ferait deux règles à tenir d'accord, et la fenêtre de réglages offrirait un
/// interrupteur qui ne commande rien. Le test de fin de fichier tient les deux ensemble.
///
/// Ce que l'utilisateur en laisse passer est l'autre moitié de la règle, et elle est dans
/// [`super::preferences`] : cette liste-ci dit ce qui est **possible**, elle ne dit pas ce
/// qui est allumé.
pub const SWITCHABLE_STATES: [AgentState; 3] =
    [AgentState::Waiting, AgentState::Error, AgentState::Done];

/// Ce qu'une notification porte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// L'onglet concerné — celui que le clic sur la bannière sélectionne.
    pub tab_id: String,
    /// L'état qui a justifié l'interruption. Toujours l'un de [`SWITCHABLE_STATES`].
    pub state: AgentState,
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
/// un texte à dire — c'est ce qui définit [`SWITCHABLE_STATES`] —, l'utilisateur laisse-t-il
/// cet état l'interrompre, et le `changed` du seul appelant qui sache distinguer un
/// changement d'une lecture.
///
/// **`choices` filtre, il n'ajoute rien** : un état sans phrase ne devient pas notifiable
/// parce qu'un fichier de préférence le dit. C'est ce qui garde `words` seul maître de ce
/// qu'une bannière peut annoncer.
pub fn notice(
    tab_id: &str,
    changed: AgentState,
    focused: bool,
    choices: NotificationChoices,
) -> Option<Notice> {
    if focused {
        return None;
    }
    if !choices.allows(changed) {
        return None;
    }
    let (title, body) = words(changed)?;
    Some(Notice {
        tab_id: tab_id.to_owned(),
        state: changed,
        title: title.to_owned(),
        body: body.to_owned(),
    })
}

/// Ce que la bannière dit, pour chacun des deux états qui interrompent.
///
/// Le texte ne nomme pas encore l'onglet : cette feature ne connaît d'un onglet que son
/// identifiant, qui n'est pas un nom à montrer. Le jour où la notification saura dire
/// « ash · sidebar », c'est ici que le nom entrera — et pas dans le composition root.
fn words(state: AgentState) -> Option<(&'static str, &'static str)> {
    match state {
        AgentState::Waiting => Some((
            "an agent is waiting",
            "it asked a question, and nothing moves until you answer.",
        )),
        AgentState::Error => Some((
            "an agent stopped on an error",
            "it left without finishing its work.",
        )),
        // `done` a une phrase, et pourtant il ne dérange personne par défaut : c'est
        // l'interrupteur éteint de la spec §8 qui le retient, pas l'absence de texte. Écrire
        // la phrase est ce qui rend le réglage possible ; la laisser sous l'interrupteur est
        // ce qui garde la règle.
        AgentState::Done => Some((
            "an agent finished",
            "it declared the end of its work, and its tab is idle again.",
        )),
        AgentState::Idle | AgentState::Working => None,
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
        assert_eq!(posted.state, AgentState::Done);
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
        assert_eq!(posted.state, AgentState::Error);
        assert_eq!(posted.title, "an agent stopped on an error");
    }

    #[test]
    fn given_the_five_states_when_the_settings_window_offers_a_switch_then_it_offers_exactly_those_a_banner_could_announce(
    ) {
        // Given — la fenêtre de réglages dessine un interrupteur par `SWITCHABLE_STATES`
        // (spec §8, dernière puce ; spec §9, `[notifications]`) pendant que la bannière, elle,
        // obéit à `words`. Deux listes, donc deux façons de dériver : un état ajouté à la
        // constante seule offrirait un interrupteur qui ne commande rien, et le retirer d'elle
        // seule cacherait une bannière qui continuerait de sortir. Rien d'autre que ceci ne
        // les tient ensemble.
        let states = [
            AgentState::Idle,
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Done,
            AgentState::Error,
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
            with_something_to_say.len(),
            SWITCHABLE_STATES.len(),
            "chaque interrupteur commande une bannière, et réciproquement"
        );
        for state in SWITCHABLE_STATES {
            assert!(with_something_to_say.contains(&state));
        }
    }
}
