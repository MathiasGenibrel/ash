use std::path::Path;

use crate::features::agents::adapter::{
    Adapter, ChildEvent, Instrumentation, RawEvent, SubagentSupport,
};
use crate::features::agents::state::AgentState;
use crate::features::agents::usage::{ModelSource, UsageSupport};

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

    /// Rien non plus du côté des enfants, et pour la même raison qu'ailleurs : un outil dont
    /// Ash n'a rien instrumenté n'a envoyé aucun événement, donc aucun enfant à nommer.
    ///
    /// C'est ce que `SubagentSupport::None` promet, et la suite contractuelle le vérifie :
    /// **aucune ligne fille ne peut apparaître** sous un onglet de cet outil, et rien
    /// n'ira suggérer à l'utilisateur qu'il en manque (spec §6.5).
    fn child_event(&self, _raw: &RawEvent) -> Option<ChildEvent> {
        None
    }

    fn subagents(&self) -> SubagentSupport {
        SubagentSupport::None
    }

    /// Rien de la place consommée non plus, et c'est la même logique qu'ailleurs.
    ///
    /// Un outil dont Ash ne sait rien ne tient pas forcément de transcript, et s'il en tient
    /// un, Ash n'en connaît pas le format. Répondre `None` est ce qui fait que la barre
    /// d'état reste **exactement** ce qu'elle était pour cet onglet : pas de jauge vide, pas
    /// de `ctx —`, rien qui laisse croire qu'une mesure a échoué.
    fn usage(&self) -> UsageSupport {
        UsageSupport::None
    }

    /// Et donc jamais de mesure, quel que soit le texte qu'on lui présente.
    ///
    /// C'est ce que `UsageSupport::None` promet, et la suite contractuelle le vérifie sur un
    /// vrai transcript de Claude Code : le socle ne doit pas se mettre à lire le format d'un
    /// autre outil « en attendant » son adaptateur.
    fn read_used_tokens(&self, _transcript_tail: &str) -> Option<u64> {
        None
    }

    /// Aucune configuration à consulter, donc aucun fichier ouvert.
    ///
    /// C'est le pendant exact de la promesse ci-dessus : un outil dont Ash ne sait rien n'a
    /// pas de `settings.json` connu, et une liste vide est ce qui garantit qu'un onglet servi
    /// par le socle ne fait ouvrir aucun fichier — pas même pour se voir répondre `None`.
    fn model_sources(&self, _cwd: Option<&Path>, _home: Option<&Path>) -> Vec<ModelSource> {
        Vec::new()
    }

    /// Et aucune fenêtre pour aucun identifiant : la table des modèles est la connaissance
    /// d'un outil, et le socle n'en est pas un.
    fn context_window(&self, _model: &str) -> Option<u64> {
        None
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
        let report = check_adapter_contract(&adapter, &[], None);

        // Then
        assert!(report.is_satisfied(), "violations :\n{report}");
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
