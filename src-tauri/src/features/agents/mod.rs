//! Les agents : leur vocabulaire d'états, et ce qui le produit.
//!
//! Les cinq états sont la seule chose que le reste du produit a le droit de connaître
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)). Deux pièces se
//! partagent le travail, et la frontière entre elles est nette :
//!
//! - le trait [`Adapter`] **traduit** le vocabulaire d'un outil vers le nôtre, et n'a
//!   aucun moyen d'en faire passer un sixième mot ;
//! - [`AgentMachine`] **décide** de l'état d'un onglet à partir de ce qui lui arrive
//!   (spec §6.4). Un adaptateur traduit ; il n'arbitre pas, et il ne connaît ni l'onglet
//!   ni l'horloge.
//!
//! Ce que la feature ne possède pas encore : le socket d'événements qui produira les
//! [`RawEvent`], et la traduction qui les transformera en [`AgentEvent`]. La machine
//! reçoit des événements déjà traduits et ne sait pas comment ils sont arrivés — c'est
//! ce qui permet de prouver toutes les règles de la spec §6.4 sans lancer ni processus,
//! ni socket, ni minuteur.

mod adapter;
mod adapters;
/// Privé et `#[cfg(test)]` : la suite contractuelle sert les implémentations de cette
/// feature, et personne d'autre. L'ouvrir au reste du crate inviterait une autre feature à
/// vérifier un adaptateur qu'elle n'a pas écrit — donc à connaître le trait par l'intérieur.
#[cfg(test)]
mod contract;
mod machine;
mod state;

pub use adapter::{Adapter, Instrumentation, RawEvent, SubagentSupport};
pub use adapters::GenericAdapter;
pub use machine::{AgentEvent, AgentMachine, Declared, Exit, LINGER};
pub use state::AgentState;
