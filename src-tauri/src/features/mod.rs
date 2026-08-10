//! Les features du backend.
//!
//! Chacune n'expose au frontend que ses `#[tauri::command]`, déclarés dans son
//! `commands.rs`. Voir `.claude/docs/architecture.md`.

pub mod probe;
pub mod pty;
