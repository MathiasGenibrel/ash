use std::io::Read;
use std::path::PathBuf;

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

use super::error::PtyError;

/// Ce qu'il faut pour ouvrir un PTY.
#[derive(Debug, Clone)]
pub struct PtySpec {
    pub shell: PathBuf,
    pub cwd: PathBuf,
    pub cols: u16,
    pub rows: u16,
    /// Variables ajoutées à celles héritées — `ASH_TAB_ID` et `ASH_SOCK`.
    pub env: Vec<(String, String)>,
}

/// Un PTY vivant, vu par le reste de la feature.
pub trait PtySession: Send {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError>;
    fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError>;
    /// Termine le processus. Idempotent : fermer deux fois n'est pas une erreur.
    fn kill(&mut self) -> Result<(), PtyError>;
    /// Vrai quand autre chose que le shell tient l'avant-plan du terminal.
    ///
    /// C'est ce qui distingue « une invite de commande vide » de « une commande, ou un
    /// agent, est en train de tourner » — donc de ce qu'un `Cmd+W` détruirait.
    ///
    /// Le trait est le point d'extension, pas `portable-pty` : sans lui, la règle de
    /// confirmation ne serait vérifiable qu'en lançant un vrai processus.
    fn has_foreground_process(&mut self) -> Result<bool, PtyError>;
}

/// Ce qu'ouvrir un PTY produit : de quoi le piloter, et de quoi le lire.
///
/// Les deux sont séparés parce qu'ils vivent dans des threads différents — le lecteur
/// bloque sur `read()` pendant que l'interface écrit et redimensionne.
pub type OpenPty = (Box<dyn PtySession>, Box<dyn Read + Send>);

/// L'effet système, derrière un trait que la feature possède.
///
/// C'est ce qui permet de tester les règles — registre, contre-pression, fermeture —
/// sans lancer de `bash`. Les tests d'intégration, eux, utilisent la vraie implémentation.
pub trait PtySpawner: Send + Sync + 'static {
    fn spawn(&self, spec: &PtySpec) -> Result<OpenPty, PtyError>;
}

/// Implémentation réelle, sur `portable-pty`.
#[derive(Default)]
pub struct SystemPtySpawner;

struct SystemSession {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn std::io::Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// Le pid du shell, retenu au lancement.
    ///
    /// Retenu, et pas redemandé : une fois le fils moissonné, `process_id()` ne rend plus
    /// rien, et c'est justement à ce moment-là qu'on compare des pids.
    child_pid: Option<i32>,
}

impl PtySession for SystemSession {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        use std::io::Write as _;
        self.writer
            .write_all(bytes)
            .and_then(|()| self.writer.flush())
            .map_err(|e| PtyError::Io(e.to_string()))
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        // `resize` sur le master déclenche le `SIGWINCH` que les TUI attendent : c'est
        // le noyau qui le poste au groupe de processus en avant-plan, pas nous.
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Io(e.to_string()))
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        // Un processus déjà mort rend une erreur système ; fermer un onglet dont le
        // shell vient de sortir est un cas nominal, pas une faute.
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return Ok(());
        }
        self.child.kill().map_err(|e| PtyError::Io(e.to_string()))
    }

    fn has_foreground_process(&mut self) -> Result<bool, PtyError> {
        // Un shell sorti ne tient plus rien en avant-plan, et son pid a pu être recyclé :
        // la comparaison ci-dessous n'aurait plus de sens.
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return Ok(false);
        }

        // `process_group_leader` lit `tcgetpgrp(master)` : le groupe de processus en
        // avant-plan du terminal. Le shell, lancé comme chef de sa propre session, a un
        // pgid égal à son pid. Les deux diffèrent ⇒ le shell a passé la main à quelqu'un.
        //
        // Cette voie est volontairement pauvre : elle dit « quelque chose tourne », pas
        // *quoi*. Nommer le processus en avant-plan est le travail de la sonde `libproc`
        // ([ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md)), qui vit dans sa
        // propre feature — c'est aussi elle qui dira, au jalon J2, s'il s'agit d'un agent.
        let (Some(leader), Some(shell)) = (self.master.process_group_leader(), self.child_pid)
        else {
            // Le système ne sait pas répondre. Prétendre qu'il ne tourne rien ferait
            // fermer un onglet sans confirmation ; on préfère se taire que mentir dans
            // ce sens-là.
            return Ok(true);
        };

        Ok(leader != shell)
    }
}

impl PtySpawner for SystemPtySpawner {
    fn spawn(&self, spec: &PtySpec) -> Result<OpenPty, PtyError> {
        let pair = NativePtySystem::default()
            .openpty(PtySize {
                rows: spec.rows,
                cols: spec.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        let mut command = CommandBuilder::new(&spec.shell);
        command.cwd(&spec.cwd);
        for (key, value) in &spec.env {
            command.env(key, value);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::Spawn(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        // Le côté esclave doit être relâché ici. Tant qu'on le garde ouvert, le master
        // ne verra jamais l'EOF quand le shell sort, et l'onglet resterait vivant après
        // un `exit`.
        drop(pair.slave);

        let child_pid = child.process_id().and_then(|pid| i32::try_from(pid).ok());

        let session = SystemSession {
            master: pair.master,
            writer,
            child,
            child_pid,
        };

        Ok((Box::new(session), reader))
    }
}
