//! Ash — bibliothèque.
//!
//! Tout le code vit ici plutôt que dans `main.rs` : c'est ce qui permet à
//! `cargo test` de le compiler sans lier l'exécutable, et ce qui laisse la porte
//! ouverte au démon `ashd` d'ADR-0009, qui réutiliserait la même bibliothèque sous un
//! autre binaire.

pub mod features;

/// Le menu applicatif : les raccourcis de la spec §4.4, et leur chemin souris.
mod menu;

/// Banc de mesure du spike xterm.js — jetable, retiré avec le spike.
pub mod spike;

use std::sync::Arc;

use features::pty::{PtyRegistry, SystemPtySpawner};

/// Assemble et démarre l'application.
///
/// Composition root : c'est le seul endroit du crate où les implémentations concrètes
/// des effets système sont choisies et injectées. `SystemPtySpawner` n'apparaît qu'ici ;
/// partout ailleurs la feature ne connaît que son trait.
pub fn run() -> tauri::Result<()> {
    let ptys = Arc::new(PtyRegistry::new(Box::new(SystemPtySpawner)));

    tauri::Builder::default()
        .manage(ptys)
        .manage(spike::Flow::default())
        .menu(menu::build)
        .on_menu_event(|app, event| menu::dispatch(app, event.id().as_ref()))
        .invoke_handler(tauri::generate_handler![
            features::pty::commands::pty_open,
            features::pty::commands::pty_write,
            features::pty::commands::pty_resize,
            features::pty::commands::pty_ack,
            features::pty::commands::pty_close,
            features::pty::commands::pty_tabs,
            features::pty::commands::pty_has_foreground_process,
            spike::spike_stream,
            spike::spike_ack,
            spike::spike_report
        ])
        .run(tauri::generate_context!())
}
