//! La boucle de sonde d'[ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md).
//!
//! « Pour chaque onglet, toutes les ~300 ms. » Une seule boucle sonde tous les onglets :
//! l'ADR demande une sonde par onglet, pas un thread par onglet, et deux appels système
//! par onglet ne justifient pas d'en réveiller dix.
//!
//! Ce que la boucle annonce, elle le tient du registre — jamais d'un état qu'elle
//! tiendrait elle-même. C'est le backend qui détient le `cwd` d'un onglet ; le frontend
//! le rend ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Weak;
use std::time::Duration;

use super::registry::{PtyRegistry, TabInfo};

/// Cadence de la boucle. L'ADR dit « ~300 ms », et le critère d'acceptation de l'issue
/// laisse 400 ms à l'utilisateur : la marge sert à la passe elle-même, pas à dormir plus.
pub const PROBE_PERIOD: Duration = Duration::from_millis(300);

/// L'attente entre deux passes, derrière un trait.
///
/// Une boucle qui dort sur `thread::sleep` en dur ne se vérifie qu'en dormant vraiment :
/// ses conditions d'arrêt deviendraient alors un test à 300 ms la passe, ou pas de test
/// du tout.
pub trait Ticker: Send + Sync {
    fn wait(&self, period: Duration);
}

/// L'horloge réelle.
pub struct SystemTicker;

impl Ticker for SystemTicker {
    fn wait(&self, period: Duration) {
        std::thread::sleep(period);
    }
}

/// L'ordre d'arrêt, partagé avec le composition root.
///
/// L'application qui quitte le pose ; la boucle le lit à chaque passe. Sans lui, l'arrêt
/// ne serait qu'un effet de bord de la fin du processus — c'est-à-dire rien du tout le
/// jour où la même bibliothèque tournera sous le démon `ashd` d'ADR-0009.
#[derive(Default)]
pub struct Shutdown(AtomicBool);

