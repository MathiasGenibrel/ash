use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;

use super::decode::Utf8Stitcher;
use super::error::PtyError;
use super::registry::{Opened, PtyRegistry, TabId};
use super::session::PtySpec;

/// Taille d'une lecture. Un master PTY macOS ne rend guère plus par appel.
const READ_BUFFER: usize = 64 * 1024;

/// Ce que le PTY envoie à la webview.
#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PtyFrame {
    /// Sortie du shell, déjà recollée en UTF-8 valide.
    Chunk { data: String },
    /// Le shell est sorti. `code` est absent s'il a été tué par un signal.
    Exit { code: Option<i32> },
}

/// Ouvre un onglet : un PTY, un shell, un lecteur.
///
/// Rend l'identifiant d'onglet — un ulid, que le shell voit dans `ASH_TAB_ID`.
#[tauri::command]
pub fn pty_open(
    registry: tauri::State<'_, Arc<PtyRegistry>>,
    channel: Channel<PtyFrame>,
    cols: u16,
    rows: u16,
) -> Result<TabId, PtyError> {
    let tab_id = ulid::Ulid::generate().to_string();

    let opened = registry.open(
        PtySpec {
            shell: default_shell(),
            cwd: home_directory(),
            cols,
            rows,
            // `ASH_SOCK` est posé dès maintenant, même si rien ne l'écoute encore : le
            // shell d'un onglet ne doit pas avoir à être relancé quand le socket
            // d'events arrivera. `ASH_TAB_ID` est ajouté par le registre.
            env: vec![("ASH_SOCK".to_owned(), socket_path().display().to_string())],
        },
        tab_id,
    )?;

    let tab_id = opened.tab_id.clone();
    spawn_reader(opened, channel, Arc::clone(&registry));
    Ok(tab_id)
}

/// Envoie une frappe au shell.
#[tauri::command]
pub fn pty_write(
    registry: tauri::State<'_, Arc<PtyRegistry>>,
    tab_id: String,
    data: String,
) -> Result<(), PtyError> {
    registry.write(&tab_id, data.as_bytes())
}

/// Redimensionne le PTY, ce qui poste un `SIGWINCH` au groupe en avant-plan.
#[tauri::command]
pub fn pty_resize(
    registry: tauri::State<'_, Arc<PtyRegistry>>,
    tab_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), PtyError> {
    registry.resize(&tab_id, cols, rows)
}

/// Acquitte un morceau : xterm.js a fini de l'écrire.
///
/// Sans cet acquittement, le lecteur s'arrête au bout de la fenêtre. C'est voulu — voir
/// [`super::flow`].
#[tauri::command]
pub fn pty_ack(
    registry: tauri::State<'_, Arc<PtyRegistry>>,
    tab_id: String,
) -> Result<(), PtyError> {
    registry.ack(&tab_id)
}

/// Ferme un onglet et termine son shell.
#[tauri::command]
pub fn pty_close(
    registry: tauri::State<'_, Arc<PtyRegistry>>,
    tab_id: String,
) -> Result<(), PtyError> {
    registry.close(&tab_id)
}

/// Lit le PTY jusqu'à sa fin, en respectant la contre-pression de la webview.
fn spawn_reader(opened: Opened, channel: Channel<PtyFrame>, registry: Arc<PtyRegistry>) {
    let Opened {
        tab_id,
        mut reader,
        credits,
    } = opened;

    // Un thread par onglet, bloquant : `read()` sur un master PTY n'a pas d'équivalent
    // asynchrone portable, et un onglet coûte alors un thread endormi la plupart du temps.
    std::thread::spawn(move || {
        let mut buffer = vec![0u8; READ_BUFFER];
        let mut stitcher = Utf8Stitcher::default();

        loop {
            // Le crédit est pris **avant** la lecture : quand la webview est en retard,
            // le PTY se remplit et c'est le programme qui écrit dedans qui se bloque.
            // C'est la contre-pression du système, pas une file qui gonfle en mémoire.
            if !credits.acquire() {
                return;
            }

            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let text = stitcher.push(&buffer[..read]);
                    if text.is_empty() {
                        // Amorce UTF-8 incomplète : rien à afficher, donc rien à
                        // acquitter — on rend le crédit nous-mêmes.
                        credits.release();
                        continue;
                    }
                    if channel.send(PtyFrame::Chunk { data: text }).is_err() {
                        return;
                    }
                }
                Err(_) => break,
            }
        }

        let tail = stitcher.flush();
        if !tail.is_empty() {
            let _ = channel.send(PtyFrame::Chunk { data: tail });
        }

        // Le shell est sorti de lui-même : l'onglet disparaît du registre, et la webview
        // l'apprend. Le code de sortie viendra avec la machine à états des agents.
        registry.forget(&tab_id);
        let _ = channel.send(PtyFrame::Exit { code: None });
    });
}

/// Le shell de l'utilisateur, pas celui d'Ash.
///
/// `SHELL` est ce que le système de connexion a posé ; s'en écarter donnerait à
/// l'utilisateur un shell qui n'est pas le sien, avec une configuration qui n'est pas la
/// sienne.
fn default_shell() -> PathBuf {
    std::env::var_os("SHELL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bin/zsh"))
}

fn home_directory() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Chemin du socket d'events. Le socket lui-même appartient à une autre tâche.
fn socket_path() -> PathBuf {
    home_directory().join(".ash").join("ash.sock")
}
