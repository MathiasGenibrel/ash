use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::error::PtyError;
use super::flow::Credits;
use super::session::{PtySession, PtySpawner, PtySpec};
use crate::features::probe::{Probe, TabWatch};

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

/// Les PTY vivants, **dans l'ordre**, et rien d'autre.
///
/// Le registre détient l'état : le frontend l'affiche, il ne le possède pas
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). L'ordre en fait
/// partie — c'est lui que `Cmd+1..9` désigne (spec §4.4), et une table de hachage n'en
/// a pas. Un `Vec` en donne un stable et une suppression qui préserve le reste ; la
/// recherche linéaire est sans objet à cette échelle, un utilisateur n'ouvre pas mille
/// onglets.
pub struct PtyRegistry {
    spawner: Box<dyn PtySpawner>,
    /// La sonde d'ADR-0005, injectée : c'est elle qui donne son `cwd` vivant à un onglet.
    probe: Arc<dyn Probe>,
    tabs: Mutex<Vec<Tab>>,
}

struct Tab {
    id: TabId,
    session: Box<dyn PtySession>,
    credits: Arc<Credits>,
    /// Répertoire de départ du shell, retenu à l'ouverture.
    ///
    /// Ce n'est plus ce que l'onglet montre : c'est le repli quand la sonde ne sait pas
    /// répondre. Un onglet doit toujours avoir un répertoire à afficher, même sur un
    /// système qui refuse de parler.
    start_dir: PathBuf,
    /// La sonde de cet onglet, quand le système le rend observable.
    ///
    /// Elle est rangée **ici**, à côté de la session : le descripteur qu'elle interroge
    /// appartient au master du PTY, et les deux disparaissent donc ensemble.
    watch: Option<TabWatch>,
}

/// Ce qu'un onglet montre de lui-même au frontend.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    pub tab_id: TabId,
    /// Le répertoire **courant** de l'onglet, sondé à la demande.
    ///
    /// C'est lui que « nouvel onglet dans le worktree courant » (spec §4.4) reprend : le
    /// répertoire où l'onglet en est, pas celui d'où il est parti.
    pub cwd: String,
}

/// Ce qu'`open` rend au-delà de l'identifiant : de quoi lancer le lecteur.
pub struct Opened {
    pub tab_id: TabId,
    pub reader: Box<dyn Read + Send>,
    pub credits: Arc<Credits>,
}

impl PtyRegistry {
    pub fn new(spawner: Box<dyn PtySpawner>, probe: Arc<dyn Probe>) -> Self {
        Self {
            spawner,
            probe,
            tabs: Mutex::new(Vec::new()),
        }
    }

    pub fn open(&self, mut spec: PtySpec, tab_id: TabId) -> Result<Opened, PtyError> {
        spec.env.push(("ASH_TAB_ID".to_owned(), tab_id.clone()));

        let (session, reader) = self.spawner.spawn(&spec)?;
        let credits = Arc::new(Credits::new(WINDOW));

        let watch = session
            .terminal()
            .map(|terminal| TabWatch::new(terminal.master_fd, terminal.shell_pid));

        // Un onglet neuf va à la fin : c'est l'ordre que la barre d'onglets montre, et
        // celui que `Cmd+1..9` numérote.
        self.lock()?.push(Tab {
            id: tab_id.clone(),
            session,
            credits: Arc::clone(&credits),
            start_dir: spec.cwd.clone(),
            watch,
        });

        Ok(Opened {
            tab_id,
            reader,
            credits,
        })
    }

