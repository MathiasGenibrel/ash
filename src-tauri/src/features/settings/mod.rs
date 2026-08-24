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
//! | `ToolStore` | `FileToolStore` — `~/.ash/tools.json` | `FakeToolStore` |
//! | `RunningTools` | le registre de PTY (composition root) | `FakeRunning` |
//!
//! **L'installation des hooks passe par le troisième**, et c'est ce qui fait que la feature
//! écrit chez l'utilisateur sans connaître un seul adaptateur ni un seul format de fichier
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)). Ce qu'elle décide, elle,
//! est **quand** : [`Verification::allows_hooks`] autorise, le doublon bloque la seconde
//! écriture, et [`hooks::report`] compose les deux avec ce que le fichier porte pour donner
//! l'un des cinq états de la ligne.
//!
//! **Deux valeurs portent les règles que la feature vérifiait puis oubliait** ([`values`]) :
//! [`Command`] est un nom de processus — ni espace, ni barre oblique, parce que la sonde
//! compare un nom (ADR-0006) — et [`ConfigTarget`] est le dossier qu'une entrée vise, sous
//! ses deux formes, celle qu'on affiche et celle qui touche le disque. Les deux ne se
//! fabriquent qu'en un endroit chacun, et c'est ce qui fait qu'une commande Tauri de plus ne
//! peut pas passer à côté : le doublon, la vérification, la recherche du bloc de hooks et la
//! mémoire du dernier dossier valide consomment tous la même valeur, donc ne peuvent plus
//! diverger sur ce que « le même dossier » veut dire.
//!
//! **« Retirer Ash de tous les fichiers » vit ici aussi** ([`withdrawal`]), et pour la même
//! raison que l'installation : c'est le registre qui sait *quels* fichiers, et le port
//! `HookBlocks` qui sait les lire et les reprendre. La feature ajoute les deux règles que ce
//! geste demande — un fichier n'est visé qu'une fois même si deux entrées le partagent, et
//! le retrait **n'est soumis à aucune vérification**, parce qu'il ne touche que ce qui porte
//! déjà le marqueur d'Ash (spec §10).
//!
//! **La section `notifications` de la fenêtre est ici aussi** ([`notifications`]), et pas
//! dans `agents` : cette feature-là n'expose rien au frontend et n'a pas de `commands.rs`,
//! quand celle-ci a déjà sa fenêtre, sa capacité et ses commandes. Elle ne décide pas ce
//! qu'Ash notifie — c'est `agents` qui le dit, par `SWITCHABLE_STATES` et par les trois
//! interrupteurs qu'il garde — elle décide **ce que l'écran en montre**, et notamment ce
//! qu'il dit quand macOS ne laisse rien savoir de son autorisation (spec §8). Le geste de
//! l'interrupteur traverse ici et repart aussitôt à `agents` : `settings` n'en garde rien.
//!
//! **Ce qu'Ash a vu tourner se déclare d'un clic** ([`suggestions`]) : sous les cartes, la
//! section `tools` propose les outils que la sonde a reconnus dans l'avant-plan d'un onglet
//! et que personne n'a déclarés (ADR-0006). C'est **le quatrième port** qui les apporte —
//! `settings` ne connaît pas `pty`, qui dépend déjà d'elle par sa reconnaissance — et rien
//! n'y est découvert : ni `PATH`, ni disque, ni autorisation. Un clic déclare, et ne pose
//! aucun hook.
//!
//! **Ce qui est déclaré survit au redémarrage** ([`store`], [`persisted`]) : les entrées
//! sont gardées dans `~/.ash/tools.json` — et non dans le `config.toml` que la spec §9
//! décrivait, corrigée depuis : les quatre magasins qui existaient déjà sont en JSON, et un
//! cinquième format aurait coûté une dépendance pour quatre champs.
//!
//! **Ce qui n'est pas gardé, et pourquoi :** le résultat des quatre tests, et l'état des
//! hooks. Une vérification est un fait daté sur la machine — un dossier peut avoir disparu
//! entre deux lancements — donc une entrée relue repart *non vérifiée* et se revérifie comme
//! une entrée saisie. Les hooks, eux, sont écrits sur le disque de l'utilisateur : ils ne se
//! déduisent pas d'un souvenir, mais du fichier, relu à chaque affichage (ADR-0007).

// `commands` est public pour la même raison que dans les autres features : les macros de
// `#[tauri::command]` ne survivent pas à un `pub use`.
pub mod commands;

mod error;
#[cfg(test)]
mod fakes;
mod hooks;
mod notifications;
mod permits;
mod persisted;
mod ports;
mod recognition;
mod registry;
mod store;
mod suggestions;
mod system;
mod tool;
mod usage;
mod values;
mod verification;
mod withdrawal;

pub use error::SettingsError;
pub use hooks::{BlockAt, HookAction, HookChoice, HookState, HooksReport};
pub use notifications::{
    NotificationPermission, NotificationSwitch, NotificationsReport, GRANT_PATH,
};
pub use ports::{CommandRunner, ConfigFiles, HookBlocks, RunningTools};
pub use recognition::{ToolRecognition, FRESHNESS};
pub use registry::ToolRegistry;
// `PersistedTools` — la forme du fichier — n'est **pas** réexportée, comme `Persisted` ne
// l'est pas par `features::sidebar` : rien hors de cette feature n'a à fabriquer ce qui sera
// écrit dans `~/.ash/tools.json`.
pub use store::{FileToolStore, ToolStore};
pub use suggestions::{ToolSuggestion, ToolSuggestions};
pub use system::{SystemCommands, SystemConfigFiles};
pub use tool::{NewTool, ToolDeclaration};
pub use usage::{UsageReport, KEYCHAIN_PATH};
pub use values::{Command, ConfigTarget};
pub use verification::{AdapterProfile, Verification, Verifier};
pub use withdrawal::{Outcome, PlannedRemoval, RemovalPlan, RemovalReport, RemovedFile};
