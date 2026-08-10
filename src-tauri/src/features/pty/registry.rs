use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::error::PtyError;
use super::flow::Credits;
use super::session::{PtySession, PtySpawner, PtySpec};
use crate::features::probe::{Probe, TabObservation, TabWatch};

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
    watch: SharedWatch,
}

/// La sonde d'un onglet, tenue à part du verrou du registre.
///
/// Deux raisons, et les deux comptent :
///
/// - **elle se prend hors du registre.** Une passe de sonde fait deux appels système par
///   onglet, trois fois par seconde ([ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md)).
///   Sous le verrou global, chaque frappe clavier attendrait derrière elle — `write`,
///   `resize` et `ack` prennent ce verrou-là. Le registre n'est donc verrouillé que le
///   temps de recopier les poignées, et la sonde tourne dehors.
/// - **`None` veut dire « onglet fermé ».** Le descripteur du master part avec la
///   session ; un `fd` recyclé se relit sans erreur, et une sonde qui survit à son onglet
///   ne se tromperait pas bruyamment, elle se tromperait en silence.
type SharedWatch = Arc<Mutex<Option<TabWatch>>>;

/// Ce qu'une passe de la boucle de sonde a trouvé de neuf sur un onglet.
///
/// Seuls les onglets qui ont **bougé** en produisent un : un onglet posé à son invite
/// serait sinon annoncé trois fois par seconde, et réveillerait la webview pour rien.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabChange {
    pub tab_id: TabId,
    pub cwd: String,
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

        let watch =
            Arc::new(Mutex::new(session.terminal().map(|terminal| {
                TabWatch::new(terminal.master_fd, terminal.shell_pid)
            })));

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
    /// Le `cwd` est sondé à l'appel, et non recopié d'une passe précédente : c'est ce qui
    /// fait que « nouvel onglet dans le worktree courant » part du répertoire où l'onglet
    /// en est, et non de celui de la dernière ouverture.
    pub fn tabs(&self) -> Result<Vec<TabInfo>, PtyError> {
        Ok(self
            .snapshot()?
            .into_iter()
            .map(|tab| TabInfo {
                cwd: self
                    .observe(&tab.watch)
                    .map_or(tab.start_dir, |seen| seen.cwd)
                    .display()
                    .to_string(),
                tab_id: tab.id,
            })
            .collect())
    }

    /// Une passe de la boucle d'ADR-0005 : ce qui a **changé** depuis la précédente.
    ///
    /// Rien pour un onglet immobile, rien pour un onglet que le système ne sait plus
    /// décrire. C'est ce que la boucle émet vers le frontend, et c'est ce qui fait suivre
    /// le titre d'un onglet à travers les `cd` — y compris pendant qu'un programme tourne,
    /// là où OSC 7 se tairait.
    pub fn changes(&self) -> Result<Vec<TabChange>, PtyError> {
        Ok(self
            .snapshot()?
            .into_iter()
            .filter_map(|tab| {
                self.observe_change(&tab.watch).map(|seen| TabChange {
                    tab_id: tab.id,
                    cwd: seen.cwd.display().to_string(),
                })
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

    /// Les poignées des onglets, recopiées sous le verrou et rendues **dehors**.
    ///
    /// C'est tout ce que le registre garde de verrouillé pendant une passe de sonde :
    /// trois `clone` par onglet, et le verrou est rendu avant le premier appel système.
    fn snapshot(&self) -> Result<Vec<TabHandle>, PtyError> {
        Ok(self
            .lock()?
            .iter()
            .map(|tab| TabHandle {
                id: tab.id.clone(),
                start_dir: tab.start_dir.clone(),
                watch: Arc::clone(&tab.watch),
            })
            .collect())
    }

    /// Le répertoire courant d'un onglet, sondé hors du verrou du registre.
    fn observe(&self, watch: &SharedWatch) -> Option<TabObservation> {
        let mut watch = watch.lock().ok()?;
        watch.as_mut()?.observe(self.probe.as_ref()).ok()
    }

    /// Idem, mais silencieux tant que rien n'a bougé — la passe de la boucle de fond.
    fn observe_change(&self, watch: &SharedWatch) -> Option<TabObservation> {
        let mut watch = watch.lock().ok()?;
        watch.as_mut()?.observe_change(self.probe.as_ref())
    }

    fn take(&self, tab_id: &str) -> Result<Option<Tab>, PtyError> {
        let removed = {
            let mut tabs = self.lock()?;
            tabs.iter()
                .position(|tab| tab.id == tab_id)
                .map(|at| tabs.remove(at))
        };

        // La sonde s'éteint **avant** que la session — donc le descripteur du master — ne
        // parte. Prendre le verrou ici attend qu'une passe en vol se termine : après ce
        // point, aucune sonde ne peut plus lire un `fd` que le système est libre de
        // recycler. Le verrou du registre, lui, est déjà rendu : les deux ne sont jamais
        // tenus ensemble.
        if let Some(tab) = removed.as_ref() {
            if let Ok(mut watch) = tab.watch.lock() {
                *watch = None;
            }
        }

        Ok(removed)
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

/// De quoi décrire un onglet sans tenir le verrou du registre.
///
/// `start_dir` voyage avec la sonde parce que se taire n'est pas une erreur à remonter :
/// un onglet dont le shell vient de mourir est encore affiché le temps que le frontend
/// l'apprenne, et le faire disparaître de la liste pour cette raison serait pire que de
/// montrer un répertoire un peu vieux.
struct TabHandle {
    id: TabId,
    start_dir: PathBuf,
    watch: SharedWatch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::probe::{Pid, ProbeError, ProcessInfo};
    use crate::features::pty::fakes::{
        observed_registry, registry, spec, FakeSpawner, SpecBuilder,
    };
    use std::os::fd::RawFd;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;

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
        let (registry, _probe) = observed_registry("/dev/ash/worktrees/probe");

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
        let registry = registry(FakeSpawner::observable());

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

    #[test]
    fn given_a_tab_that_moved_since_the_last_listing_when_the_tabs_are_listed_again_then_the_new_directory_is_reported(
    ) {
        // Given — un `cd` après une première lecture de la liste
        let (registry, probe) = observed_registry("/dev/ash");
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();
        assert_eq!(registry.tabs().unwrap()[0].cwd, "/dev/ash");
        probe.move_to("/tmp");

        // When
        let listed = registry.tabs().unwrap();

        // Then — chaque lecture sonde à nouveau. Rendre la valeur de la lecture
        // précédente ferait partir `Cmd+N` du répertoire d'il y a une ouverture d'onglet.
        assert_eq!(listed[0].cwd, "/tmp");
    }

    #[test]
    fn given_a_tab_that_moved_when_the_loop_sweeps_then_the_change_is_reported_once_and_not_again()
    {
        // Given
        let (registry, probe) = observed_registry("/dev/ash");
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();
        registry.changes().unwrap(); // la passe qui découvre l'onglet
        probe.move_to("/tmp");

        // When
        let moved = registry.changes().unwrap();
        let settled = registry.changes().unwrap();

        // Then — c'est ce qui fait suivre le titre de l'onglet, sans réveiller la webview
        // trois fois par seconde pour un onglet posé à son invite
        assert_eq!(
            moved,
            vec![TabChange {
                tab_id: "A".to_owned(),
                cwd: "/tmp".to_owned(),
            }]
        );
        assert_eq!(settled, vec![]);
    }

    #[test]
    fn given_a_probe_pass_in_flight_when_a_keystroke_arrives_then_it_does_not_wait_behind_the_probe(
    ) {
        // Given — une sonde qui bloque, comme un `proc_pidinfo` sur un système chargé.
        // À 3 Hz par onglet, une passe qui tient le verrou du registre met chaque frappe
        // de l'utilisateur derrière elle.
        let (entered, has_entered) = mpsc::channel();
        let (release, wait_for_release) = mpsc::channel::<()>();
        let registry = Arc::new(PtyRegistry::new(
            Box::new(FakeSpawner::observable()),
            Arc::new(BlockingProbe {
                entered,
                release: Mutex::new(wait_for_release),
            }),
        ));
        registry.open(spec(), "A".to_owned()).unwrap();

        let sweeping = {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || registry.changes())
        };
        has_entered.recv().unwrap();

        // When — la frappe part pendant que la passe de sonde est bloquée
        let (typed, keystroke) = mpsc::channel();
        let typing = {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || {
                let written = registry.write("A", b"ls\n");
                typed.send(written).unwrap();
            })
        };

        // Then — elle aboutit sans attendre la fin de la passe
        let written = keystroke.recv_timeout(std::time::Duration::from_secs(5));
        release.send(()).unwrap();
        sweeping.join().unwrap().unwrap();
        typing.join().unwrap();
        assert!(
            matches!(written, Ok(Ok(()))),
            "la frappe a attendu la fin de la sonde : {written:?}"
        );
    }

    /// Une sonde qui prévient qu'elle est entrée, puis attend qu'on la libère.
    ///
    /// Elle ne décrit aucun système réel : ce qu'elle rend visible, c'est la durée d'une
    /// passe, et ce qui reste bloqué pendant.
    struct BlockingProbe {
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl Probe for BlockingProbe {
        fn foreground_pgid(&self, _terminal: RawFd) -> Result<Pid, ProbeError> {
            self.entered.send(()).unwrap();
            let _ = self.release.lock().unwrap().recv();
            Ok(100)
        }

        fn inspect(&self, pid: Pid) -> Result<ProcessInfo, ProbeError> {
            Ok(ProcessInfo {
                pid,
                name: "bash".to_owned(),
                cwd: PathBuf::from("/dev/ash"),
            })
        }
    }
}
