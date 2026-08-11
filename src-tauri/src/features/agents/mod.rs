//! Les agents : leur vocabulaire d'états, et ce qui le produit.
//!
//! Les cinq états sont la seule chose que le reste du produit a le droit de connaître
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)). Trois pièces se
//! partagent le travail, et les frontières entre elles sont nettes :
//!
//! - le **transport** ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)) — le
//!   socket unix par lequel un hook lancé dans un agent rejoint Ash, et le format qui y
//!   circule. C'est délibérément le côté qui **écoute** qui possède l'adresse du socket :
//!   `pty` la lui demande pour la poser dans `ASH_SOCK`, il n'en garde pas de copie ;
//! - le trait [`Adapter`], qui **traduit** le vocabulaire d'un outil vers le nôtre, et n'a
//!   aucun moyen d'en faire passer un sixième mot ;
//! - [`AgentMachine`], qui **décide** de l'état d'un onglet à partir de ce qui lui arrive
//!   (spec §6.4). Un adaptateur traduit ; il n'arbitre pas, et il ne connaît ni l'onglet
//!   ni l'horloge.
//!
//! Ce qui manque encore est la couture entre les trois : rien ne transforme aujourd'hui un
//! [`EventFrame`] arrivé du socket en [`RawEvent`], ni le [`RawEvent`] traduit en
//! [`AgentEvent`] posé sur la machine. La machine reçoit des événements déjà traduits et
//! ne sait pas comment ils sont arrivés — c'est ce qui permet de prouver toutes les règles
//! de la spec §6.4 sans lancer ni processus, ni socket, ni minuteur.

mod adapter;
mod adapters;
// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte
// pas. C'est aussi cohérent avec l'architecture — `commands.rs` *est* la surface
// publique de la feature vers le frontend.
pub mod commands;
/// Privé et `#[cfg(test)]` : la suite contractuelle sert les implémentations de cette
/// feature, et personne d'autre. L'ouvrir au reste du crate inviterait une autre feature à
/// vérifier un adaptateur qu'elle n'a pas écrit — donc à connaître le trait par l'intérieur.
#[cfg(test)]
mod contract;
mod error;
mod machine;
mod socket;
mod state;
mod wire;

pub use adapter::{Adapter, Instrumentation, RawEvent, SubagentSupport};
pub use adapters::GenericAdapter;
pub use error::AgentError;
pub use machine::{AgentEvent, AgentMachine, Declared, Exit, LINGER};
pub use socket::{listen, EventSink, EventSocket};
pub use state::AgentState;
pub use wire::{socket_path, EventFrame};
