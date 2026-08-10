use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

use super::error::PtyError;
use super::flow::Credits;
use super::session::{PtySession, PtySpawner, PtySpec};

/// Identifiant d'onglet — un ulid, posé dans `ASH_TAB_ID` au lancement du shell.
///
/// C'est par lui, et par rien d'autre, que les events d'agent seront corrélés
/// ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)) : ni par le `cwd`, ni par
/// un horodatage.
pub type TabId = String;

/// Morceaux qui peuvent être en vol sans acquittement de la webview.
///
/// Huit lectures de 64 Kio font 512 Kio, très loin des 50 Mo au-delà desquels xterm.js
/// jette la sortie (voir [`super::flow`] et `docs/spike-xterm.md`).
const WINDOW: usize = 8;

/// Les PTY vivants, et rien d'autre.
///
/// Le registre détient l'état : le frontend l'affiche, il ne le possède pas
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
pub struct PtyRegistry {
    spawner: Box<dyn PtySpawner>,
    tabs: Mutex<HashMap<TabId, Tab>>,
}

struct Tab {
    session: Box<dyn PtySession>,
    credits: Arc<Credits>,
}

/// Ce qu'`open` rend au-delà de l'identifiant : de quoi lancer le lecteur.
pub struct Opened {
    pub tab_id: TabId,
    pub reader: Box<dyn Read + Send>,
    pub credits: Arc<Credits>,
}

impl PtyRegistry {
    pub fn new(spawner: Box<dyn PtySpawner>) -> Self {
        Self {
            spawner,
            tabs: Mutex::new(HashMap::new()),
        }
    }

    pub fn open(&self, mut spec: PtySpec, tab_id: TabId) -> Result<Opened, PtyError> {
        spec.env.push(("ASH_TAB_ID".to_owned(), tab_id.clone()));

        let (session, reader) = self.spawner.spawn(&spec)?;
        let credits = Arc::new(Credits::new(WINDOW));

        self.lock()?.insert(
            tab_id.clone(),
            Tab {
                session,
                credits: Arc::clone(&credits),
            },
        );

        Ok(Opened {
            tab_id,
            reader,
            credits,
        })
    }

    pub fn write(&self, tab_id: &str, bytes: &[u8]) -> Result<(), PtyError> {
        self.with_tab(tab_id, |tab| tab.session.write(bytes))
    }

    pub fn resize(&self, tab_id: &str, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.with_tab(tab_id, |tab| tab.session.resize(cols, rows))
    }

    /// La webview a fini d'écrire un morceau : le lecteur peut en émettre un de plus.
    pub fn ack(&self, tab_id: &str) -> Result<(), PtyError> {
        self.with_tab(tab_id, |tab| {
            tab.credits.release();
            Ok(())
        })
    }

    /// Ferme un onglet : le processus est terminé et le lecteur réveillé.
    ///
    /// Idempotent. Fermer un onglet dont le shell vient de sortir de lui-même est le cas
    /// nominal, pas une erreur à remonter à l'utilisateur.
    pub fn close(&self, tab_id: &str) -> Result<(), PtyError> {
        let Some(mut tab) = self.lock()?.remove(tab_id) else {
            return Ok(());
        };
        // Fermer les crédits d'abord : un lecteur bloqué en attente doit être réveillé
        // pour constater l'arrêt, sinon son thread survit au shell.
        tab.credits.close();
        tab.session.kill()
    }

    /// Retire l'onglet dont le shell est sorti tout seul.
    pub fn forget(&self, tab_id: &str) {
        if let Ok(mut tabs) = self.tabs.lock() {
            if let Some(tab) = tabs.remove(tab_id) {
                tab.credits.close();
            }
        }
    }

