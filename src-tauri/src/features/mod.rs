//! Les features du backend.
//!
//! Chacune n'expose au frontend que ses `#[tauri::command]`, déclarés dans son
//! `commands.rs`. Voir `.claude/docs/architecture.md`.

pub mod agents;
pub mod card;
pub mod git;
pub mod hooks;
pub mod journal;
pub mod links;
pub mod merge;
pub mod notifications;
pub mod probe;
pub mod pty;
pub mod settings;
pub mod shortcuts;
pub mod sidebar;
pub mod theme;
pub mod usage;
