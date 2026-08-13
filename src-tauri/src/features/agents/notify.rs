//! Interrompre l'utilisateur quand Ash n'est pas devant lui (spec §8).
//!
//! C'est le seul endroit du produit qui a le droit de sortir de la fenêtre. Trois règles le
//! gouvernent, et elles sont ici — pures, donc éprouvées — plutôt que dans le fil qui poste :
//!
//! - **deux états, et deux seulement** : `waiting` et `error`. `done` ne notifie pas en v1
//!   (spec §8), et `working`/`idle` encore moins — un agent qui travaille n'a rien à dire ;
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

use super::state::AgentState;

/// Les états qui interrompent l'utilisateur, dans l'ordre de la spec §8.
///
/// La liste est publique parce que la fenêtre de réglages l'affiche : ce qu'Ash notifie est
/// une décision de cette feature, pas une phrase recopiée dans une vue.
///
/// **Elle décrit [`words`], elle ne la double pas.** C'est le fait d'avoir un texte à dire
/// qui décide qu'un état interrompt, et rien d'autre : une liste qui déciderait *aussi*
/// ferait deux règles à tenir d'accord, et la fenêtre de réglages promettrait une bannière
/// qu'Ash n'enverrait jamais. Le test de fin de fichier tient les deux ensemble.
pub const NOTIFIED_STATES: [AgentState; 2] = [AgentState::Waiting, AgentState::Error];

/// Ce qu'une notification porte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// L'onglet concerné — celui que le clic sur la bannière sélectionne.
    pub tab_id: String,
    /// L'état qui a justifié l'interruption. Toujours l'un de [`NOTIFIED_STATES`].
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
/// Les trois règles sont ici, et une seule fois chacune : Ash est-il devant, l'état a-t-il
/// un texte à dire — c'est ce qui définit [`NOTIFIED_STATES`] — et le `changed` du seul
/// appelant qui sache distinguer un changement d'une lecture.
pub fn notice(tab_id: &str, changed: AgentState, focused: bool) -> Option<Notice> {
    if focused {
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
        AgentState::Idle | AgentState::Working | AgentState::Done => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_ash_behind_another_window_when_an_agent_asks_a_question_then_the_user_is_interrupted()
    {
        // Given — le critère de sortie du jalon : un agent en `waiting` doit être vu en
        // moins de dix secondes, même hors d'Ash. C'est la seule raison d'être de tout ce
        // module.
        let focused = false;

        // When
        let posted = notice("01J0TAB", AgentState::Waiting, focused);

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
        let posted = notice("01J0TAB", AgentState::Waiting, focused);

        // Then
        assert_eq!(posted, None);
    }

    #[test]
    fn given_an_agent_that_finished_its_work_when_it_declares_done_then_it_does_not_notify() {
        // Given — « `done` ne notifie pas en v1 » (spec §8). Un travail fini n'attend rien
        // de l'utilisateur : l'interrompre pour ça userait la seule interruption qui compte.
        // Cette règle est un refus, donc rien ne l'attraperait si elle disparaissait.
        let states = [AgentState::Done, AgentState::Working, AgentState::Idle];

        // When
        let posted: Vec<_> = states
            .iter()
            .filter_map(|state| notice("01J0TAB", *state, false))
            .collect();

        // Then
        assert_eq!(posted, vec![]);
    }

    #[test]
    fn given_an_agent_that_died_without_declaring_its_end_when_it_turns_to_error_then_the_user_is_told(
    ) {
        // Given — le second des deux états de la spec §8. Un agent parti en panne pendant
        // qu'on regarde ailleurs est exactement ce qu'on ne veut pas découvrir une heure
        // plus tard.
        // When
        let posted = notice("01J0TAB", AgentState::Error, false);

        // Then
        let posted = posted.expect("error notifies");
        assert_eq!(posted.state, AgentState::Error);
        assert_eq!(posted.title, "an agent stopped on an error");
    }

    #[test]
    fn given_the_five_states_when_the_settings_window_names_what_interrupts_then_it_names_exactly_what_a_banner_would_say(
    ) {
        // Given — la fenêtre de réglages affiche `NOTIFIED_STATES` (spec §8, dernière puce)
        // pendant que la bannière, elle, obéit à `words`. Deux listes, donc deux façons de
        // dériver : un état ajouté à la constante seule ferait promettre aux réglages une
        // interruption qu'Ash n'enverrait jamais, et le retirer d'elle seule cacherait une
        // bannière qui continuerait de sortir. Rien d'autre que ceci ne les tient ensemble.
        let states = [
            AgentState::Idle,
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Done,
            AgentState::Error,
        ];

        // When
        let with_something_to_say: Vec<AgentState> = states
            .into_iter()
            .filter(|state| notice("01J0TAB", *state, false).is_some())
            .collect();

        // Then
        assert_eq!(with_something_to_say, NOTIFIED_STATES.to_vec());
    }
}
