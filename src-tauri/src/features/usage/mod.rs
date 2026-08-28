//! Les quotas du compte : combien il en reste, et quand ça repart de zéro (spec §4.2).
//!
//! **C'est la seule feature du dépôt qui sorte sur le réseau**, et la seule qui lise un
//! secret. Deux ADR l'encadrent, et elles se lisent avant ce fichier :
//! [ADR-0016](../../../../docs/adr/0016-ash-sort-sur-le-reseau.md) — quatre conditions
//! cumulatives pour tout appel — et
//! [ADR-0017](../../../../docs/adr/0017-ash-lit-le-jeton-de-l-outil.md) — quatre conditions
//! pour le jeton.
//!
//! ## Pourquoi une feature, et pas un dossier de `agents/`
//!
//! `features/agents/` est le candidat évident : c'est le compte de l'outil qu'Ash
//! supervise. Il a été écarté pour trois raisons, dans cet ordre.
//!
//! - **Ce que `agents` détient est l'état d'un onglet.** Une machine par onglet, un socket
//!   qui reçoit les hooks d'un onglet, des lignes filles sous une ligne d'onglet. Un quota
//!   ne dépend d'aucun onglet, ne change pas quand on en sélectionne un autre, et n'a rien
//!   à faire d'une machine à états. Ce sont deux choses qui ne partagent aucune donnée.
//! - **La condition 4 d'ADR-0016 est une frontière, et une frontière se pose sur un
//!   dossier.** « Une feature qui n'a pas de raison d'appeler n'a aucun moyen de le faire » :
//!   mettre le seul client HTTP du dépôt dans la feature la plus grosse et la plus
//!   sollicitée reviendrait à l'offrir au superviseur, au socket et aux adaptateurs.
//! - **Rien ne se partagerait.** Aucun type, aucun port, aucun fichier de préférence n'est
//!   commun aux deux — la cohabitation n'achèterait que la proximité du mot « agent ».
//!
//! ## Les pièces, et la frontière entre elles
//!
//! | Module | Ce qu'il porte | Ce qu'il ne fait pas |
//! |---|---|---|
//! | [`token`] | la lecture du trousseau, et le refus définitif | il n'appelle rien |
//! | [`api`] | **la seule adresse réseau du dépôt**, et le `GET` | il ne décide pas quand |
//! | [`quota`] | la lecture défensive de la réponse | il ne connaît ni jeton ni socket |
//! | [`poller`] | le portillon : premier plan, interrupteur, une minute | il ne parle pas à Tauri |
//! | [`preferences`] | l'interrupteur, et le fichier qui s'en souvient | il ne lit personne |
//! | [`commands`] | une lecture et un event | il ne détient rien, et ne bascule rien |
//! | `rehearsal` | des doublures de décor, **absentes du binaire distribué** | il ne lit rien, n'appelle rien |
//!
//! L'interrupteur, lui, se bascule par la fenêtre de réglages (`features/settings/usage.rs`)
//! et par elle seule : une seule écriture, qui rend la section recomposée. Voir `commands.rs`.
//!
//! **Les effets système de la feature**, chacun avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | [`TokenSource`] (`token.rs`) | [`KeychainTokens`] | `FakeKeychain` (idem, et `poller.rs`) |
//! | [`UsageApi`] (`api.rs`) | [`AnthropicUsage`] | `FakeHost` (`poller.rs`) |
//! | [`UsageSink`] (`poller.rs`) | `UsageEvents` (`lib.rs`) | `FakeScreen` (`poller.rs`) |
//! | [`UsageStore`] (`preferences.rs`) | [`FileUsageStore`] — `~/.ash/usage.json` | `FakeStore` (idem) |
//!
//! Les deux premiers sont des traits pour une raison plus forte que d'habitude : **aucun
//! `cargo test` ne doit lire le trousseau de qui le lance**, faire apparaître un dialogue
//! d'autorisation macOS sur son écran, ni sortir sur le réseau.
//!
//! ## Ce que la feature ne fait pas, et ne fera pas
//!
//! **Elle ne notifie jamais.** Aucun seuil, aucune bannière, aucun `Notifier`. Une
//! interruption a un producteur unique dans ce produit — le superviseur d'`agents`, sur un
//! **changement** d'état d'agent (spec §8) — et un quota n'est pas un état d'agent. C'est
//! aussi un critère d'acceptation de l'issue #147.
//!
//! **Elle ne dit rien à personne d'autre que l'écran.** Aucune autre feature ne la connaît,
//! et aucune n'a de raison de la connaître : `pty` ne ralentit pas parce qu'un quota est
//! haut, et `agents` n'en déduit aucun état.

mod api;
/// Public comme les autres `commands.rs` du crate : `tauri::generate_handler!` a besoin
/// des modules d'assistance que la macro pose à côté de chaque commande, et un `pub use`
/// ne les emporte pas.
pub mod commands;
mod error;
mod poller;
mod preferences;
mod quota;
/// Les doublures que le build de développement peut brancher à la place du trousseau et de
/// l'hôte, quand une variable d'environnement le demande. **Ce module n'existe pas dans le
/// binaire de `bun run package`** : `debug_assertions` est éteint par `tauri build`, comme
/// il l'est pour tout ce qui sépare Ash d'Ash-dev. Lire son en-tête avant de le nommer.
#[cfg(debug_assertions)]
mod rehearsal;
mod token;

// **Ce qui n'est pas là est aussi une décision.** Le composition root a besoin d'assembler la
// feature et `settings` d'en rapporter deux faits ; rien d'autre n'a de raison de la nommer.
// `AccessToken` en particulier ne sort pas : hors de ce dossier, le type qui porte le secret
// n'est même pas nommable, et la condition 2 d'ADR-0017 n'a donc pas de porte à surveiller
// ailleurs. `UsageError`, `UsageChoices` et `MIN_INTERVAL` restent dedans pour la raison
// ordinaire — un `pub` posé au cas où est une frontière qui ne se referme plus.
pub use api::{AnthropicUsage, UsageApi, USAGE_ENDPOINT};
pub use commands::ACCOUNT_USAGE_EVENT;
pub use poller::{UsagePoller, UsageSink};
pub use preferences::{FileUsageStore, UsagePreferences, UsageStore};
pub use quota::{AccountUsage, Quota};
#[cfg(debug_assertions)]
pub use rehearsal::{Rehearsal, RehearsalError, REHEARSAL_VAR};
pub use token::{Credentials, KeychainTokens, Readability, TokenSource};
