//! L'apparence de la fenêtre : son thème — clair, sombre, ou celui du système —, la taille
//! de police du terminal, et la colonne de gauche (sa largeur, son repli).
//!
//! Le module s'appelle `theme` et détient une `Appearance` : le nom est celui de sa
//! première préférence, pas celui de ce qu'il garde. Il ne se renomme pas au fil d'une
//! tâche — le fichier s'appelle `~/.ash/theme.json` et le contrat avec le frontend porte le
//! mot aussi.
//!
//! La feature ne peint rien — c'est le CSS qui peint, et xterm.js qui compose ses
//! cellules. Ce qu'elle détient, ce sont les **choix**, et elle les détient en Rust parce
//! que le frontend rend un état, il ne le garde pas
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le partage est net :
//! ici le mode, dans `src/app/theme.ts` la résolution de *système* en une palette — la
//! webview est seule à savoir de quelle humeur est macOS, et seule à l'apprendre à chaud
//! quand il change.
//!
//! **La taille de police est ici, et pas dans une feature `terminal`**, parce que c'est la
//! même nature de préférence que le thème, écrite dans le même fichier, relue au même
//! moment : `store.rs` avait été écrit pour ça — « le jour où une seconde préférence
//! d'apparence s'y ajoute, le fichier n'a pas à changer de forme ». Elle vaut pour
//! **toute l'application**, et non par onglet : voir [`FontSize`].
//!
//! **Les deux préférences ont désormais deux points d'entrée, et un seul détenteur** : le
//! menu natif (`src-tauri/src/menu.rs`) et la section `appearance` de la fenêtre de réglages
//! (spec §9). Les deux demandent la même chose de la même façon — un mode, un **pas** de
//! taille —, et les deux l'apprennent par les mêmes annonces : ni l'une ni l'autre ne retient
//! quoi que ce soit. Le choix de thème passe par `menu.rs` dans les deux cas, parce que la
//! coche du menu doit suivre un choix fait ailleurs et qu'une feature n'a pas à connaître la
//! forme d'un menu.
//!
//! **La colonne de gauche est ici aussi**, largeur et repli (`⌘B`), et pour la même raison
//! que la taille de police : c'est une préférence d'apparence (spec §9), elle s'écrit dans le
//! même fichier, elle se relit au même moment. Elle n'est **pas** dans `~/.ash/state.json`,
//! qui ne garde que les épingles et les lignes repliées — voir [`SidebarColumn`]. Ce qui n'est
//! pas ici, en revanche, ce sont ses **bornes** : de 10 % à 80 % de la fenêtre, elles
//! dépendent d'un viewport que seule la webview connaît, et vivent dans
//! `src/features/sidebar/resize.ts`.
//!
//! Ce qui reste à l'issue #22 : la police au choix, la densité, et l'aperçu du thème montrant
//! les cinq états d'agent — ce qui demande des planches de design, pas un état de plus.
//!
//! **L'effet système de la feature**, avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `ThemeStore` (`store.rs`) | `FileThemeStore` — `~/.ash/theme.json` | `FakeStore` (`state.rs`) |

// `commands` est public : `tauri::generate_handler!` a besoin des macros que
// `#[tauri::command]` génère à côté de chaque fonction, et un `pub use` ne les emporte pas.
pub mod commands;

mod appearance;
mod error;
mod font_size;
mod mode;
mod sidebar_column;
mod state;
mod store;

pub use error::ThemeError;
pub use font_size::{FontSize, FontStep};
pub use mode::ThemeMode;
pub use sidebar_column::{SidebarColumn, SidebarWidth};
pub use state::ThemeState;
pub use store::{FileThemeStore, ThemeStore};
