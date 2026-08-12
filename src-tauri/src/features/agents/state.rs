/// Les cinq états d'un agent — le vocabulaire commun du produit.
///
/// C'est **le backend** qui les détient
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : le frontend rend
/// un état, il ne le calcule pas. Et c'est la feature `agents` qui les déclare, et non
/// celle qui les affiche, parce qu'[ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)
/// en fait la frontière que les particularités d'un outil n'ont pas le droit de franchir :
/// un adaptateur traduit vers ces cinq mots, et le cœur n'en connaît pas d'autres.
///
/// Ils ont d'abord vécu dans `pty`, la seule feature qui en produisait. Les faire remonter
/// ici est ce qui permet aux trois producteurs à venir — socket d'événements, adaptateurs,
/// machine à états — de partager le même type sans dépendre les uns des autres.
///
/// À ce jalon, seuls `Idle` et `Working` ont un producteur : la sonde
/// d'[ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md), qui sait si le shell est à
/// son invite ou si autre chose tient l'avant-plan. `Waiting`, `Done` et `Error` viendront
/// des **hooks** ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)), qui interdit de
/// les déduire de la sortie du PTY. Rien ne les produit aujourd'hui, et c'est le
/// comportement correct.
///
/// La représentation sérialisée est le contrat partagé avec le TypeScript
/// (`src/shared/ipc`) : cinq mots en minuscules, et `presentAgentState` en face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum AgentState {
    Idle,
    Working,
    Waiting,
    Done,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_the_five_agent_states_when_they_cross_the_boundary_then_they_keep_the_names_the_frontend_knows(
    ) {
        // Given — le même modèle est déclaré des deux côtés de la frontière : `AgentState`
        // ici, `AgentState` dans `src/shared/ipc/index.ts`. Rien ne les tient ensemble à
        // la compilation, et un état renommé ici ferait silencieusement tomber la sidebar
        // sur `undefined`. Le `match` est exhaustif : un état ajouté ne compile pas tant
        // que son nom n'a pas été décidé — et donc reporté côté TypeScript.
        let states = [
            AgentState::Idle,
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Done,
            AgentState::Error,
        ];
        let expected = states.map(|state| match state {
            AgentState::Idle => "idle",
            AgentState::Working => "working",
            AgentState::Waiting => "waiting",
            AgentState::Done => "done",
            AgentState::Error => "error",
        });

        // When
        let on_the_wire: Vec<String> = states
            .iter()
            .map(|state| serde_json::to_string(state).unwrap())
            .collect();

        // Then
        assert_eq!(
            on_the_wire,
            expected
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
        );
    }
}
