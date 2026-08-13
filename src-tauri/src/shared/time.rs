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

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Une date **absolue**, en millisecondes depuis l'époque Unix.
///
/// C'est la seule forme du temps qui a le droit de traverser la frontière Tauri : un
/// [`Instant`] mesure un écart mais ne dit pas l'heure, et ne se sérialise pas — c'est
/// exactement l'écart entre `performance.now()` et `Date.now()`. La milliseconde est la
/// résolution du `Date` du TypeScript ; en donner plus obligerait le frontend à diviser.
pub type UnixMillis = u64;

/// Le temps, sous ses deux formes.
///
/// Un seul trait pour les deux, et non deux traits : une application qui n'a qu'un temps
/// ne doit pas pouvoir avancer d'un côté sans avancer de l'autre, ce que deux horloges
/// injectées séparément rendraient possible dans un test — et invisible.
pub trait Clock: Send + Sync {
    /// L'instant courant, **monotone**. Pour mesurer un écart : les trente secondes d'une
    /// ligne finie, le délai d'un débit limité.
    fn now(&self) -> Instant;

    /// L'heure **murale**, pour dater un événement que quelqu'un d'autre devra situer.
    ///
    /// Elle peut reculer — l'utilisateur change de fuseau, `ntp` recale la machine — et
    /// c'est pourquoi aucune règle du produit ne s'appuie dessus : elle ne sert qu'à dire
    /// *quand* une chose est arrivée, jamais à décider *si* un délai est écoulé.
    fn wall(&self) -> UnixMillis;
}

/// L'horloge du système, monotone d'un côté et murale de l'autre.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wall(&self) -> UnixMillis {
        // Une horloge posée avant 1970 rendrait une erreur, et un dépassement de `u64` ne
        // se produira pas avant l'an 584 millions : les deux se replient sur l'époque
        // plutôt que de paniquer dans une boucle de sonde.
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| UnixMillis::try_from(since.as_millis()).unwrap_or(UnixMillis::MAX))
            .unwrap_or_default()
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
