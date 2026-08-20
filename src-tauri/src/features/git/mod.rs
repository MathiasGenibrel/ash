//! Git, domaine de premier plan ([ADR-0011](../../../../docs/adr/0011-git-domaine-de-premier-plan.md)).
//!
//! La feature apporte trois choses, et chacune s'appuie sur la précédente :
//!
//! - la **résolution** d'un `cwd` vers son worktree et son dépôt commun — la brique dont
//!   dépend la hiérarchie à trois niveaux d'
//!   [ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md) ;
//! - les **métadonnées** d'un worktree — branche et opération en cours — tenues à jour par
//!   **surveillance de fichiers**, jamais par sondage (spec §5.3) ;
//! - ce qu'une opération **arrêtée** dit d'elle-même ([`stopped`]), et le **texte** qu'on
//!   en tire pour l'agent ([`prompt`], spec §7.4).
//!
//! # Pourquoi la rédaction du prompt vit ici
//!
//! Le prompt de conflit parle à un agent, et on pourrait croire sa place dans
//! `features/agents` ou dans `features/pty`. Elle est ici, et pour une raison qui se
//! vérifie : son entrée est **entièrement** de l'état git — l'opération, les chemins en
//! conflit, le commit d'arrêt —, et il ne nomme aucun outil, aucun onglet, aucun PTY. Le
//! sortir d'ici obligerait à faire voyager tout cet état vers une autre feature pour n'en
//! rapporter qu'une chaîne.
//!
//! Ce qui ne vit **pas** ici, et c'est la ligne de partage : le droit d'écrire ce texte
//! quelque part. Les trois conditions d'
//! [ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md) — prompt vide,
//! outil reconnu en avant-plan, fin du tour — sont arbitrées par
//! `features::pty::compose`, qui seul tient les onglets. `git` rédige, `pty` décide où et
//! quand, l'utilisateur envoie. Aucune des deux ne sait faire le travail de l'autre.
//!
//! [`test_command`] suit la même règle : sa question — « quelle commande teste ce
//! worktree ? » — porte sur un **worktree**, et sa réponse se lit avec le port
//! [`FileSystem`] que cette feature possède déjà. Ailleurs, elle demanderait un second
//! port et un second type d'erreur pour une seule fonction.
//!
//! La résolution et la lecture des fichiers de contrôle n'invoquent **jamais** le binaire
//! `git` : tout se lit derrière le trait [`FileSystem`]. Le seul appel à `git` de tout le
//! dépôt est celui de [`git_cli`], pour l'état de l'arbre et l'avance sur l'amont, que
//! rien dans `.git` ne porte. Il est déclenché par la surveillance et par elle seule —
//! jamais par la boucle de sonde, ce que l'ADR-0011 exclut explicitement.
//!
//! **Les effets système de la feature**, chacun avec ses deux adaptateurs — celui du
//! système, et celui des tests :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `FileSystem` (`ports.rs`) | `system_fs.rs` | `fake_fs.rs`, `fakes.rs` |
//! | `FileWatcher` (`watcher.rs`) | `watcher.rs` | `fakes.rs` |
//! | `Clock`, `Scheduler` (`shared/time.rs`) | `shared/time.rs` | `fakes.rs` |
//! | `StatusReader` (`git_cli.rs`) | `git_cli.rs` | `fakes.rs` |
//! | `BranchReader`, `TreeWriter` (`git_cli.rs`) | `git_cli.rs` | `branch_actions.rs` |
//! | `WorkingAgents` (`working_agents.rs`) | `lib.rs` | `branches.rs` |
//!
//! Le dernier n'est pas un effet système : c'est un **fait** que `git` ne peut pas
//! connaître — quel agent écrit dans ce worktree. Il est un port pour la même raison que
//! `pty::AgentStates` : sans lui, `git` importerait `pty`, et l'avertissement de la spec
//! §7.1 ne se vérifierait qu'en ouvrant un PTY.

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod branch_actions;
mod branches;
mod control;
mod error;
mod git_cli;
mod metadata;
mod metadata_watch;
mod porcelain;
mod ports;
mod prompt;
mod stopped;
mod system_fs;
mod targets;
mod test_command;
mod throttle;
mod watcher;
mod working_agents;
mod worktree;

/// L'arbre en mémoire qui double le port `FileSystem` dans les tests de la feature.
#[cfg(test)]
mod fake_fs;

/// Les doubles des autres effets système : surveillance, horloge, reports.
#[cfg(test)]
mod fakes;

pub use branch_actions::{ActionOffer, ActionOutcome, BranchAction};
pub use branches::{
    overview as branch_overview, Branch, BranchGroup, BranchKind, BranchOverview, BranchSection,
    BranchWorktree,
};
pub use error::GitError;
pub use git_cli::{
    BranchReader, CommitRecord, StatusReader, SystemGit, TreeWriter, STATUS_TIMEOUT,
};
pub use metadata::{
    read_metadata, Head, Operation, OperationKind, Progress, Status, TreeStatus, Upstream,
    WorktreeMetadata,
};
pub use metadata_watch::MetadataWatch;
pub use porcelain::parse_status;
pub use ports::{Entry, FileSystem};
pub use prompt::{compose_conflict_prompt, PromptSubject};
pub use stopped::{read_stopped, StoppedCommit, StoppedOperation};
pub use system_fs::SystemFileSystem;
pub use test_command::detect_test_command;
pub use working_agents::{at_risk, BusyAgent, WorkingAgents};
pub use worktree::{resolve_worktree, Repo, Worktree, WorktreeLocation};
