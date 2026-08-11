//! Les agents : leur vocabulaire d'états, et ce qui le produit.
//!
//! La feature est encore réduite à son vocabulaire. C'est délibéré : les cinq états sont
//! la seule chose que le reste du produit a le droit de connaître
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)), et ils doivent donc
//! exister avant les trois mécanismes qui les alimenteront — le socket d'événements, les
//! adaptateurs, et la machine à états.

mod state;

pub use state::AgentState;
