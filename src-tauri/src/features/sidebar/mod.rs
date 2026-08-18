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
//! **Pourquoi une feature backend, et pas un coin de `src/features/sidebar`.** Un épinglage
//! survit à la fermeture : c'est, par définition, quelque chose que la webview ne peut pas
//! détenir. Le repli d'une ligne, lui, pourrait vivre dans la colonne — il y vivait — mais la
//! spec le fait survivre au redémarrage avec les épingles, donc il a la même adresse. La
//! colonne entière (`⌘B`), elle, **n'est pas ici** : elle ne se replie pas par ligne, elle ne
//! survit pas, et rien dans la spec ne le demande.
//!
//! **Et pourquoi elle s'appelle `sidebar`.** Parce que c'est la moitié backend de la colonne,
//! et parce que le mot qui venait d'abord à l'esprit — « workspace » — a été retiré du
//! vocabulaire par [ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md) : il y
//! désigne désormais un **worktree**. Une feature nommée d'après lui aurait promis de détenir
//! les worktrees, qui sont à `features::git`, alors qu'elle ne détient que deux faits sur
//! leurs lignes. Voir `features::git::WorktreeLocation`, qui écarte le même mot pour la même
//! raison.
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
//! | `SidebarStore` (`store.rs`) | `FileSidebarStore` — `~/.ash/state.json` | `FakeStore` (`state.rs`) |
//! | `WorktreePlaces` (`places.rs`) | le composition root, sur `features::git` | `FakePlaces` (`state.rs`) |

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod error;
mod persisted;
mod places;
mod state;
mod store;

pub use error::SidebarError;
pub use persisted::Persisted;
pub use places::{PinnedRepo, PinnedWorktree, WorktreePlaces};
pub use state::{SidebarRows, SidebarState};
pub use store::{FileSidebarStore, SidebarStore};
