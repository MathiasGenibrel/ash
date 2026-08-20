//! Ouvrir ce qu'un terminal a imprimé : une URL dans le navigateur, un chemin dans le
//! Finder (spec §4.2).
//!
//! **C'est la seule capacité d'ouverture du dépôt**, et la troisième frontière de sécurité
//! après l'appel à `git` (`features/git/git_cli.rs`) et la lecture du trousseau
//! (`features/usage/token.rs`). Deux fichiers se lisent avant d'y toucher :
//! [`target`] — ce qu'un mot a le droit de devenir — et [`opener`] — comment il est lancé.
//! La phrase à garder sous les yeux est dans le premier : **la sortie d'un PTY est du texte
//! hostile**.
//!
//! ## Pourquoi une feature, et pas un dossier de `pty` ou de `git`
//!
//! `features/pty/` est le candidat évident — c'est de sa sortie que viennent les mots — et
//! `features/git/` a déjà le seul lancement de binaire externe. Les deux ont été écartés,
//! pour trois raisons, dans cet ordre.
//!
//! - **Un point d'ouverture est une frontière, et une frontière se pose sur un dossier.**
//!   C'est l'argument de `features/usage/` (condition 4 d'ADR-0016 : « une feature qui n'a
//!   pas de raison d'appeler n'a aucun moyen de le faire »), et il vaut mot pour mot ici :
//!   mettre `open` dans la feature qui écrit dans les PTY reviendrait à l'offrir au
//!   registre, à la boucle de sonde et à la composition de prompts. Ici, la seule porte est
//!   [`Opener`], et il n'y a qu'un appelant.
//! - **Ce que `pty` détient est un descripteur, pas un sens.** La feature ne lit jamais ce
//!   qui traverse ses PTY — c'est même une règle du projet, les états d'agent viennent des
//!   hooks et « jamais d'une analyse de la sortie du PTY » ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
//!   Donner un sens à un mot imprimé serait exactement le geste qu'elle ne fait pas.
//! - **Rien ne se partagerait.** Aucun type, aucun port, aucun fichier de préférence n'est
//!   commun. La cohabitation n'achèterait que la proximité du mot « terminal ».
//!
//! ## Les pièces, et la frontière entre elles
//!
//! | Module | Ce qu'il porte | Ce qu'il ne fait pas |
//! |---|---|---|
//! | [`target`] | **la décision** : liste blanche, existence, résolution du relatif | il ne lance rien |
//! | [`opener`] | l'`argv`, et le seul lancement | il ne décide de rien : il ne sait lancer qu'un [`LinkTarget`] |
//! | [`files`] | la seule question posée au disque | il ne connaît ni schéma ni URL |
//! | [`commands`] | une question et une ouverture | il ne retient rien d'un survol à un clic |
//!
//! **Les effets système de la feature**, chacun avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | [`Files`] (`files.rs`) | [`SystemFiles`] | `FakeFiles` (idem) |
//! | [`Opener`] (`opener.rs`) | [`LaunchServices`] | `FakeOpener` (idem) |
//!
//! Le second est un trait pour une raison plus forte que d'habitude : **aucun `cargo test`
//! ne doit ouvrir le Finder ni lancer un navigateur** sur la machine de qui le lance.
//!
//! ## Ce que la feature ne fait pas, et ne fera pas
//!
//! - **Elle n'exécute rien.** Il n'existe aucune variante qui lance un fichier : un `.sh`,
//!   un `.app`, un binaire exécutable sont **révélés**, comme le reste.
//! - **Elle n'ouvre pas dans un éditeur.** Le geste décidé par l'issue #126 est « révéler
//!   dans le Finder », et un `code://` ou un `$EDITOR` serait une autre décision — qui
//!   rouvrirait, elle, la question de l'exécution.
//! - **Elle ne découpe pas les lignes.** Quels mots sont soumis est un fait d'affichage,
//!   qui vit dans `src/features/terminal/link-scan.ts` et n'a aucune autorité.

/// Public comme les autres `commands.rs` du crate : `tauri::generate_handler!` a besoin
/// des modules d'assistance que la macro pose à côté de chaque commande, et un `pub use`
/// ne les emporte pas.
pub mod commands;
mod error;
mod files;
mod opener;
mod target;

// **Ce qui n'est pas là est aussi une décision.** Le composition root a besoin d'assembler
// les deux ports ; rien d'autre n'a de raison de nommer cette feature. `resolve` en
// particulier ne sort pas : hors de ce dossier, la décision n'est même pas appelable, donc
// il n'y a pas de second chemin vers `Opener` à surveiller ailleurs.
pub use files::{Files, SystemFiles};
pub use opener::{LaunchServices, Opener};
pub use target::LinkTarget;
