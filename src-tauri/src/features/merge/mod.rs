//! L'onglet de merge — **le premier onglet sans PTY** (spec §7.4, issue #30).
//!
//! Trois panneaux, hunk par hunk, panneau central éditable, `continue` éteint tant qu'il
//! reste un conflit. Et surtout : les côtés portent le **nom de leur branche**, jamais le
//! `ours`/`theirs` de git, qui s'inverse en rebase — c'est [`sides`] qui porte cette règle,
//! et c'est elle que la suite d'intégration `merge_real_repository` vérifie sur un vrai
//! dépôt, dans les deux sens.
//!
//! # Pourquoi une feature, et pas un genre d'onglet dans `pty`
//!
//! [ADR-0003](../../../../docs/adr/0003-zone-terminal-unique.md) demande que le type de
//! l'onglet soit une somme : `Shell | Merge`. Il restait à choisir *où* la somme vit.
//!
//! `features::pty::registry` tient des onglets qui **sont** des PTY : un maître, un lecteur
//! avec ses crédits, une grille, une sonde de 300 ms, un pupitre de composition, un groupe
//! de processus qu'on peut arrêter. Un onglet de merge n'a aucun des sept. Faire de `kind`
//! une somme *dans* ce registre aurait ajouté à chacune de ses méthodes une question — « et
//! si ce n'était pas un shell ? » — dont la mauvaise réponse est silencieuse : un `resize`
//! qui ne trouve pas de PTY, une sonde qui cherche un processus en avant-plan là où il n'y
//! en a pas, une pause qui poste un signal à personne.
//!
//! Ici, la somme est **structurelle** : un onglet de merge n'entre jamais dans le registre
//! de PTY, donc aucune de ses méthodes ne peut le voir. `pty_write` sur un onglet de merge
//! rend `PtyError::UnknownTab` — un refus nommé, pas une panique. Ce que l'hypothèse
//! « un onglet = un PTY » avait de vrai dans `pty` le reste entièrement.
//!
//! Ce que ça coûte, et il faut le dire : la **réunion** des deux listes n'appartient à
//! aucune des deux features. Elle est dans `src-tauri/src/tabs.rs`, au composition root —
//! le seul endroit qui a le droit de connaître les deux —, et c'est là qu'est écrit l'ordre
//! que `⌘1..9` numérote.
//!
//! # Ce que cette feature ne détient pas
//!
//! Aucun état de résolution. Un onglet, c'est un identifiant et une racine de worktree ;
//! tout le reste est relu dans le worktree et dans l'index à chaque appel. C'est la
//! traduction directe du critère « fermer l'onglet ne perd rien : l'état vit dans l'index
//! git, pas dans Ash » — il n'y a rien à perdre.

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction.
pub mod commands;

mod conflict;
mod error;
mod ports;
mod sides;
mod surface;
mod tabs;

#[cfg(test)]
mod fakes;

pub use conflict::{ConflictFile, Hunk};
pub use error::MergeError;
pub use ports::{ConflictFiles, MergeOutcome, StoppedWorktree, TreeGit};
pub use sides::{MergeSides, SideLabel};
pub use surface::{MergeSurface, MergeView, StoppedView};
pub use tabs::{MergeTabInfo, TabId};
