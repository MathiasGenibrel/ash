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
use super::preferences::{NotificationChoices, NotificationPreferences, NotificationStore};
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
    /// Les deux origines, posées une fois et jamais touchées.
    origin: Instant,
    /// **Le seul état de cette horloge** : le temps écoulé depuis les deux origines.
    ///
    /// Un unique compteur, et non un `Instant` et une durée tenus côte à côte : les deux
    /// formes du temps se **dérivent** alors l'une de l'autre, au lieu d'être avancées
    /// séparément et de pouvoir se désaccorder. C'est la même forme que le `ControlledTime`
    /// de `features/git`, et c'est ce que le trait [`Clock`] promet — une application qui
    /// n'a qu'un temps.
    elapsed: Mutex<Duration>,
}

impl ManualClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            origin: Instant::now(),
            elapsed: Mutex::new(Duration::ZERO),
        })
    }

    pub fn advance(&self, seconds: u64) {
        if let Ok(mut elapsed) = self.elapsed.lock() {
            *elapsed += Duration::from_secs(seconds);
        }
    }

    fn elapsed(&self) -> Duration {
        self.elapsed
            .lock()
            .map(|elapsed| *elapsed)
            .unwrap_or_default()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.origin + self.elapsed()
    }

    fn wall(&self) -> UnixMillis {
        FAKE_EPOCH + UnixMillis::try_from(self.elapsed().as_millis()).unwrap_or_default()
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

/// Le fichier de préférences de notification, en mémoire.
///
/// Ici plutôt que dans `preferences.rs` parce qu'il sert **deux** modules : celui qui décide
/// des trois interrupteurs, et le superviseur, qui doit pouvoir décrire un utilisateur ayant
/// coupé `waiting` sans toucher au `$HOME` de qui lance les tests.
pub struct FakeNotificationStore(Mutex<Option<NotificationChoices>>);

impl FakeNotificationStore {
    /// Les préférences telles qu'une session précédente les aurait laissées.
    pub fn holding(choices: NotificationChoices) -> Arc<NotificationPreferences> {
        Arc::new(NotificationPreferences::restore(Arc::new(Self(
            Mutex::new(Some(choices)),
        ))))
    }
}

impl NotificationStore for FakeNotificationStore {
    fn load(&self) -> Option<NotificationChoices> {
        self.0.lock().ok().and_then(|held| *held)
    }

    fn save(&self, choices: NotificationChoices) -> Result<(), std::io::Error> {
        if let Ok(mut held) = self.0.lock() {
            *held = Some(choices);
        }
        Ok(())
    }
}

/// Un transcript qu'un scénario **décrit**, au lieu de l'écrire sur le disque.
///
/// C'est le port [`Transcripts`] tenu par une table : le test dit « ce chemin porte ce
/// texte », et le superviseur lit exactement ça. Un chemin absent rend `None`, comme le
/// vrai lecteur devant un fichier effacé.
#[derive(Debug, Default)]
pub(crate) struct FakeTranscripts {
    tails: std::collections::HashMap<std::path::PathBuf, String>,
}

impl FakeTranscripts {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub(crate) fn holding(mut self, path: &str, tail: &str) -> Self {
        self.tails
            .insert(std::path::PathBuf::from(path), tail.to_owned());
        self
    }
}

impl crate::features::agents::usage::Transcripts for FakeTranscripts {
    fn tail(&self, path: &std::path::Path) -> Option<String> {
        self.tails.get(path).cloned()
    }
}

/// La configuration d'un outil qu'un scénario **décrit**, au lieu de l'écrire dans un foyer.
///
/// Le port [`ToolConfig`](super::usage::ToolConfig) tenu par deux tables et un dossier. C'est
/// ce qui garantit qu'**aucun `cargo test` ne lit le vrai `~/.claude/settings.json`** : un
/// test qui tomberait sur celui de la machine qui le lance dirait `opus[1m]` chez son auteur
/// et rien chez le voisin.
#[derive(Debug, Default)]
pub(crate) struct FakeToolConfig {
    files: std::collections::HashMap<std::path::PathBuf, String>,
    home: Option<std::path::PathBuf>,
}

impl FakeToolConfig {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Le foyer de cet utilisateur-là — celui du scénario, jamais celui de la machine.
    #[must_use]
    pub(crate) fn homed_at(mut self, home: &str) -> Self {
        self.home = Some(std::path::PathBuf::from(home));
        self
    }

    /// Un fichier de configuration, et ce qu'il contient.
    #[must_use]
    pub(crate) fn holding(mut self, path: &str, contents: &str) -> Self {
        self.files
            .insert(std::path::PathBuf::from(path), contents.to_owned());
        self
    }

    /// Le raccourci des scénarios qui ne parlent que du modèle — le seul cas courant.
    #[must_use]
    pub(crate) fn declaring_model(self, path: &str, model: &str) -> Self {
        self.holding(path, &format!(r#"{{"model":"{model}"}}"#))
    }
}

impl crate::features::agents::usage::ToolConfig for FakeToolConfig {
    /// Aucune variable d'environnement dans les scénarios du superviseur.
    ///
    /// La priorité d'`ANTHROPIC_MODEL` se prouve là où elle est décidée — dans les tests de
    /// `usage.rs` —, et la redire ici ne dirait rien du superviseur.
    fn variable(&self, _name: &str) -> Option<String> {
        None
    }

    fn read(&self, path: &std::path::Path) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn home(&self) -> Option<std::path::PathBuf> {
        self.home.clone()
    }
}
