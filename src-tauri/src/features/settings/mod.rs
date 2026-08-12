//! Les réglages : les commandes reconnues, et la fenêtre qui les montre.
//!
//! La feature possède **la liste des `[[command]]` de la spec §9** — ce qu'ADR-0006
//! appelle « les commandes reconnues », c'est-à-dire ce qui fait qu'un onglet devient un
//! agent. Elle ne possède ni la découverte, ni les hooks, ni la vérification : elle tient
//! la déclaration, et le reste s'y branche.
//!
//! Elle est en Rust, et pas dans un état de la webview, parce que ses lecteurs ne sont pas
//! dans la webview : la sonde compare un nom de processus à cette liste (ADR-0006), et
//! l'installation des hooks y lira le dossier de configuration
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)). La fenêtre de réglages n'est
//! qu'un de ses lecteurs, et le seul qui ait une surface
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! **La vérification des quatre tests de la spec §9.1 vit ici** ([`verification`]), et
//! c'est ce qui donne à la feature ses effets système : lire un dossier, parcourir le
//! `PATH`, lancer une commande. Ils passent par les deux traits de [`ports`], que la
//! feature possède — sans eux, aucune de ses règles ne serait vérifiable sans un vrai
//! `~/.claude` sur la machine de celui qui lance `cargo test`.
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `ConfigFiles` | `SystemConfigFiles` | `FakeFolders` |
//! | `CommandRunner` | `SystemCommands` | `FakeCommands` |
//! | `HookBlocks` | `AdapterHooks` (composition root) | `FakeBlocks` |
//!
//! **L'installation des hooks passe par le troisième**, et c'est ce qui fait que la feature
//! écrit chez l'utilisateur sans connaître un seul adaptateur ni un seul format de fichier
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)). Ce qu'elle décide, elle,
//! est **quand** : [`Verification::allows_hooks`] autorise, le doublon bloque la seconde
//! écriture, et [`hooks::report`] compose les deux avec ce que le fichier porte pour donner
//! l'un des cinq états de la ligne.
//!
//! **Ce qui n'y est pas encore, et pourquoi :** l'**écriture dans `~/.ash/config.toml`**,
//! qui n'a lieu que pour une entrée vérifiée. La vérification l'a débloquée ; la persistance
//! appartient à la tâche qui la porte, et un registre en mémoire dit exactement la vérité du
//! produit d'ici là. Les hooks, eux, sont déjà écrits sur le disque de l'utilisateur : ils
//! ne se déduisent pas d'un souvenir, mais du fichier, relu à chaque affichage.

// `commands` est public pour la même raison que dans les autres features : les macros de
// `#[tauri::command]` ne survivent pas à un `pub use`.
pub mod commands;

mod error;
#[cfg(test)]
mod fakes;
mod hooks;
mod permits;
mod ports;
mod registry;
mod system;
mod tool;
mod verification;

pub use error::SettingsError;
pub use hooks::{BlockAt, HookAction, HookState, HooksReport};
pub use ports::{CommandRunner, ConfigFiles, HookBlocks};
pub use registry::ToolRegistry;
pub use system::{SystemCommands, SystemConfigFiles};
pub use tool::{NewTool, ToolDeclaration};
pub use verification::{AdapterProfile, Verification, Verifier};
