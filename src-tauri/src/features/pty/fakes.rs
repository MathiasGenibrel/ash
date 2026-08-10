//! Les effets système de la feature, remplacés par ce que le test décrit.
//!
//! Ces doublures sont partagées par les tests du registre et par ceux de la boucle de
//! sonde : les deux ont besoin d'un registre complet — un onglet ne s'ouvre pas sans
//! spawner, et ne se sonde pas sans sonde — et les dupliquer les ferait diverger.
//!
//! Aucun processus n'est lancé ici. Les tests d'intégration, eux, utilisent les vraies
//! implémentations sur un vrai shell (`tests/pty_real_shell.rs`, `tests/probe_real_shell.rs`).

use std::os::fd::RawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::error::PtyError;
use super::registry::PtyRegistry;
use super::session::{OpenPty, PtySession, PtySpawner, PtySpec, Terminal};
use crate::features::probe::{Pid, Probe, ProbeError, ProcessInfo};

/// Le terminal que les faux onglets annoncent. Aucun appel système ne le touche : le
/// [`FakeProbe`] répond sans regarder.
pub const TERMINAL: Terminal = Terminal {
    master_fd: 7,
    shell_pid: 100,
};

/// Une sonde qui répond ce qu'on lui a dit — et qu'on peut faire bouger.
///
/// Le répertoire est derrière un verrou parce que c'est ce que le scénario de la boucle
/// demande : sonder, faire bouger le processus observé, sonder à nouveau. Sans ça, un
/// faux backend qui rend toujours la même valeur ne distingue pas « répertoire courant »
/// de « répertoire de départ », et laisse passer les bugs qui vivent exactement là.
#[derive(Default)]
pub struct FakeProbe {
    cwd: Mutex<Option<PathBuf>>,
}

impl FakeProbe {
    /// Un système qui ne répond rien : ni avant-plan, ni processus.
    pub fn silent() -> Self {
        Self::default()
    }

    pub fn reporting(cwd: &str) -> Self {
        Self {
            cwd: Mutex::new(Some(PathBuf::from(cwd))),
        }
    }

    /// Le processus observé a changé de répertoire — un `cd`, ou un programme lancé
    /// ailleurs. C'est ce que la sonde doit voir à la passe suivante.
    pub fn move_to(&self, cwd: &str) {
        *self.cwd.lock().unwrap() = Some(PathBuf::from(cwd));
    }

    fn seen(&self) -> Option<PathBuf> {
        self.cwd.lock().unwrap().clone()
    }
}

impl Probe for FakeProbe {
    fn foreground_pgid(&self, terminal: RawFd) -> Result<Pid, ProbeError> {
        self.seen()
            .map(|_| TERMINAL.shell_pid)
            .ok_or(ProbeError::NoForeground(terminal))
    }

    fn inspect(&self, pid: Pid) -> Result<ProcessInfo, ProbeError> {
        self.seen()
            .map(|cwd| ProcessInfo {
                pid,
                name: "bash".to_owned(),
                cwd,
            })
            .ok_or(ProbeError::Vanished(pid))
    }
}

#[derive(Default)]
pub struct FakeSession {
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
pub struct FakeSpawner {
    pub killed: Arc<AtomicBool>,
    pub written: Arc<Mutex<Vec<u8>>>,
    pub resized: Arc<Mutex<Vec<(u16, u16)>>>,
    pub spawns: Arc<AtomicUsize>,
    pub last_env: Arc<Mutex<Vec<(String, String)>>>,
    pub foreground: Arc<AtomicBool>,
    /// Les onglets ouverts par ce spawner sont-ils sondables ?
    pub observable: bool,
}

impl FakeSpawner {
    /// Un spawner dont les onglets exposent un terminal : sans ça, le registre s'en tient
    /// au répertoire de départ et aucune sonde ne tourne.
    pub fn observable() -> Self {
        Self {
            observable: true,
            ..Self::default()
        }
    }
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

/// Test Data Builder : un `PtySpec` valide et déterministe, dont on ne surcharge que ce
/// que le scénario regarde.
pub struct SpecBuilder {
    cwd: PathBuf,
}

impl SpecBuilder {
    pub fn new() -> Self {
        Self { cwd: "/tmp".into() }
    }

    pub fn starting_in(mut self, cwd: &str) -> Self {
        self.cwd = cwd.into();
        self
    }

    pub fn build(self) -> PtySpec {
        PtySpec {
            shell: "/bin/bash".into(),
            cwd: self.cwd,
            cols: 80,
            rows: 24,
            env: vec![("ASH_SOCK".to_owned(), "/tmp/ash.sock".to_owned())],
        }
    }
}

pub fn spec() -> PtySpec {
    SpecBuilder::new().build()
}

/// Un registre dont les onglets ne sont pas sondables : la plupart des règles du registre
/// — ordre, fermeture, crédits — n'ont rien à voir avec la sonde.
pub fn registry(spawner: FakeSpawner) -> PtyRegistry {
    PtyRegistry::new(Box::new(spawner), Arc::new(FakeProbe::silent()))
}

/// Un registre sondable, et la sonde qu'on garde en main pour la faire bouger.
pub fn observed_registry(cwd: &str) -> (PtyRegistry, Arc<FakeProbe>) {
    let probe = Arc::new(FakeProbe::reporting(cwd));
    let registry = PtyRegistry::new(
        Box::new(FakeSpawner::observable()),
        Arc::clone(&probe) as Arc<dyn Probe>,
    );
    (registry, probe)
}
