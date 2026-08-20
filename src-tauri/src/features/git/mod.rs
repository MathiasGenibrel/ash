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
//!   en tire pour l'agent ([`prompt`], spec §7.4) ;
//! - le **graphe de commits** et sa colonne `by` ([`graph`], [`history`], spec §7.2) : les
//!   couloirs sont une fonction pure, et le nom de l'agent vient du journal d'ADR-0014 par le
//!   port [`Attributions`], que cette feature possède parce que c'est elle qui pose la
//!   question.
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
//! | `GraphLog` (`git_cli.rs`) | `git_cli.rs` | `history.rs` |
//! | `Attributions` (`attribution.rs`) | `lib.rs`, sur `CommitJournal` | `history.rs` |

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod attribution;
mod control;
mod error;
mod git_cli;
mod graph;
mod history;
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
mod worktree;

/// L'arbre en mémoire qui double le port `FileSystem` dans les tests de la feature.
#[cfg(test)]
mod fake_fs;

/// Les doubles des autres effets système : surveillance, horloge, reports.
#[cfg(test)]
mod fakes;

pub use attribution::{Attribution, Attributions};
pub use error::GitError;
pub use git_cli::{CommitRecord, GraphLog, StatusReader, SystemGit, STATUS_TIMEOUT};
// Du graphe, l'extérieur ne voit que **le chemin de production** — [`CommitGraphReader`], que
// `lib.rs` assemble — et de quoi refaire un dessin sur un vrai dépôt, ce que
// `tests/commit_graph_real_repository.rs` est seul à faire : un test d'intégration ne peut
// atteindre que l'API publique, et la chaîne qu'il vérifie (le processus `git`, la lecture de
// sa sortie, les couloirs) n'a pas d'autre porte.
//
// Ce qui n'est **pas** exporté ne manque à personne, et c'est voulu : `MAX_LANES`,
// `INACTIVE_AFTER`, `DEFAULT_WINDOW`, `MAX_GRAPH_WINDOW` sont des choix de produit que cette
// feature applique elle-même, et `CommitGraph` / `CommitRow` sont la forme de la réponse d'une
// commande Tauri — l'écran les connaît par le contrat, jamais une autre feature Rust. Publier
// une seconde porte vers les couloirs reviendrait à laisser une autre feature les recalculer,
// c'est-à-dire à rouvrir ce qu'ADR-0009 ferme.
//
// `graph::FoldedBranch` reste privée : `history` en expose une jumelle sérialisable, et deux
// types du même nom dans la même API publique n'apprendraient rien à personne.
pub use graph::{lay_out, GraphCommit, Layout, Link};
pub use history::CommitGraphReader;
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
pub use worktree::{resolve_worktree, Repo, Worktree, WorktreeLocation};