    /// Les onglets vivants, dans leur ordre d'affichage, avec leur répertoire courant.
    ///
    /// Le `cwd` est sondé ici, à la demande, et non par une boucle de fond : tant que
    /// personne n'écoute en continu — la sidebar d'ADR-0006 n'existe pas encore — une
    /// boucle de ~300 ms sonderait pour rien. La sonde, elle, est déjà taillée pour cette
    /// boucle (voir `TabWatch::observe_change`).
    pub fn tabs(&self) -> Result<Vec<TabInfo>, PtyError> {
        let probe = self.probe.as_ref();
        Ok(self
            .lock()?
            .iter_mut()
            .map(|tab| TabInfo {
                tab_id: tab.id.clone(),
                cwd: tab.cwd(probe).display().to_string(),
            })
            .collect())
    }

    /// Vrai si quelque chose d'autre que le shell tient l'avant-plan de l'onglet.
    ///
    /// C'est la question que `Cmd+W` pose avant de détruire quoi que ce soit (spec §4.4).
    pub fn has_foreground_process(&self, tab_id: &str) -> Result<bool, PtyError> {
        self.with_tab(tab_id, |tab| tab.session.has_foreground_process())
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
        // `remove` sur un `Vec` décale la suite : c'est exactement ce qu'on veut, l'ordre
        // des onglets restants ne doit pas bouger quand on en ferme un au milieu.
        let Some(mut tab) = self.take(tab_id)? else {
            return Ok(());
        };
        // Fermer les crédits d'abord : un lecteur bloqué en attente doit être réveillé
        // pour constater l'arrêt, sinon son thread survit au shell.
        tab.credits.close();
        tab.session.kill()
    }

    /// Retire l'onglet dont le shell est sorti tout seul.
    pub fn forget(&self, tab_id: &str) {
        if let Ok(Some(tab)) = self.take(tab_id) {
            tab.credits.close();
        }
    }

    fn take(&self, tab_id: &str) -> Result<Option<Tab>, PtyError> {
        let mut tabs = self.lock()?;
        Ok(tabs
            .iter()
            .position(|tab| tab.id == tab_id)
            .map(|at| tabs.remove(at)))
    }

    fn with_tab<T>(
        &self,
        tab_id: &str,
        action: impl FnOnce(&mut Tab) -> Result<T, PtyError>,
    ) -> Result<T, PtyError> {
        let mut tabs = self.lock()?;
        let tab = tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id)
            .ok_or_else(|| PtyError::UnknownTab(tab_id.to_owned()))?;
        action(tab)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Vec<Tab>>, PtyError> {
        self.tabs
            .lock()
            .map_err(|_| PtyError::Io("registre de PTY empoisonné".to_owned()))
    }
}

impl Tab {
    /// Le répertoire courant de l'onglet, ou son répertoire de départ si la sonde se tait.
    ///
    /// Se taire n'est pas une erreur à remonter : un onglet dont le shell vient de mourir
    /// est encore affiché le temps que le frontend l'apprenne, et le faire disparaître de
    /// la liste pour cette raison serait pire que de montrer un répertoire un peu vieux.
    fn cwd(&mut self, probe: &dyn Probe) -> PathBuf {
        self.watch
            .as_mut()
            .and_then(|watch| watch.observe(probe).ok())
            .map_or_else(|| self.start_dir.clone(), |seen| seen.cwd)
    }
}

#[cfg(test)]
mod tests {
    use super::super::session::{OpenPty, Terminal};
    use super::*;
    use crate::features::probe::{Pid, ProbeError, ProcessInfo};
    use std::os::fd::RawFd;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// Le terminal que les faux onglets annoncent. Aucun appel système ne le touche : le
    /// `FakeProbe` répond sans regarder.
    const TERMINAL: Terminal = Terminal {
        master_fd: 7,
        shell_pid: 100,
    };

    /// Une sonde qui répond ce qu'on lui a dit, ou qui se tait.
    #[derive(Default)]
    struct FakeProbe {
        cwd: Option<PathBuf>,
    }

    impl FakeProbe {
        fn silent() -> Self {
            Self::default()
        }

        fn reporting(cwd: &str) -> Self {
            Self {
                cwd: Some(PathBuf::from(cwd)),
            }
        }
    }

