//! Ash — bibliothèque.
//!
//! Tout le code vit ici plutôt que dans `main.rs` : c'est ce qui permet à
//! `cargo test` de le compiler sans lier l'exécutable, et ce qui laisse la porte
//! ouverte au démon `ashd` d'ADR-0009, qui réutiliserait la même bibliothèque sous un
//! autre binaire.
//!
//! Les features (`pty`, `probe`, `agents`, `git`, `journal`, `hooks`) apparaîtront ici
//! au fur et à mesure. Aucune n'est déclarée d'avance : un module vide ne documente
//! rien qu'`.claude/docs/architecture.md` ne dise déjà mieux.

/// Assemble et démarre l'application.
///
/// Composition root : c'est le seul endroit du crate où les implémentations concrètes
/// des effets système sont choisies et injectées.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default().run(tauri::generate_context!())
}