impl Shutdown {
    /// Demande l'arrêt. La boucle s'arrête au plus tard une passe plus tard.
    pub fn ask(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn asked(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// La boucle elle-même. Bloquante : c'est son thread qui la porte.
///
/// Elle s'arrête à l'arrêt demandé, et si le registre a disparu — la boucle observe les
/// onglets, elle ne doit jamais être ce qui les maintient en vie. D'où le [`Weak`].
///
/// `settle` reçoit, à **chaque** passe, les racines de worktree où vit un onglet. Deux
/// raisons pour que ce ne soit pas `announce` qui s'en charge : la fermeture du dernier
/// onglet d'un worktree ne produit aucun changement à annoncer — et c'est pourtant le
/// moment où il faut cesser de le surveiller — et la question ne coûte rien, la réponse
/// étant déjà en mémoire (voir [`PtyRegistry::worktree_roots`]).
pub fn run(
    registry: &Weak<PtyRegistry>,
    ticker: &dyn Ticker,
    shutdown: &Shutdown,
    announce: &dyn Fn(Vec<TabInfo>),
    settle: &dyn Fn(Vec<String>),
) {
    while !shutdown.asked() {
        let Some(registry) = registry.upgrade() else {
            return;
        };

        // Un registre empoisonné n'a rien à annoncer, et un thread de fond n'a personne à
        // qui remonter une erreur : la passe suivante retentera.
        let changes = registry.changes().unwrap_or_default();
        let roots = registry.worktree_roots().unwrap_or_default();

        // Le registre est relâché avant l'attente, et avant l'émission : rien ne doit
        // survivre à la fermeture d'un onglet parce que la boucle dormait.
        drop(registry);

        if !changes.is_empty() {
            announce(changes);
        }
        settle(roots);

        ticker.wait(PROBE_PERIOD);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::pty::fakes::{observed_registry, registry, FakeSpawner, SpecBuilder};
    use crate::features::pty::TabId;
    use std::sync::{Arc, Mutex};

    /// Une horloge qui ne dort pas, et qui décide quand la boucle s'arrête.
    ///
    /// Le nombre de passes est ce que le test choisit ; sans ça, vérifier qu'une boucle
    /// s'arrête coûterait une seconde par test.
    struct FakeTicker {
        passes: Mutex<u32>,
        stop_after: u32,
        shutdown: Arc<Shutdown>,
    }

    impl FakeTicker {
        fn stopping_after(passes: u32, shutdown: &Arc<Shutdown>) -> Self {
            Self {
                passes: Mutex::new(0),
                stop_after: passes,
                shutdown: Arc::clone(shutdown),
            }
        }

        fn passes(&self) -> u32 {
            *self.passes.lock().unwrap()
        }
    }

    impl Ticker for FakeTicker {
        fn wait(&self, _period: Duration) {
            let mut passes = self.passes.lock().unwrap();
            *passes += 1;
            if *passes >= self.stop_after {
                self.shutdown.ask();
            }
        }
    }

    /// Ce que la boucle a poussé vers le frontend.
    #[derive(Default)]
    struct Announced(Mutex<Vec<Vec<TabInfo>>>);

    impl Announced {
        fn batches(&self) -> Vec<Vec<TabInfo>> {
            self.0.lock().unwrap().clone()
        }
    }

    #[test]
    fn given_a_tab_whose_shell_moved_when_the_loop_makes_its_passes_then_the_change_is_pushed_once()
    {
        // Given — un onglet parti de /dev/ash, et un `cd` entre deux passes
        let (registry, probe) = observed_registry("/dev/ash");
        let registry = Arc::new(registry);
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".into(),
            )
            .unwrap();
        let shutdown = Arc::new(Shutdown::default());
        let announced = Announced::default();
        let ticker = FakeTicker::stopping_after(3, &shutdown);
        probe.move_to("/tmp");

        // When — trois passes : celle qui découvre le déplacement, et deux qui suivent
        run(
            &Arc::downgrade(&registry),
            &ticker,
            &shutdown,
            &|changes| announced.0.lock().unwrap().push(changes),
            &|_| {},
        );

        // Then — le titre de l'onglet suit le `cd` sans qu'aucun onglet ne soit ouvert ni
        // fermé, et un onglet immobile ne réveille pas la webview pour rien
        let pushed: Vec<Vec<(TabId, String)>> = announced
            .batches()
            .into_iter()
            .map(|batch| batch.into_iter().map(|tab| (tab.tab_id, tab.cwd)).collect())
            .collect();
        assert_eq!(pushed, vec![vec![("A".to_owned(), "/tmp".to_owned())]]);
    }

    #[test]
    fn given_a_tab_that_stopped_moving_when_the_loop_makes_a_pass_then_its_worktree_is_still_reported_as_inhabited(
    ) {
        // Given — un onglet posé à son invite : il n'a plus rien à annoncer, et pourtant
        // son worktree reste celui qu'il faut surveiller. C'est la fermeture du dernier
        // onglet, elle non plus sans changement à annoncer, qui doit relâcher la
        // surveillance git (spec §5.3).
        let (registry, _probe) = observed_registry("/dev/ash");
        let registry = Arc::new(registry);
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".into(),
            )
            .unwrap();
        let shutdown = Arc::new(Shutdown::default());
        let ticker = FakeTicker::stopping_after(2, &shutdown);
        let settled: Mutex<Vec<Vec<String>>> = Mutex::new(Vec::new());

        // When — deux passes, sans le moindre `cd`
        run(
            &Arc::downgrade(&registry),
            &ticker,
            &shutdown,
            &|_| {},
            &|roots| settled.lock().unwrap().push(roots),
        );

        // Then — chaque passe dit où vivent les onglets, changement ou pas
        let settled = settled.lock().unwrap().clone();
        assert_eq!(settled.len(), 2);
        assert_eq!(settled[0], vec!["/dev/ash".to_owned()]);
    }

    #[test]
    fn given_the_application_quits_when_the_loop_finishes_its_pass_then_it_stops_probing() {
        // Given — un onglet fermé dont la sonde continuerait de lire un `fd` recyclé est
        // un bug de durée de vie ; à l'échelle de l'application, c'est cette boucle-là
        let (registry, _probe) = observed_registry("/dev/ash");
        let registry = Arc::new(registry);
        let shutdown = Arc::new(Shutdown::default());
        let ticker = FakeTicker::stopping_after(2, &shutdown);

        // When
        run(
            &Arc::downgrade(&registry),
            &ticker,
            &shutdown,
            &|_| {},
            &|_| {},
        );

        // Then — la boucle rend la main, et ne sonde plus après l'ordre d'arrêt
        assert_eq!(ticker.passes(), 2);
    }

    #[test]
    fn given_the_registry_is_gone_when_the_loop_wakes_up_then_it_stops_without_being_asked_to() {
        // Given — la boucle observe les onglets, elle ne doit pas être ce qui les tient
        let registry = Arc::new(registry(FakeSpawner::observable()));
        let weak = Arc::downgrade(&registry);
        drop(registry);
        let shutdown = Arc::new(Shutdown::default());
        let ticker = FakeTicker::stopping_after(u32::MAX, &shutdown);

        // When
        run(&weak, &ticker, &shutdown, &|_| {}, &|_| {});

        // Then — elle n'a même pas attendu : il n'y avait plus rien à sonder
        assert_eq!(ticker.passes(), 0);
    }
}
