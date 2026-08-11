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
//!
//! **Les effets système de la feature**, chacun avec ses deux adaptateurs — celui du
//! système, et celui des tests :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `EventSink` (`socket.rs`) | `HookEvents` (`lib.rs`) | `FakeSink` (`socket.rs`) |
//!
//! Le port n'est **pas** le socket : celui-ci est l'effet que la feature exerce elle-même,
//! et ses tests l'exercent pour de vrai. Ce que `EventSink` abstrait, c'est la **livraison**
//! — savoir qu'un onglet existe, et prévenir la webview. C'est ce qui laisse `agents` et
//! `pty` s'ignorer, et ce qui rend l'écoute vérifiable sans lancer un seul PTY.

mod adapter;
mod adapters;
// `commands` est public pour la même raison que dans les autres features : `commands.rs`
// *est* la surface publique de la feature vers le frontend, et les macros de
// `#[tauri::command]` ne survivent pas à un `pub use`. La feature n'expose encore aucune
// commande — à ce jalon, rien ne se demande, tout se pousse.
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
pub use adapters::{ClaudeCodeAdapter, GenericAdapter};
pub use error::AgentError;
pub use machine::{AgentEvent, AgentMachine, Declared, Exit, LINGER};
pub use socket::{listen, EventSink, EventSocket};
pub use state::AgentState;
pub use wire::{socket_path, EventFrame};
