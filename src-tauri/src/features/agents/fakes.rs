//! Les effets système de la feature, remplacés par ce que le test décrit.
//!
//! Il n'y en a qu'un ici, et c'est le **temps** : la machine à états et le superviseur
//! parlent tous les deux des trente secondes de la spec §6.4, et une horloge par module
//! ferait deux temps différents dans une feature qui n'en a qu'un. Même raison que
//! `pty/fakes.rs` et `git/fakes.rs`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::shared::time::Clock;

/// Une horloge qu'on avance à la main.
///
/// C'est tout ce qu'il faut pour prouver « 30 s » et « une heure sans rien » sans qu'aucun
/// test ne dorme une milliseconde — et un test qui dort finit par être désactivé.
pub struct ManualClock(Mutex<Instant>);

impl ManualClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Instant::now())))
    }

    pub fn advance(&self, seconds: u64) {
        if let Ok(mut now) = self.0.lock() {
            *now += Duration::from_secs(seconds);
        }
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.0
            .lock()
            .map(|now| *now)
            .unwrap_or_else(|_| Instant::now())
    }
}
