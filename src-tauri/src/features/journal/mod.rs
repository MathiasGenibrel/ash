//! L'attribution locale des commits
//! ([ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md)).
//!
//! C'est ce qu'aucun client git n'a : la colonne `by` du graphe, qui dit que `8f3a1c2` a été
//! écrit par `claude` et non par le nom d'auteur qu'on lit partout ailleurs. La donnée
//! n'existe nulle part — c'est Ash qui l'observe, à l'instant où le commit naît, et qui la
//! garde dans un **journal local append-only** sous `~/.ash/journal/<repo>.jsonl`
//! (spec §9.2). **Rien n'est écrit dans le dépôt de l'utilisateur** : ni `git notes`, ni
//! trailer, ni hook `prepare-commit-msg` — les trois sont examinés et écartés par l'ADR.
//!
//! ## Comment un commit arrive ici
//!
//! Par la **surveillance de fichiers** de `features/git`, et par elle seule : l'écriture de
//! `.git/logs/HEAD` d'un worktree suivi déclenche [`CommitJournal::on_head_moved`]. C'est la
//! lettre de l'ADR — « surveiller `.git/logs/HEAD` par dépôt, pas sonder `git log` » — et
//! c'est aussi ce qui évite un second abonnement FSEvents : les racines étaient déjà
//! surveillées, seul le filtre manquait.
//!
//! ## Les trois règles qui gouvernent l'écriture
//!
//! 1. **Ce qu'Ash n'a pas vu naître n'est pas attribué.** La borne est la date de démarrage :
//!    un `git checkout` fait bouger `HEAD` sans rien créer, et la lecture qui suit rend
//!    l'histoire entière de la branche.
//! 2. **Sans agent reconnu, rien n'est écrit.** Un `git commit` tapé à la main a déjà un nom
//!    d'auteur, et l'ADR est explicite : la colonne ne montre un agent que quand Ash l'a
//!    observé.
//! 3. **Un commit déjà connu n'est jamais réécrit**, même sous un `sha` neuf. C'est ce qui
//!    laisse un rebase garder l'attribution d'origine, au lieu de tout attribuer à l'agent
//!    qui a lancé le rebase.
//!
//! ## Deux champs sans source, et c'est la même
//!
//! ADR-0014 nomme huit champs. Six ont une source certaine, qui est la **sonde** — d'où le
//! fait que l'attribution marche pour tous les outils, `generic` compris
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)). Les deux autres,
//! `session_started` et `prompt`, n'en ont **aucune aujourd'hui**, et ce n'est pas la même
//! chose que « on ne sait pas les calculer » :
//!
//! - le **prompt** existe, mais sur l'entrée standard du hook `UserPromptSubmit`, que
//!   `ash-event` lit déjà pour en tirer `agent_id` / `agent_type` (ADR-0007, amendement du
//!   2026-08-13). Le faire remonter demande trois choses : que `ash-event` le lise, que la
//!   trame le transporte, et que le superviseur retienne le dernier prompt de chaque onglet.
//!   Rien d'impossible — mais c'est une tranche à part, qui touche le format du fil et qui
//!   engage une décision de confidentialité que personne n'a encore prise ;
//! - `session_started` a la même origine : aucun hook de démarrage de session n'est
//!   installé, et la date d'entrée dans un état n'est pas une date de session.
//!
//! Ils sont donc **facultatifs et vides**, jamais devinés. C'est le même parti que l'angle
//! mort documenté d'`agents/subagents.rs` : un trou nommé vaut mieux qu'une valeur
//! plausible. L'attribution, elle, ne dépend d'aucun des deux — et ADR-0014 demande justement
//! qu'elle ne dépende pas des hooks.
//!
//! **Les effets système de la feature**, chacun avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `JournalStore` (`store.rs`) | `FileJournalStore` (idem) | `MemoryJournal` (`fakes.rs`) |
//! | `Tabs` (`tabs.rs`) | `lib.rs`, sur le registre de PTY | `FakeTabs` (`fakes.rs`) |
//! | `CommitLog` (`commits.rs`) | `lib.rs`, sur `SystemGit` | `FakeCommits` (`fakes.rs`) |
//!
//! Les trois ports appartiennent à la feature, parce que c'est elle qui pose les trois
//! questions ; les trois adaptateurs du système sont posés par le composition root. Le
//! troisième mérite un mot : **il n'y a qu'un seul endroit du dépôt où le binaire `git` est
//! lancé**, et c'est `features/git/git_cli.rs`, avec la frontière de sécurité qui l'encadre.
//! Le journal n'en ouvre pas un second — il demande, et `lib.rs` branche la méthode qui
//! répond. Seul le vocabulaire d'un commit, [`CommitRecord`], vient de `features::git` : il
//! décrit ce que git dit, et la colonne `by` du graphe (#27) lira le même.

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod commits;
mod entry;
mod error;
#[cfg(test)]
mod fakes;
#[allow(clippy::module_inception)]
mod journal;
mod resolve;
mod store;
mod tabs;

pub use commits::{CommitLog, CommitRecord};
pub use entry::Entry;
pub use error::JournalError;
pub use journal::{CommitJournal, JournalSummary};
pub use store::{FileJournalStore, JournalStore};
pub use tabs::{TabAgent, Tabs};
