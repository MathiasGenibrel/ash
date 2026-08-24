//! L'écriture chez l'utilisateur, dans un `settings.json` — et son seul propriétaire.
//!
//! **Depuis #31, ce n'est plus le seul endroit du produit qui écrive chez l'utilisateur :**
//! [`crate::features::card`] écrit la fiche de branche (ADR-0013). Les deux features
//! appliquent la même règle — « Ash n'écrit que ce qui lui appartient, et sait le
//! reconnaître ; sauvegarde, jamais silencieux » — et **le régime n'est pourtant pas
//! partagé**, délibérément : la garantie ne tient pas dans l'ordre des gestes, elle tient
//! dans le type que le port accepte. Ici c'est un [`Document`], qui ne se compose que
//! d'entrées portant `# ash:hook v` ; là-bas un `CardDocument`, qui ne se compose que d'un
//! bloc remplacé, ajouté, ou d'un fichier neuf. Un port commun aurait pour seul dénominateur
//! un `write(path, &str)`, et les deux garanties redeviendraient de la prudence d'appelant.
//! Ce qui est partagé est ce qui ne porte la règle d'aucune des deux :
//! [`crate::shared::text_diff`].
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
//! | le diff de ce qu'Ash écrirait, montré avant toute écriture | [`presence`], [`crate::shared::text_diff`] |
//! | désinstallation qui ne laisse rien, à l'octet près | [`merge`], [`install`] |
//! | ce qu'un retrait emporterait, dit **avant** de le poser | [`removal`] |
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
//! `~/.ash/tools.json` (spec §9), que l'écran de réglages pose.
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

mod document;
mod error;
mod install;
mod json;
mod merge;
mod ports;
mod presence;
mod removal;
mod system_files;

/// Le double du port `ConfigFiles`, réservé aux tests — **de tout le crate**.
///
/// `pub(crate)` et non privé : la désinstallation globale est décidée par
/// `features::settings`, et le seul moyen d'y prouver « le fichier est rendu à l'octet
/// près » est de brancher son port sur la vraie écriture d'ici, au-dessus d'un disque en
/// mémoire. Un second double, écrit là-bas, ne doublerait pas la même chose.
#[cfg(test)]
pub(crate) mod fakes;

pub use document::Document;
pub use error::HookError;
pub use install::{install, uninstall, Installation, Removal};
pub use ports::ConfigFiles;
pub use presence::{inspect, Presence};
pub use removal::{foresee, Withdrawal};
pub use system_files::SystemConfigFiles;
