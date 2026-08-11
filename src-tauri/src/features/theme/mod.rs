//! Le thème de la fenêtre : clair, sombre, ou celui du système.
//!
//! La feature ne peint rien — c'est le CSS qui peint. Ce qu'elle détient, c'est le
//! **choix**, et elle le détient en Rust parce que le frontend rend un état, il ne le
//! garde pas ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le
//! partage est net : ici le mode, dans `src/app/theme.ts` la résolution de *système* en
//! une palette — la webview est seule à savoir de quelle humeur est macOS, et seule à
//! l'apprendre à chaud quand il change.
//!
//! Le point d'entrée est le **menu natif** (`src-tauri/src/menu.rs`), et c'est délibéré :
//! la fenêtre de réglages est l'issue #14, son écran d'apparence l'issue #22.
//!
//! **L'effet système de la feature**, avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `ThemeStore` (`store.rs`) | `FileThemeStore` — `~/.ash/theme.json` | `FakeStore` (`state.rs`) |

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod error;
mod mode;
mod state;
mod store;

pub use error::ThemeError;
pub use mode::ThemeMode;
pub use state::ThemeState;
pub use store::{FileThemeStore, ThemeStore};
