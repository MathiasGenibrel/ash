//! Sonde système : le `cwd` d'un onglet et le processus qui tient son avant-plan.
//!
//! Ash **sonde le système**, il ne demande rien au shell : aucun fichier de
//! configuration de l'utilisateur n'est touché, ni sur le disque ni dans l'environnement
//! du PTY ([ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md)). La contrepartie
//! est un `tcgetpgrp` + `proc_pidinfo` par onglet, et une réponse qui reste juste
//! *pendant* qu'un programme tourne — là où OSC 7 se tait.
//!
//! La feature est aussi le point de confinement de l'`unsafe` : tout appel système brut
//! du crate vit dans [`macos`], derrière des fonctions sûres.

mod error;
#[cfg(target_os = "macos")]
mod macos;
mod port;
mod watch;

pub use error::ProbeError;
#[cfg(target_os = "macos")]
pub use macos::SystemProbe;
pub use port::{Pid, Probe, ProcessInfo};
pub use watch::{Foreground, TabObservation, TabWatch};
