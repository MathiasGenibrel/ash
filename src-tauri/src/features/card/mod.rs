//! La fiche de branche — `.ash/worktree.md`
//! ([ADR-0013](../../../../docs/adr/0013-fiche-de-branche-dans-le-depot.md), spec §7.5).
//!
//! **C'est le deuxième endroit où Ash écrit chez l'utilisateur, et le premier dans son
//! dépôt.** Cette phrase est toute la feature : ce qui compte n'est pas ce que la fiche
//! affiche, c'est *comment* Ash y écrit. Le régime est celui des `settings.json`
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)), et il tient en cinq lignes :
//!
//! | La règle | Où elle vit |
//! |---|---|
//! | une **seule zone**, `<!-- ash:log -->` … `<!-- /ash:log -->`, reconnue même dans un fichier qui la cite | [`block`] |
//! | rien n'est modifié hors de cette zone | [`document`] — le port n'accepte qu'un [`CardDocument`], qui ne se compose que de trois façons |
//! | `.bak` **avant** l'écriture, et jamais écrasé | [`write`] |
//! | **refus** si le bloc n'est pas celui qu'Ash y a laissé, avec le diff de ce qui changerait | [`write`], [`log::is_ours`] |
//! | un **conflit git** dans la zone n'est jamais résolu par Ash | [`block::carries_a_conflict`], [`write`] |
//!
//! ## Pourquoi une feature, et pas `features/git`
//!
//! La fiche parle d'un **worktree**, pas de git : elle n'appelle pas `git`, ne lit aucun
//! fichier de contrôle, et n'a pas d'opinion sur une branche. Ce qu'elle porte de sérieux
//! est un régime d'**écriture chez l'utilisateur** — le sujet de `features/hooks`, pas celui
//! de `features/git`, qui est déjà la plus grosse du dépôt et dont le mandat est la lecture.
//! Les deux features qui savent écrire chez quelqu'un d'autre sont donc voisines et
//! symétriques, et la seule chose qu'elles partagent est ce qui ne porte la règle d'aucune :
//! [`crate::shared::text_diff`].
//!
//! ## Ce que la fiche ne fait pas
//!
//! - **Elle ne commite rien.** Ash écrit le fichier ; l'utilisateur ou l'agent le commite.
//!   Il n'y a aucun `git add` dans cette feature, et il n'y en aura pas.
//! - **Elle n'écrit jamais dans un `.gitignore`.** ADR-0013 l'interdit en toutes lettres.
//!   Le `.gitignore` est **lu** ([`place`]) pour deviner ce que l'équipe veut, et c'est tout.
//! - **Elle ne déplace aucun fichier** quand le mode change : elle change où Ash regarde.
//!
//! **Les effets système de la feature**, chacun avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `CardFiles` (`ports.rs`) | `SystemCardFiles` (`system_files.rs`) | `MemoryCardFiles` (`fakes.rs`) |
//! | `ModeStore` (`modes.rs`) | `FileModeStore` (idem) | `MemoryModes` (`fakes.rs`) |
//! | `AgentWork` (`ports.rs`) | `lib.rs`, sur `CommitJournal` + `SystemGit` | `FakeWork` (`fakes.rs`) |
//! | `Clock` (`shared/time.rs`) | `SystemClock` | `FrozenClock` (`card.rs`) |
//!
//! Le troisième mérite un mot : la feature ne connaît **ni le journal, ni git**. Elle demande
//! « qui a écrit quoi dans ce worktree », et le composition root relie la question à
//! `features::journal` — qui seul garde l'attribution — et à `features::git`, seul endroit du
//! dépôt où le binaire `git` est lancé. C'est la même forme que `journal` avec `CommitLog`.

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod block;
#[allow(clippy::module_inception)]
mod card;
mod document;
mod error;
mod log;
mod modes;
mod place;
mod ports;
mod system_files;
mod write;

/// Les doubles des effets système de la feature.
#[cfg(test)]
mod fakes;

pub use card::{BranchCard, Cards};
pub use document::CardDocument;
pub use error::CardError;
pub use modes::{FileModeStore, ModeStore};
pub use place::CardMode;
pub use ports::{AgentWork, CardFiles, WorkRecord};
pub use system_files::SystemCardFiles;
pub use write::{LogState, LogWrite};
