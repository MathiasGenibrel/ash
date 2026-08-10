use std::fmt;

use super::port::Pid;

/// Erreurs de la feature `probe`.
///
/// Un type par feature. Aucune de ces variantes ne remonte au frontend : sonder est un
/// service interne, et ses appelants décident eux-mêmes quoi montrer quand le système ne
/// répond pas — le registre de PTY, par exemple, rend le répertoire de départ de l'onglet.
#[derive(Debug, PartialEq, Eq)]
pub enum ProbeError {
    /// Le terminal ne désigne aucun groupe de processus en avant-plan.
    ///
    /// Arrive quand le descripteur n'est plus valide, ou quand le shell est mort sans que
    /// personne n'ait encore repris la main.
    NoForeground(std::os::fd::RawFd),
    /// Le processus n'existe plus, ou le noyau refuse d'en parler.
    ///
    /// C'est le cas nominal d'une course : entre `tcgetpgrp` et `proc_pidinfo`, la
    /// commande en avant-plan a eu le temps de se terminer.
    Vanished(Pid),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeError::NoForeground(fd) => {
                write!(f, "aucun processus en avant-plan sur le terminal {fd}")
            }
            ProbeError::Vanished(pid) => write!(f, "le processus {pid} n'est plus observable"),
        }
    }
}

impl std::error::Error for ProbeError {}
