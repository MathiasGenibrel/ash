//! Ce que la feature `agents` expose au frontend.
//!
//! Pas encore de `#[tauri::command]` : à ce jalon, rien ne se demande, tout se pousse.
//! Le backend détient l'état ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)),
//! et le frontend l'apprend par cet event.

use super::wire::EventFrame;

/// Nom de l'event qui porte un événement de hook jusqu'à la webview.
///
/// Contrat avec le TypeScript, au même titre que `ash://tab-changed` : une chaîne que rien
/// ne vérifie à la compilation.
pub const AGENT_EVENT: &str = "ash://agent-event";

/// Un événement de hook, tel que la webview le voit.
///
/// Distinct de l'[`EventFrame`] qui circule sur le socket, et c'est volontaire : le format
/// du fil est un protocole avec `ash-event`, celui-ci est le contrat avec le frontend. Les
/// deux évoluent pour des raisons différentes — l'un avec les outils qu'Ash instrumente,
/// l'autre avec ce que l'interface montre — et les confondre reviendrait à faire dépendre
/// une mise à jour d'Ash de la version d'`ash-event` installée dans un `settings.json`.
///
/// Il porte le verbe **brut**, pas un `AgentState` : le traduire est le travail de
/// l'adaptateur d'[ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md), et décider
/// quoi en faire celui de la machine à états d'ADR-0007 §6.4. Ni l'un ni l'autre n'existe
/// encore, et le frontend n'a donc rien à en déduire de son côté.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub tab_id: String,
    pub kind: String,
}

impl From<&EventFrame> for AgentEvent {
    fn from(frame: &EventFrame) -> Self {
        Self {
            tab_id: frame.tab_id.clone(),
            kind: frame.kind.clone(),
        }
    }
}
