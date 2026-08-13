//! Les effets système de la feature, remplacés par ce que le test décrit.
//!
//! Deux, et ils ont la même raison d'être ici plutôt que dans le module qui les consomme :
//! le **temps** — la machine à états et le superviseur parlent tous les deux des trente
//! secondes de la spec §6.4, et une horloge par module ferait deux temps différents dans une
//! feature qui n'en a qu'un — et la **notification**, que `notify` décide et que le
//! superviseur poste. Même raison que `pty/fakes.rs` et `git/fakes.rs`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::notify::{Notice, Notifier};
use crate::shared::time::{Clock, UnixMillis};

/// L'heure qu'il est au début de chaque scénario — le 1ᵉʳ janvier 2026 à minuit UTC.
///
/// Une date **fixe** : une datation lue dans un `Then` doit valoir la même chose sur la
/// machine de qui lance les tests que sur celle d'à côté.
pub const FAKE_EPOCH: UnixMillis = 1_767_225_600_000;

/// Une horloge qu'on avance à la main, murale et monotone **ensemble**.
///
/// C'est tout ce qu'il faut pour prouver « 30 s » et « une heure sans rien » sans qu'aucun
/// test ne dorme une milliseconde — et un test qui dort finit par être désactivé. Les deux
/// formes du temps avancent du même pas : un scénario qui ferait vieillir l'une sans
/// l'autre décrirait une machine qui n'existe pas.
pub struct ManualClock {
    monotonic: Mutex<Instant>,
    elapsed: Mutex<Duration>,
}

impl ManualClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            monotonic: Mutex::new(Instant::now()),
            elapsed: Mutex::new(Duration::ZERO),
        })
    }

    pub fn advance(&self, seconds: u64) {
        let step = Duration::from_secs(seconds);
        if let Ok(mut now) = self.monotonic.lock() {
            *now += step;
        }
        if let Ok(mut elapsed) = self.elapsed.lock() {
            *elapsed += step;
        }
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.monotonic
            .lock()
            .map(|now| *now)
            .unwrap_or_else(|_| Instant::now())
    }

    fn wall(&self) -> UnixMillis {
        let elapsed = self
            .elapsed
            .lock()
            .map(|elapsed| UnixMillis::try_from(elapsed.as_millis()).unwrap_or_default())
            .unwrap_or_default();
        FAKE_EPOCH + elapsed
    }
}

/// Ce qui aurait interrompu l'utilisateur, retenu au lieu d'être posé sur son écran.
///
/// Aucun test de cette feature ne doit faire apparaître une vraie bannière macOS : ce
/// double est ce qui rend « une seule notification pour un `waiting` qui dure » assertable,
/// et pas seulement plausible.
#[derive(Default)]
pub struct FakeNotifier(Mutex<Vec<Notice>>);

impl FakeNotifier {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Ce qui a été posé, dans l'ordre.
    pub fn posted(&self) -> Vec<Notice> {
        self.0
            .lock()
            .map(|posted| posted.clone())
            .unwrap_or_default()
    }

    /// Les titres seuls — ce qu'un `Then` lit le plus souvent.
    pub fn titles(&self) -> Vec<String> {
        self.posted()
            .into_iter()
            .map(|notice| notice.title)
            .collect()
    }
}

impl Notifier for FakeNotifier {
    fn post(&self, notice: Notice) {
        if let Ok(mut posted) = self.0.lock() {
            posted.push(notice);
        }
    }
}
