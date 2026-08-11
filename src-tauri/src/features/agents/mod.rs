//! Les agents : leur vocabulaire d'états, et ce qui le produit.
//!
//! Les cinq états sont la seule chose que le reste du produit a le droit de connaître
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)) ; le trait [`Adapter`]
//! est la frontière qui les tient. Un outil y traduit son vocabulaire, et n'a aucun moyen
//! d'en faire passer un sixième mot.
//!
//! Ce que la feature ne possède pas encore : le socket d'événements qui produira les
//! [`RawEvent`], et la machine à états qui arbitrera entre eux et la sonde. Un adaptateur
//! traduit ; il n'arbitre pas, et il ne connaît ni l'onglet ni l'horloge.

mod adapter;
mod adapters;
/// Privé et `#[cfg(test)]` : la suite contractuelle sert les implémentations de cette
/// feature, et personne d'autre. L'ouvrir au reste du crate inviterait une autre feature à
/// vérifier un adaptateur qu'elle n'a pas écrit — donc à connaître le trait par l'intérieur.
#[cfg(test)]
mod contract;
mod state;

pub use adapter::{Adapter, Instrumentation, RawEvent, SubagentSupport};
pub use adapters::GenericAdapter;
pub use state::AgentState;
