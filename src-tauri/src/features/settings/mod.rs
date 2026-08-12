//! Les réglages : les commandes reconnues, et la fenêtre qui les montre.
//!
//! La feature possède **la liste des `[[command]]` de la spec §9** — ce qu'ADR-0006
//! appelle « les commandes reconnues », c'est-à-dire ce qui fait qu'un onglet devient un
//! agent. Elle ne possède ni la découverte, ni les hooks, ni la vérification : elle tient
//! la déclaration, et le reste s'y branche.
//!
//! Elle est en Rust, et pas dans un état de la webview, parce que ses lecteurs ne sont pas
//! dans la webview : la sonde compare un nom de processus à cette liste (ADR-0006), et
//! l'installation des hooks y lira le dossier de configuration
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)). La fenêtre de réglages n'est
//! qu'un de ses lecteurs, et le seul qui ait une surface
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! **Ce qui n'y est pas encore, et pourquoi :**
//!
//! - la **vérification** des quatre tests de la spec §9.1 (issue #15). [`ToolDeclaration`]
//!   porte déjà le `verified` qu'elle posera ;
//! - l'**écriture dans `~/.ash/config.toml`**, qui n'a lieu que pour une entrée vérifiée :
//!   sans vérification, aucune entrée ne peut l'atteindre, et un registre en mémoire dit
//!   donc exactement la vérité du produit ;
//! - l'**installation des hooks** (issue #16), qui lira `config` sur une entrée valide.
//!
//! La feature n'a pas de `ports.rs` : elle n'exerce aucun effet système. Le jour où elle
//! lira et écrira `config.toml`, elle en aura un — c'est le moment où le trait devra
//! exister, pas avant.

// `commands` est public pour la même raison que dans les autres features : les macros de
// `#[tauri::command]` ne survivent pas à un `pub use`.
pub mod commands;

mod error;
mod registry;
mod tool;

pub use error::SettingsError;
pub use registry::ToolRegistry;
pub use tool::{NewTool, ToolDeclaration};
