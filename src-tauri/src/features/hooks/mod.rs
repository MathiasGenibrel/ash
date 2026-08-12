//! L'écriture chez l'utilisateur, et son seul propriétaire dans le code.
//!
//! C'est le premier endroit où Ash touche à un fichier qui ne lui appartient pas
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), spec §10). Toute la feature
//! existe pour que ce geste soit rare, visible et réversible :
//!
//! | La règle | Où elle vit |
//! |---|---|
//! | un marqueur **par entrée**, versionné, qui cohabite avec les hooks de l'utilisateur | [`merge`], [`document`] |
//! | un seul classement de l'état d'un fichier, pour agir **et** pour l'afficher | [`presence`] |
//! | rien n'est modifié hors de ce qui est à Ash | [`document`] — le fichier est du texte, jamais un arbre relu, et le port n'accepte qu'un [`Document`] |
//! | `.bak` **avant** l'écriture, et jamais écrasé | [`install`] |
//! | le diff de ce qu'Ash écrirait, montré avant toute écriture | [`presence`], [`diff`] |
//! | désinstallation qui ne laisse rien, à l'octet près | [`merge`], [`install`] |
//!
//! **Le bloc délimité `ash:begin` / `ash:end` a disparu le 2026-08-12**
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), amendement). Il savait poser
//! une région entière ; il ne savait pas cohabiter, et un `settings.json` portant déjà un
//! hook rendait la fonction centrale du produit inatteignable. La garantie a été
//! **reformulée, pas retirée** : Ash n'écrit que ce qui porte son marqueur, et sait le
//! reconnaître. Voir [`document`], qui la porte dans ses types.
//!
//! **Un adaptateur n'écrit rien.** Il décrit ce qu'il veut voir écrit — une
//! [`Instrumentation`](crate::features::agents::Instrumentation) : un fichier, des entrées
//! qui nomment chacune le chemin de clés où elle va, une version — et cette feature s'en
//! charge. C'est ce qui fait qu'il n'y a qu'une façon d'écrire chez l'utilisateur, donc une
//! seule façon de se tromper.
//!
//! La feature ne sait rien des outils : ni le nom de leurs hooks, ni la forme de leur
//! configuration. Elle sait descendre un chemin de clés dans du JSON, reconnaître ses
//! propres entrées, et compter celles des autres. Symétriquement, elle ne
//! lit **aucune** configuration d'Ash : quels dossiers instrumenter est une question de
//! `~/.ash/config.toml` (spec §9), que l'écran de réglages posera (#14, #16).
//!
//! **Les effets système de la feature**, avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `ConfigFiles` (`ports.rs`) | `SystemConfigFiles` (`system_files.rs`) | `FakeConfigFiles` (`fakes.rs`) |
//!
//! **Cette feature n'a pas de `commands.rs`, et n'en aura pas** : l'écran de réglages est le
//! seul geste d'installation du produit (#16), et il appelle `settings`, qui sait de quel
//! outil il s'agit et si son entrée a prouvé son dossier. Exposer `install` directement au
//! frontend ouvrirait un second chemin vers l'écriture — celui qui contourne la garde.
//! `settings` traverse cette frontière par le port `HookBlocks`, que la composition root
//! relie ici en traduisant un identifiant d'adaptateur en instrumentation.

mod diff;
mod document;
mod error;
mod install;
mod json;
mod merge;
mod ports;
mod presence;
mod system_files;

/// Le double du port `ConfigFiles`, réservé aux tests de la feature.
#[cfg(test)]
mod fakes;

pub use document::Document;
pub use error::HookError;
pub use install::{install, uninstall, Installation, Removal};
pub use ports::ConfigFiles;
pub use presence::{inspect, Presence};
pub use system_files::SystemConfigFiles;
