//! L'écriture chez l'utilisateur, et son seul propriétaire dans le code.
//!
//! C'est le premier endroit où Ash touche à un fichier qui ne lui appartient pas
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), spec §10). Toute la feature
//! existe pour que ce geste soit rare, visible et réversible :
//!
//! | La règle | Où elle vit |
//! |---|---|
//! | bloc délimité `ash:begin` / `ash:end`, versionné | [`block`] |
//! | rien n'est modifié hors marqueurs | [`block`] — le fichier est du texte, jamais un arbre relu, et le port n'accepte qu'un [`Document`] |
//! | `.bak` **avant** l'écriture, et jamais écrasé | [`install`] |
//! | refus d'écrire sur un bloc édité à la main, avec son diff | [`install`], [`diff`] |
//! | désinstallation qui ne laisse rien | [`install`] |
//!
//! **Un adaptateur n'écrit rien.** Il décrit ce qu'il veut voir écrit — une
//! [`Instrumentation`](crate::features::agents::Instrumentation) : un fichier, un contenu,
//! une version — et cette feature s'en charge. C'est ce qui fait qu'il n'y a qu'une façon
//! d'écrire chez l'utilisateur, donc une seule façon de se tromper.
//!
//! La feature ne sait rien des outils : ni le nom de leurs hooks, ni la forme de leur
//! configuration. Elle ne connaît que des marqueurs et des octets. Symétriquement, elle ne
//! lit **aucune** configuration d'Ash : quels dossiers instrumenter est une question de
//! `~/.ash/config.toml` (spec §9), que l'écran de réglages posera (#14, #16).
//!
//! **Les effets système de la feature**, avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `ConfigFiles` (`ports.rs`) | `SystemConfigFiles` (`system_files.rs`) | `FakeConfigFiles` (`fakes.rs`) |
//!
//! Rien n'est encore exposé au frontend : il n'y a pas d'écran de réglages, donc pas de
//! geste d'installation à offrir. Le `commands.rs` de cette feature naîtra avec lui (#16).

mod block;
mod diff;
mod error;
mod install;
mod ports;
mod system_files;

/// Le double du port `ConfigFiles`, réservé aux tests de la feature.
#[cfg(test)]
mod fakes;

pub use block::Document;
pub use error::HookError;
pub use install::{install, uninstall, Installation, Removal};
pub use ports::ConfigFiles;
pub use system_files::SystemConfigFiles;
