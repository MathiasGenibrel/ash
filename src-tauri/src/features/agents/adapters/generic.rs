use std::path::Path;

use crate::features::agents::adapter::{Adapter, Instrumentation, RawEvent, SubagentSupport};
use crate::features::agents::state::AgentState;

/// Le socle : l'adaptateur d'un outil dont on ne sait rien
/// ([ADR-0008](../../../../../docs/adr/0008-abstraction-adapter.md)).
///
/// Il n'instrumente rien et ne déclare aucune sous-tâche. Sa valeur n'est pas dans ce
/// qu'il calcule — il ne calcule rien — mais dans le fait qu'un outil inconnu **existe**
/// dans la sidebar au lieu d'être invisible : la découverte d'ADR-0006 le reconnaît par
/// son nom de processus, la sonde suit son `cwd`, et sa disparition est vue.
///
/// D'où ses états : `idle` / `done` / `error`, et rien d'autre. Ils viennent de la seule
/// sonde, c'est-à-dire de la seule chose qu'ADR-0007 l'autorise à dire — le processus est
/// là, ou il n'y est plus. `working` et `waiting` demandent que l'agent parle, et un outil
/// sans point d'instrumentation ne parle pas. Les produire ici voudrait dire les deviner à
/// partir de la sortie du PTY, ce qu'ADR-0007 écarte explicitement : un faux `waiting`
/// détruit la confiance dans la seule notification qui compte.
///
/// C'est aussi pourquoi [`Adapter::interpret`] ne rend jamais rien : aucun événement de
/// hook ne peut lui parvenir, puisqu'il n'en a fait installer aucun. S'il en arrivait un
/// malgré tout, il viendrait d'un outil que cet adaptateur ne connaît pas, et l'interpréter
/// serait une devinette.
#[derive(Debug, Clone, Copy, Default)]
pub struct GenericAdapter;

impl Adapter for GenericAdapter {
    fn id(&self) -> &str {
        "generic"
    }

    fn instrumentation(&self, _config_dir: &Path) -> Option<Instrumentation> {
        None
    }

    fn interpret(&self, _raw: &RawEvent) -> Option<AgentState> {
        None
    }

    fn subagents(&self) -> SubagentSupport {
        SubagentSupport::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::contract::check_adapter_contract;

    #[test]
    fn given_the_generic_adapter_when_it_is_run_through_the_adapter_contract_then_it_holds_every_invariant(
    ) {
        // Given — le socle est la première implémentation à passer la suite que toute
        // implémentation doit passer. Aucun événement propre : il n'en reçoit pas.
        let adapter = GenericAdapter;

        // When
        let report = check_adapter_contract(&adapter, &[]);

        // Then
        assert_eq!(report.violations, Vec::<String>::new());
    }

    #[test]
    fn given_an_event_named_like_another_tools_hook_when_generic_interprets_it_then_it_still_says_nothing(
    ) {
        // Given — `Stop` est un vrai hook de Claude Code. La tentation, le jour où un
        // adaptateur manquera, sera de le faire comprendre au socle « en attendant ».
        let adapter = GenericAdapter;
        let claude_stop = RawEvent::new("Stop").with_field("session_id", "01J");

        // When
        let interpreted = adapter.interpret(&claude_stop);

        // Then — un outil dont Ash n'a rien instrumenté ne peut rien avoir dit (ADR-0007)
        assert_eq!(interpreted, None);
    }
}
