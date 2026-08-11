//! Le temps, derrière deux traits que la feature possède.
//!
//! Lire l'heure et attendre sont des effets système, au même titre qu'un PTY ou qu'un
//! `libproc`. Les injecter est ce qui permet de vérifier « au plus un rafraîchissement
//! toutes les 5 s » sans dormir cinq secondes — un test qui dort finit par être désactivé.

use std::time::{Duration, Instant};

/// L'instant courant.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

/// L'horloge monotone du système.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Exécuter une action plus tard.
///
/// C'est ce qui rend un rafraîchissement **différé** possible sans boucle d'attente : rien
/// ne tourne tant qu'aucun report n'est en cours, et c'est exactement ce que demande le
/// critère « avec 5 dépôts ouverts, la consommation CPU au repos reste négligeable ».
pub trait Scheduler: Send + Sync {
    fn after(&self, delay: Duration, action: Box<dyn FnOnce() + Send + 'static>);
}

/// Un fil par report, qui dort puis meurt.
///
/// Un report ne dure jamais plus que la fenêtre de limitation (5 s), et il n'y en a qu'un
/// à la fois par worktree : le compte de fils est borné par le nombre de dépôts ouverts,
/// et il retombe à zéro dès que le disque se tait.
#[derive(Debug, Default, Clone, Copy)]
pub struct ThreadScheduler;

impl Scheduler for ThreadScheduler {
    fn after(&self, delay: Duration, action: Box<dyn FnOnce() + Send + 'static>) {
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            action();
        });
    }
}
