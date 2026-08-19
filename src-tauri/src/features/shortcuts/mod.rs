//! Les raccourcis d'Ash : **la liste unique**, désormais réglable (spec §4.4, issue #22).
//!
//! Jusqu'ici, les accélérateurs étaient écrits en dur dans `descriptor()` de
//! `src-tauri/src/menu.rs`, et la fenêtre de réglages les **lisait** là — c'est le critère
//! de l'issue #110, et il existe parce que deux listes finissent toujours par diverger.
//! Rendre les raccourcis réglables ne l'a pas remis en cause : la liste a **changé de côté**
//! plutôt que de se dédoubler.
//!
//! ```text
//!            ┌──────────────────────────────┐
//!            │  features::shortcuts         │   ~/.ash/shortcuts.json
//!            │  Bindings — la liste unique  │◄──────────────┘
//!            └───────┬──────────────┬───────┘
//!         accelerator│              │report
//!                    ▼              ▼
//!            menu natif        section `shortcuts`
//!         (menu.rs, dérivé)    (la fenêtre de réglages)
//! ```
//!
//! Trois conséquences, qui sont autant de décisions :
//!
//! - **le menu natif se refait** quand une liaison change. `MenuItem::set_accelerator`
//!   existe pourtant dans `muda` 0.19.3, mais son implémentation macOS
//!   (`MenuChild::set_key_accelerator`) ne touche au `NSMenuItem` que si le nouvel
//!   accélérateur est `Some` : passer `None` met à jour le champ Rust et **laisse la touche
//!   sur l'entrée**. Un `⌫` ne retirerait donc rien du menu, et la touche continuerait de
//!   jouer l'action. `AppHandle::set_menu` — qui repose le menu applicatif entier sur le fil
//!   principal — est le seul chemin qui vaille pour les deux sens ;
//! - **chaque action a un défaut**, sinon `back to default`, `reset all` et le compteur
//!   `n changed` n'ont rien à comparer. Les défauts sont déclarés par `menu.rs`, qui possède
//!   les actions ; le fichier ne garde que les **écarts** ;
//! - **une combinaison réservée est annoncée, jamais interdite** (`reserved.rs`).
//!
//! Ce que cette feature ne sait pas : quelles actions Ash a, ce qu'elles font, et à quoi
//! ressemble un menu. Tout lui est donné à la construction, sous la forme d'une liste
//! d'[`ActionBinding`]. Ses commandes Tauri sont, elles, dans `menu.rs` — pour la même
//! raison que `theme_set_mode` y est : elles doivent refaire le menu, et une feature n'a pas
//! à connaître la forme d'un menu.

mod bindings;
mod combination;
mod error;
#[cfg(test)]
mod fakes;
mod reserved;
mod store;

pub use bindings::{
    ActionBinding, Bindings, CapturePreview, ConflictChoice, Listing, ShortcutConflict,
    ShortcutRow, ShortcutsReport,
};
pub use combination::{Combination, KeyStroke};
pub use error::ShortcutError;
#[cfg(test)]
pub use fakes::FakeBindingStore;
pub use reserved::{reservation, Reservation, ReservedBy};
pub use store::{BindingStore, FileBindingStore, StoredBindings};