    impl Probe for FakeProbe {
        fn foreground_pgid(&self, terminal: RawFd) -> Result<Pid, ProbeError> {
            self.cwd
                .as_ref()
                .map(|_| TERMINAL.shell_pid)
                .ok_or(ProbeError::NoForeground(terminal))
        }

        fn inspect(&self, pid: Pid) -> Result<ProcessInfo, ProbeError> {
            self.cwd
                .as_ref()
                .map(|cwd| ProcessInfo {
                    pid,
                    name: "bash".to_owned(),
                    cwd: cwd.clone(),
                })
                .ok_or(ProbeError::Vanished(pid))
        }
    }

    #[derive(Default)]
    struct FakeSession {
        killed: Arc<AtomicBool>,
        written: Arc<Mutex<Vec<u8>>>,
        resized: Arc<Mutex<Vec<(u16, u16)>>>,
        foreground: Arc<AtomicBool>,
        observable: bool,
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
        fn has_foreground_process(&mut self) -> Result<bool, PtyError> {
            Ok(self.foreground.load(Ordering::SeqCst))
        }
        fn terminal(&self) -> Option<Terminal> {
            self.observable.then_some(TERMINAL)
        }
    }

    #[derive(Default)]
    struct FakeSpawner {
        killed: Arc<AtomicBool>,
        written: Arc<Mutex<Vec<u8>>>,
        resized: Arc<Mutex<Vec<(u16, u16)>>>,
        spawns: Arc<AtomicUsize>,
        last_env: Arc<Mutex<Vec<(String, String)>>>,
        foreground: Arc<AtomicBool>,
        /// Les onglets ouverts par ce spawner sont-ils sondables ?
        observable: bool,
    }

    impl PtySpawner for FakeSpawner {
        fn spawn(&self, spec: &PtySpec) -> Result<OpenPty, PtyError> {
            self.spawns.fetch_add(1, Ordering::SeqCst);
            *self.last_env.lock().unwrap() = spec.env.clone();
            let session = FakeSession {
                killed: Arc::clone(&self.killed),
                written: Arc::clone(&self.written),
                resized: Arc::clone(&self.resized),
                foreground: Arc::clone(&self.foreground),
                observable: self.observable,
            };
            Ok((Box::new(session), Box::new(std::io::empty())))
        }
    }

    /// Test Data Builder : un `PtySpec` valide et déterministe, dont on ne surcharge que
    /// ce que le scénario regarde.
    struct SpecBuilder {
        cwd: PathBuf,
    }

    impl SpecBuilder {
        fn new() -> Self {
            Self { cwd: "/tmp".into() }
        }

        fn starting_in(mut self, cwd: &str) -> Self {
            self.cwd = cwd.into();
            self
        }

        fn build(self) -> PtySpec {
            PtySpec {
                shell: "/bin/bash".into(),
                cwd: self.cwd,
                cols: 80,
                rows: 24,
                env: vec![("ASH_SOCK".to_owned(), "/tmp/ash.sock".to_owned())],
            }
        }
    }

    fn spec() -> PtySpec {
        SpecBuilder::new().build()
    }

    /// Un registre dont les onglets ne sont pas sondables : la plupart des règles du
    /// registre — ordre, fermeture, crédits — n'ont rien à voir avec la sonde.
    fn registry(spawner: FakeSpawner) -> PtyRegistry {
        PtyRegistry::new(Box::new(spawner), Arc::new(FakeProbe::silent()))
    }

    fn ids(registry: &PtyRegistry) -> Vec<TabId> {
        registry
            .tabs()
            .unwrap()
            .into_iter()
            .map(|tab| tab.tab_id)
            .collect()
    }

