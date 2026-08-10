//! PTY et cycle de vie des onglets shell.
//!
//! Un onglet porte **au plus un PTY** ([ADR-0003](../../../../docs/adr/0003-zone-terminal-unique.md)).
//! L'état des onglets vit ici, pas dans la webview
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : le frontend
//! rend ce que le registre détient.

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte
// pas. C'est aussi cohérent avec l'architecture — `commands.rs` *est* la surface
// publique de la feature vers le frontend.
pub mod commands;
mod decode;
mod error;
mod flow;
mod registry;
mod session;

pub use error::PtyError;
pub use registry::PtyRegistry;
pub use registry::TabId;
pub use registry::TabInfo;
pub use session::{PtySession, PtySpawner, PtySpec, SystemPtySpawner};
