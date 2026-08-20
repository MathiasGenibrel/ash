//! Qui travaille dans un worktree — le port que `git` possède.
//!
//! C'est ce qui fait la valeur de la popup de branches (spec §7.1) : un checkout déplace des
//! fichiers sous les pieds de qui écrit dedans, et aucun client git ne le dit. Pour le dire,
//! il faut savoir **qui** — pas « un agent tourne », mais `claude`, dans `ash-sidebar`.
//!
//! `git` ne connaît ni les onglets ni les hooks : il connaît ce trait, et le composition
//! root le branche sur le registre des PTY. C'est la même forme que
//! [`pty::AgentStates`](crate::features::pty::AgentStates), prise dans l'autre sens — là,
//! `pty` demande un état à `agents` ; ici, `git` demande à `pty` qui habite un worktree. Les
//! deux features continuent de s'ignorer.
//!
//! Ce que le trait ne rend **pas**, et c'est délibéré : aucun moyen d'agir. Mettre un agent
//! en pause reste une commande de `pty`, déclenchée par un geste de l'utilisateur sur la
//! question qu'on lui a posée. `git` sait qu'il dérangerait quelqu'un ; il n'a pas la main
//! sur lui.

use std::path::Path;

use crate::features::agents::AgentState;

/// Un agent qui écrit dans un worktree, nommé.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BusyAgent {
    /// L'onglet qui le porte — c'est par lui que la pause le retrouve.
    pub tab_id: String,
    /// Le nom de l'outil, tel que la sidebar l'affiche — `claude`.
    pub name: String,
    pub state: AgentState,
    /// Son groupe en avant-plan est déjà arrêté (`SIGSTOP`).
    ///
    /// Un agent en pause reste dans la liste : il n'écrit plus, mais il écrira à nouveau dès
    /// qu'on le reprendra, et le faire disparaître de l'avertissement laisserait croire
    /// qu'il n'y a plus personne dans ce worktree.
    pub paused: bool,
}

/// À qui `git` demande qui habite un worktree.
pub trait WorkingAgents: Send + Sync {
    /// Les agents **en danger** dans ce worktree, dans l'ordre de leurs onglets.
    ///
    /// « En danger » est une règle, et elle vit chez l'appelant du port plutôt que dans son
    /// implémentation : voir [`at_risk`].
    fn in_worktree(&self, worktree_root: &Path) -> Vec<BusyAgent>;
}

/// Cet état d'agent est-il mis en danger par un geste sur l'arbre de travail ?
///
/// Trois cas, et la frontière n'est pas « est-il en train d'écrire à cet instant » :
///
/// - **`working`** — il écrit maintenant. Le cas évident.
/// - **`waiting`** — il attend une réponse de l'utilisateur, et il se remettra à écrire dès
///   qu'il l'aura. Ses fichiers sont son contexte : un checkout entre les deux lui fait
///   reprendre sur un arbre qui n'est plus celui qu'il a lu. C'est le cas **le plus** piégeux
///   des trois, parce que rien ne bouge à l'écran pendant qu'on fait le checkout.
/// - **`idle`, `done`, `error`** — il n'y a personne à déranger. Un agent fini ne reprendra
///   pas, et un shell à son invite n'a pas de contexte à perdre. Avertir pour eux ferait
///   sonner l'avertissement en permanence, donc ne le ferait plus lire du tout.
///
/// Un agent **en pause** reste en danger : la pause est ce qu'on propose pour rendre le geste
/// sûr, pas ce qui fait disparaître la question.
pub fn at_risk(state: AgentState) -> bool {
    matches!(state, AgentState::Working | AgentState::Waiting)
}

/// Le constructeur de test de [`BusyAgent`] — quatre champs, dont trois ont un défaut valide.
#[cfg(test)]
pub struct BusyAgentBuilder(BusyAgent);

#[cfg(test)]
impl BusyAgentBuilder {
    pub fn new() -> Self {
        Self(BusyAgent {
            tab_id: "01J0TAB".to_owned(),
            name: "claude".to_owned(),
            state: AgentState::Working,
            paused: false,
        })
    }

    pub fn name(mut self, name: &str) -> Self {
        self.0.name = name.to_owned();
        self
    }

    pub fn state(mut self, state: AgentState) -> Self {
        self.0.state = state;
        self
    }

    pub fn build(self) -> BusyAgent {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_an_agent_that_is_writing_when_asking_whether_a_checkout_would_disturb_it_then_it_would(
    ) {
        // Given
        let state = AgentState::Working;

        // When
        let disturbed = at_risk(state);

        // Then
        assert!(disturbed);
    }

    #[test]
    fn given_an_agent_waiting_for_an_answer_when_asking_whether_a_checkout_would_disturb_it_then_it_would(
    ) {
        // Given — il ne bouge pas, et c'est ce qui rend le cas piégeux
        let state = AgentState::Waiting;

        // When
        let disturbed = at_risk(state);

        // Then — il reprendra sur un arbre qui n'est plus celui qu'il a lu
        assert!(disturbed);
    }

    #[test]
    fn given_a_finished_or_idle_agent_when_asking_whether_a_checkout_would_disturb_it_then_it_would_not(
    ) {
        // Given
        let quiet = [AgentState::Idle, AgentState::Done, AgentState::Error];

        // When
        let disturbed: Vec<bool> = quiet.into_iter().map(at_risk).collect();

        // Then — un avertissement qui sonne toujours ne se lit plus
        assert_eq!(disturbed, vec![false, false, false]);
    }
}
