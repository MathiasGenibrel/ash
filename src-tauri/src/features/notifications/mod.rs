//! Les bannières macOS : les poser, savoir si on en a le droit, et recevoir le clic.
//!
//! La feature ne connaît **ni agent, ni onglet, ni état** — c'est délibéré. Elle prend un
//! titre, un corps et une chaîne opaque, et rend cette chaîne quand l'utilisateur clique.
//! Ce que la chaîne désigne est décidé ailleurs : `features/agents` en fait un identifiant
//! d'onglet ([`crate::features::agents::Notice`]), le composition root en fait une
//! sélection. Sans cette ignorance, la seule couche `unsafe` du produit connaîtrait des
//! règles de produit, et il faudrait une application empaquetée pour les éprouver.
//!
//! Elle est posée comme `features/probe/` (ADR-0005), et pour la même raison : un effet
//! système propre à macOS, isolé derrière un trait que la feature possède, avec l'`unsafe`
//! dans un seul fichier — [`macos`] — et une frontière sûre autour.
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | [`Banners`] | [`SystemBanners`] | l'appelant en fournit un — aucun test du dépôt ne pose de vraie bannière |
//!
//! **Elle n'expose rien au frontend et n'a pas de `commands.rs`** : ce qu'un clic déclenche
//! traverse la frontière Tauri depuis le composition root, seul endroit qui sache que la
//! chaîne rendue est un onglet à sélectionner.
//!
//! Deux choses à savoir avant d'y toucher, toutes deux détaillées dans [`macos`] : la pile
//! est `UNUserNotificationCenter` et non les `NSUserNotification` de
//! `tauri-plugin-notification`, et **rien de tout ceci ne fonctionne hors d'une application
//! empaquetée** — donc pas en `bun run tauri dev`.

mod macos;
mod port;

pub use macos::SystemBanners;
pub use port::{Authorization, Banner, Banners, Clicked};