    #[test]
    fn given_a_tab_is_opened_when_the_shell_starts_then_it_carries_its_own_ash_tab_id() {
        // Given
        let spawner = FakeSpawner::default();
        let env = Arc::clone(&spawner.last_env);
        let registry = registry(spawner);

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
        let registry = registry(spawner);
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
        let registry = registry(FakeSpawner::default());
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
        let registry = registry(FakeSpawner::default());
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();
        registry.close(&opened.tab_id).unwrap();

        // When
        let written = registry.write(&opened.tab_id, b"ls\n");

        // Then
        assert!(matches!(written, Err(PtyError::UnknownTab(_))));
    }

    #[test]
    fn given_three_tabs_opened_in_turn_when_the_middle_one_closes_then_the_others_keep_their_order()
    {
        // Given — l'ordre est ce que `Cmd+1..9` numérote : il ne doit pas se réarranger
        // à la fermeture d'un onglet.
        let registry = registry(FakeSpawner::default());
        for id in ["A", "B", "C"] {
            registry.open(spec(), id.to_owned()).unwrap();
        }

        // When
        registry.close("B").unwrap();

        // Then
        assert_eq!(ids(&registry), vec!["A".to_owned(), "C".to_owned()]);
    }

    #[test]
    fn given_a_tab_closed_in_the_middle_when_a_new_one_is_opened_then_it_lands_last() {
        // Given
        let registry = registry(FakeSpawner::default());
        for id in ["A", "B", "C"] {
            registry.open(spec(), id.to_owned()).unwrap();
        }
        registry.close("A").unwrap();

        // When
        registry.open(spec(), "D".to_owned()).unwrap();

        // Then — et non pas dans le trou laissé par « A »
        assert_eq!(
            ids(&registry),
            vec!["B".to_owned(), "C".to_owned(), "D".to_owned()]
        );
    }

    #[test]
    fn given_a_tab_whose_shell_has_moved_when_the_frontend_lists_the_tabs_then_it_learns_the_current_directory(
    ) {
        // Given — l'onglet est parti de /dev/ash, la sonde le voit dans un worktree
        let registry = PtyRegistry::new(
            Box::new(FakeSpawner {
                observable: true,
                ..FakeSpawner::default()
            }),
            Arc::new(FakeProbe::reporting("/dev/ash/worktrees/probe")),
        );

        // When
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();

        // Then — c'est ce répertoire-là que « nouvel onglet dans le worktree courant »
        // (spec §4.4) reprend, pas celui de lancement
        assert_eq!(
            registry.tabs().unwrap(),
            vec![TabInfo {
                tab_id: "A".to_owned(),
                cwd: "/dev/ash/worktrees/probe".to_owned(),
            }]
        );
    }

    #[test]
    fn given_a_tab_the_probe_cannot_observe_when_the_frontend_lists_the_tabs_then_it_falls_back_to_the_start_directory(
    ) {
        // Given — un système qui ne répond pas ne doit pas produire un onglet sans nom
        let registry = registry(FakeSpawner {
            observable: true,
            ..FakeSpawner::default()
        });

        // When
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();

        // Then
        assert_eq!(
            registry.tabs().unwrap(),
            vec![TabInfo {
                tab_id: "A".to_owned(),
                cwd: "/dev/ash".to_owned(),
            }]
        );
    }

    #[test]
    fn given_a_shell_that_handed_the_terminal_over_when_the_tab_is_questioned_then_it_reports_a_running_process(
    ) {
        // Given
        let spawner = FakeSpawner::default();
        let foreground = Arc::clone(&spawner.foreground);
        let registry = registry(spawner);
        registry.open(spec(), "A".to_owned()).unwrap();

        // When
        foreground.store(true, Ordering::SeqCst);

        // Then — le frontend n'a plus qu'à demander confirmation avant de fermer
        assert!(registry.has_foreground_process("A").unwrap());
    }

    #[test]
    fn given_an_open_tab_when_the_webview_acks_then_the_reader_gets_a_credit_back() {
        // Given — la fenêtre est vidée, le lecteur serait bloqué
        let registry = registry(FakeSpawner::default());
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
