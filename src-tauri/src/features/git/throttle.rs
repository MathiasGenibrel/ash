//! « Au plus un rafraîchissement toutes les 5 s par worktree » (spec §5.3).
//!
//! La règle n'est **pas** « ignorer ce qui arrive pendant 5 s » : un rebase écrit une
//! rafale de fichiers, et l'état qui compte est celui qui reste à la fin. Un événement
//! reçu dans la fenêtre n'est donc pas perdu — il est **différé**, et la relecture qui
//! suit lit le dernier état sur le disque.
//!
//! Tout est ici plutôt que dans la boucle qui l'utilise, et l'instant courant est un
//! paramètre : sans ça, vérifier la règle coûterait cinq secondes de sommeil par test.

use std::time::{Duration, Instant};

/// Le délai minimal entre deux relectures d'un même worktree (spec §5.3).
pub const MIN_INTERVAL: Duration = Duration::from_secs(5);

/// Ce qu'il faut faire d'une demande de rafraîchissement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Relire maintenant.
    Now,
    /// Relire dans ce délai — la fenêtre n'est pas écoulée.
    In(Duration),
    /// Ne rien faire : une relecture est déjà prévue, et elle lira le même disque.
    Pending,
}

/// La limitation de débit d'un worktree. Un état, pas un minuteur.
#[derive(Debug)]
pub struct Throttle {
    interval: Duration,
    last: Option<Instant>,
    deferred: bool,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
            deferred: false,
        }
    }

    /// Quelque chose demande une relecture : un fichier a bougé, la fenêtre a repris le
    /// focus, un onglet vient de se rattacher.
    pub fn request(&mut self, now: Instant) -> Decision {
        if self.deferred {
            return Decision::Pending;
        }
        match self.last {
            Some(last) if now.duration_since(last) < self.interval => {
                self.deferred = true;
                Decision::In(self.interval - now.duration_since(last))
            }
            _ => {
                self.last = Some(now);
                Decision::Now
            }
        }
    }

    /// Le délai est écoulé : la relecture différée a lieu maintenant.
    ///
    /// Rend `false` si plus rien n'était différé — un minuteur peut se réveiller après un
    /// arrêt, et relire alors ferait un rafraîchissement de trop.
    pub fn due(&mut self, now: Instant) -> bool {
        if !self.deferred {
            return false;
        }
        self.deferred = false;
        self.last = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : une horloge d'instants relatifs, lisible dans un `Given`.
    ///
    /// `Instant` ne se construit pas de zéro — il est monotone et opaque. Un point de
    /// départ pris une fois, et des décalages explicites, suffisent à écrire le temps
    /// sans jamais le laisser passer.
    struct Timeline(Instant);

    impl Timeline {
        fn start() -> Self {
            Self(Instant::now())
        }

        fn at(&self, seconds: u64) -> Instant {
            self.0 + Duration::from_secs(seconds)
        }
    }

    fn throttle() -> Throttle {
        Throttle::new(MIN_INTERVAL)
    }

    #[test]
    fn given_a_worktree_never_read_when_something_asks_for_a_refresh_then_it_happens_immediately() {
        // Given — le rattachement d'un onglet ne doit pas attendre cinq secondes
        let time = Timeline::start();
        let mut throttle = throttle();

        // When
        let decision = throttle.request(time.at(0));

        // Then
        assert_eq!(decision, Decision::Now);
    }

    #[test]
    fn given_a_burst_of_writes_inside_the_window_when_they_arrive_then_a_single_refresh_is_deferred(
    ) {
        // Given — un rebase écrit `HEAD`, `msgnum`, `end` et ses refs en quelques
        // millisecondes ; relire à chaque écriture serait le sondage qu'on évite
        let time = Timeline::start();
        let mut throttle = throttle();
        throttle.request(time.at(0));

        // When
        let decisions: Vec<_> = [1, 2, 3]
            .map(|second| throttle.request(time.at(second)))
            .to_vec();

        // Then — un seul report, puis le silence : la relecture prévue lira le dernier
        // état, elle n'a pas besoin d'être reprogrammée
        assert_eq!(
            decisions,
            vec![
                Decision::In(Duration::from_secs(4)),
                Decision::Pending,
                Decision::Pending
            ]
        );
    }

    #[test]
    fn given_a_deferred_refresh_when_its_delay_elapses_then_it_takes_place_and_reopens_the_window()
    {
        // Given — l'événement reçu pendant la fenêtre n'est pas perdu
        let time = Timeline::start();
        let mut throttle = throttle();
        throttle.request(time.at(0));
        throttle.request(time.at(1));

        // When
        let took_place = throttle.due(time.at(5));

        // Then — et la fenêtre repart de la relecture qui vient d'avoir lieu
        assert!(took_place);
        assert_eq!(
            throttle.request(time.at(6)),
            Decision::In(Duration::from_secs(4))
        );
    }

    #[test]
    fn given_a_quiet_period_longer_than_the_window_when_a_write_arrives_then_it_is_read_at_once() {
        // Given — la limitation ne doit pas ajouter de latence quand rien ne presse
        let time = Timeline::start();
        let mut throttle = throttle();
        throttle.request(time.at(0));

        // When
        let decision = throttle.request(time.at(5));

        // Then
        assert_eq!(decision, Decision::Now);
    }

    #[test]
    fn given_nothing_deferred_when_a_late_timer_fires_then_no_extra_refresh_happens() {
        // Given — un minuteur programmé puis rendu inutile par un arrêt, ou par une
        // relecture déjà faite
        let time = Timeline::start();
        let mut throttle = throttle();
        throttle.request(time.at(0));

        // When
        let took_place = throttle.due(time.at(5));

        // Then
        assert!(!took_place);
    }
}
