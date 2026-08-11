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
#[cfg(test)]
pub(crate) mod contract;
mod state;

pub use adapter::{Adapter, Instrumentation, RawEvent, SubagentSupport};
pub use adapters::GenericAdapter;
pub use state::AgentState;
