//! Ce qui survit à la fermeture de la fenêtre : les **worktrees épinglés**, et les **lignes
//! repliées** (spec §3.1, §5.2, §9.2).
//!
//! Deux faits, un fichier — `~/.ash/state.json` —, et **rien d'autre**. La règle de la spec
//! §3.1 est une règle de non-écriture autant que d'écriture : *Ash persiste ce que les agents
//! ont fait, jamais ce qu'ils étaient en train de faire.* Aucune session, aucun onglet, aucun
//! worktree courant, aucun état d'agent n'entre ici
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — et `store.rs` a un
//! test dont c'est le seul rôle : dire ce que le fichier contient, et donc ce qu'il ne
//! contient pas.
//!
//! **Pourquoi une feature, et pas un coin de `sidebar` côté TypeScript.** Un épinglage
//! survit à la fermeture : c'est, par définition, quelque chose que la webview ne peut pas
//! détenir. Le repli d'une ligne, lui, pourrait vivre dans la colonne — il y vivait — mais la
//! spec le fait survivre au redémarrage avec les épingles, donc il a la même adresse. La
//! colonne entière (`⌘B`), elle, **n'est pas ici** : elle ne se replie pas par ligne, elle ne
//! survit pas, et rien dans la spec ne le demande.
//!
//! **Ce que le disque garde est un chemin, jamais une fiche.** Le nom du worktree et son
//! dépôt se relisent — c'est le travail de `features::git` — et les recopier dans
//! `state.json` en ferait des copies périmées le jour d'un `git worktree move`. Le port
//! [`WorktreePlaces`] est ce qui relit, et c'est aussi lui qui répond de la conduite décidée
//! pour un dossier disparu : voir [`state`].
//!
//! **L'effet système de la feature**, avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `WorkspacesStore` (`store.rs`) | `FileWorkspacesStore` — `~/.ash/state.json` | `FakeStore` (`state.rs`) |
//! | `WorktreePlaces` (`places.rs`) | le composition root, sur `features::git` | `FakePlaces` (`state.rs`) |

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod error;
mod persisted;
mod places;
mod state;
mod store;

pub use error::WorkspacesError;
pub use persisted::Persisted;
pub use places::{PinnedRepo, PinnedWorktree, WorktreePlaces};
pub use state::{Workspaces, WorkspacesState};
pub use store::{FileWorkspacesStore, WorkspacesStore};