    fn with_tab<T>(
        &self,
        tab_id: &str,
        action: impl FnOnce(&mut Tab) -> Result<T, PtyError>,
    ) -> Result<T, PtyError> {
        let mut tabs = self.lock()?;
        let tab = tabs
            .get_mut(tab_id)
            .ok_or_else(|| PtyError::UnknownTab(tab_id.to_owned()))?;
        action(tab)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<TabId, Tab>>, PtyError> {
        self.tabs
            .lock()
            .map_err(|_| PtyError::Io("registre de PTY empoisonné".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::session::OpenPty;
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    struct FakeSession {
        killed: Arc<AtomicBool>,
        written: Arc<Mutex<Vec<u8>>>,
        resized: Arc<Mutex<Vec<(u16, u16)>>>,
    }

    impl PtySession for FakeSession {
        fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
            self.written.lock().unwrap().extend_from_slice(bytes);
            Ok(())
        }
        fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
            self.resized.lock().unwrap().push((cols, rows));
            Ok(())
        }
        fn kill(&mut self) -> Result<(), PtyError> {
            self.killed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeSpawner {
        killed: Arc<AtomicBool>,
        written: Arc<Mutex<Vec<u8>>>,
        resized: Arc<Mutex<Vec<(u16, u16)>>>,
        spawns: Arc<AtomicUsize>,
        last_env: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl PtySpawner for FakeSpawner {
        fn spawn(&self, spec: &PtySpec) -> Result<OpenPty, PtyError> {
            self.spawns.fetch_add(1, Ordering::SeqCst);
            *self.last_env.lock().unwrap() = spec.env.clone();
            let session = FakeSession {
                killed: Arc::clone(&self.killed),
                written: Arc::clone(&self.written),
                resized: Arc::clone(&self.resized),
            };
            Ok((Box::new(session), Box::new(std::io::empty())))
        }
    }

    fn spec() -> PtySpec {
        PtySpec {
            shell: "/bin/bash".into(),
            cwd: "/tmp".into(),
            cols: 80,
            rows: 24,
            env: vec![("ASH_SOCK".to_owned(), "/tmp/ash.sock".to_owned())],
        }
    }

    #[test]
    fn given_a_tab_is_opened_when_the_shell_starts_then_it_carries_its_own_ash_tab_id() {
        // Given
        let spawner = FakeSpawner::default();
        let env = Arc::clone(&spawner.last_env);
        let registry = PtyRegistry::new(Box::new(spawner));

        // When
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();

        // Then
        let env = env.lock().unwrap().clone();
        assert!(env.contains(&("ASH_TAB_ID".to_owned(), "01J0TAB".to_owned())));
        assert!(env.contains(&("ASH_SOCK".to_owned(), "/tmp/ash.sock".to_owned())));
        assert_eq!(opened.tab_id, "01J0TAB");
    }

    #[test]
    fn given_an_open_tab_when_it_is_closed_then_the_process_is_killed_and_the_reader_released() {
        // Given
        let spawner = FakeSpawner::default();
        let killed = Arc::clone(&spawner.killed);
        let registry = PtyRegistry::new(Box::new(spawner));
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();

        // When
        registry.close(&opened.tab_id).unwrap();

        // Then
        assert!(killed.load(Ordering::SeqCst), "le shell doit être terminé");
        assert!(
            !opened.credits.acquire(),
            "le lecteur doit être réveillé avec un ordre d'arrêt"
        );
    }

    #[test]
    fn given_a_closed_tab_when_it_is_closed_again_then_it_is_not_an_error() {
        // Given
        let registry = PtyRegistry::new(Box::new(FakeSpawner::default()));
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();
        registry.close(&opened.tab_id).unwrap();

        // When
        let again = registry.close(&opened.tab_id);

        // Then
        assert!(again.is_ok());
    }

    #[test]
    fn given_a_tab_that_no_longer_exists_when_writing_to_it_then_it_fails_without_panicking() {
        // Given
        let registry = PtyRegistry::new(Box::new(FakeSpawner::default()));
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();
        registry.close(&opened.tab_id).unwrap();

        // When
        let written = registry.write(&opened.tab_id, b"ls\n");

        // Then
        assert!(matches!(written, Err(PtyError::UnknownTab(_))));
    }

    #[test]
    fn given_an_open_tab_when_the_webview_acks_then_the_reader_gets_a_credit_back() {
        // Given — la fenêtre est vidée, le lecteur serait bloqué
        let registry = PtyRegistry::new(Box::new(FakeSpawner::default()));
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();
        for _ in 0..WINDOW {
            assert!(opened.credits.acquire());
        }

        // When
        registry.ack(&opened.tab_id).unwrap();

        // Then
        assert!(
            opened.credits.acquire(),
            "l'acquittement doit débloquer une émission"
        );
    }
}
