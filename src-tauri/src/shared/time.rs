//! Le temps, derrière deux traits.
//!
//! Lire l'heure et attendre sont des effets système, au même titre qu'un PTY ou qu'un
//! `libproc`. Les injecter est ce qui permet de prouver une règle qui parle de secondes
//! sans en dormir une seule — un test qui dort finit par être désactivé.
//!
//! Ces deux traits ont d'abord vécu dans `features/git/`, seule feature à limiter un
//! débit. La machine à états des agents (spec §6.4) a besoin de la même horloge pour ses
//! « 30 s », et une seconde déclaration du même trait ferait deux temps incompatibles dans
//! une application qui n'en a qu'un. Ils remontent donc ici : deux features les utilisent,
//! et ils ne portent la règle d'aucune des deux — les durées, elles, restent chez qui les
//! décide.

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
/// C'est ce qui rend une action **différée** possible sans boucle d'attente : rien ne
/// tourne tant qu'aucun report n'est en cours. Combien de temps, et pour quelle règle, se
/// décide chez l'appelant.
pub trait Scheduler: Send + Sync {
    fn after(&self, delay: Duration, action: Box<dyn FnOnce() + Send + 'static>);
}

/// Un fil par report, qui dort puis meurt.
///
/// Le compte de fils vaut donc le nombre de reports en cours, et il retombe à zéro dès
/// qu'ils sont échus : c'est à l'appelant de ne pas en empiler — celui qui limite un débit
/// n'a qu'un report à la fois, et le délai qu'il choisit est court.
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
