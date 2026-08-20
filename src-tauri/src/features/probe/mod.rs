//! Les processus tels que le système les voit — et le seul endroit d'où Ash leur parle.
//!
//! ## Ce que la feature couvre, et pourquoi son nom est plus étroit qu'elle
//!
//! Elle s'appelle `probe` parce qu'elle a commencé par **lire** : le `cwd` d'un onglet et le
//! processus qui tient son avant-plan. Depuis la pause d'
//! [ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md), elle **écrit**
//! aussi — `SIGSTOP` et `SIGCONT` sur un groupe de processus ([`ProcessControl`]). Une sonde
//! qui envoie des signaux n'est plus tout à fait une sonde, et la question a été posée
//! plutôt que subie : la réponse est que le périmètre est « ce que le crate sait des
//! processus du système, et le seul endroit d'où il leur parle », pas « la lecture seule ».
//!
//! Le signal reste donc ici, et pas dans `pty`, pour deux raisons qui tiennent ensemble :
//!
//! - **la cible d'un signal est le groupe en avant-plan**, et c'est cette feature qui sait
//!   le nommer ([`Probe::foreground_pgid`]). Séparer celui qui désigne la cible de celui qui
//!   la vise mettrait entre eux une frontière que rien ne justifie ;
//! - **l'`unsafe` du crate est confiné dans [`macos`]** : le déplacer donnerait à `pty` son
//!   propre `libc` et un second site `unsafe`, ce que l'organisation du dépôt refuse.
//!
//! Ce que ce périmètre n'autorise **pas** pour autant : interpréter (aucun état d'agent ne
//! se déduit d'ici — [ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)) et détruire
//! (il n'y a pas de `kill` dans [`ProcessControl`], et il n'y en aura pas).
//!
//! ## La lecture
//!
//! Ash **sonde le système**, il ne demande rien au shell : aucun fichier de
//! configuration de l'utilisateur n'est touché, ni sur le disque ni dans l'environnement
//! du PTY ([ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md)). La contrepartie
//! est un `tcgetpgrp` + `proc_pidinfo` par onglet, et une réponse qui reste juste
//! *pendant* qu'un programme tourne — là où OSC 7 se tait.
//!
//! La feature est aussi le point de confinement de l'`unsafe` : tout appel système brut
//! du crate vit dans [`macos`], derrière des fonctions sûres.

mod control;
mod error;
#[cfg(target_os = "macos")]
mod macos;
mod port;
mod watch;

pub use control::ProcessControl;
pub use error::ProbeError;
#[cfg(target_os = "macos")]
pub use macos::SystemProbe;
pub use port::{Pid, Probe, ProcessInfo};
pub use watch::{Foreground, TabObservation, TabWatch};
