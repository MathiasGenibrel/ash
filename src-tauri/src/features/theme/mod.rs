//! L'apparence de la fenêtre : son thème — clair, sombre, ou celui du système — et la
//! taille de police du terminal.
//!
//! La feature ne peint rien — c'est le CSS qui peint, et xterm.js qui compose ses
//! cellules. Ce qu'elle détient, ce sont les **choix**, et elle les détient en Rust parce
//! que le frontend rend un état, il ne le garde pas
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le partage est net :
//! ici le mode, dans `src/app/theme.ts` la résolution de *système* en une palette — la
//! webview est seule à savoir de quelle humeur est macOS, et seule à l'apprendre à chaud
//! quand il change.
//!
//! **La taille de police est ici, et pas dans une feature `terminal`**, parce que c'est la
//! même nature de préférence que le thème, écrite dans le même fichier, relue au même
//! moment : `store.rs` avait été écrit pour ça — « le jour où une seconde préférence
//! d'apparence s'y ajoute, le fichier n'a pas à changer de forme ». Elle vaut pour
//! **toute l'application**, et non par onglet : voir [`FontSize`].
//!
//! Le point d'entrée est le **menu natif** (`src-tauri/src/menu.rs`), et c'est délibéré :
//! la fenêtre de réglages existe désormais, mais sa section `appearance` est l'issue #22 —
//! elle ne fait qu'y renvoyer.
//!
//! **L'effet système de la feature**, avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `ThemeStore` (`store.rs`) | `FileThemeStore` — `~/.ash/theme.json` | `FakeStore` (`state.rs`) |

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod appearance;
mod error;
mod font_size;
mod mode;
mod state;
mod store;

pub use error::ThemeError;
pub use font_size::{FontSize, FontStep};
pub use mode::ThemeMode;
pub use state::ThemeState;
pub use store::{FileThemeStore, ThemeStore};
