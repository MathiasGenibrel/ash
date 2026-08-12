use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Runtime};

use super::decode::Utf8Stitcher;
use super::error::PtyError;
use super::registry::{Opened, PtyRegistry, TabId, TabInfo};
use super::session::PtySpec;
use super::sweep::{self, Shutdown, SystemTicker};

/// Taille d'une lecture. Un master PTY macOS ne rend guère plus par appel.
const READ_BUFFER: usize = 64 * 1024;

/// Nom de l'event qui porte les onglets qui ont bougé.
///
/// Contrat avec `src/features/terminal/pty-bridge.ts` : une chaîne que rien ne vérifie à
/// la compilation, comme celle du menu. Le frontend ne connaît de la feature que ses
/// commandes, ce nom, et les types qui traversent.
pub const TAB_CHANGED_EVENT: &str = "ash://tab-changed";

/// Ce que le PTY envoie à la webview.
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PtyFrame {
    /// Sortie du shell, déjà recollée en UTF-8 valide.
    Chunk { data: String },
    /// Le shell est sorti. `code` est absent s'il a été tué par un signal.
    Exit { code: Option<i32> },
}

/// Ouvre un onglet : un PTY, un shell, un lecteur.
///
/// `cwd` absent vaut `~` — c'est le `Cmd+Shift+T` de la spec §4.4. `Cmd+T`, lui, passe
/// le répertoire de départ de l'onglet actif, que `pty_tabs` lui a rendu.
///
/// Rend l'identifiant d'onglet — un ulid, que le shell voit dans `ASH_TAB_ID`.
#[tauri::command]
pub fn pty_open(
    registry: tauri::State<'_, Arc<PtyRegistry>>,
    channel: Channel<PtyFrame>,
    cols: u16,
    rows: u16,
    cwd: Option<String>,
) -> Result<TabId, PtyError> {
    let tab_id = ulid::Ulid::generate().to_string();

    let opened = registry.open(
        PtySpec {
            shell: default_shell(),
            cwd: cwd.map_or_else(home_directory, PathBuf::from),
            cols,
            rows,
            // L'adresse du socket d'events appartient à `agents`, la feature qui écoute :
            // `pty` la lui demande plutôt que d'en garder une copie qui pourrait dériver.
            // `ASH_TAB_ID` est ajouté par le registre, et c'est par lui — et rien d'autre —
            // que les events seront corrélés
            // ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
            env: vec![(
                "ASH_SOCK".to_owned(),
                crate::features::agents::socket_path().display().to_string(),
            )],
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

/// Les onglets vivants, dans l'ordre que le backend détient.
///
/// C'est cet ordre — pas celui du DOM — que `Cmd+1..9` numérote. Le frontend le relit
/// après chaque ouverture et chaque fermeture plutôt que d'en tenir une copie qu'il
/// ferait évoluer de son côté ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[tauri::command]
pub fn pty_tabs(registry: tauri::State<'_, Arc<PtyRegistry>>) -> Result<Vec<TabInfo>, PtyError> {
    registry.tabs()
}

/// Quelque chose tourne-t-il dans cet onglet ?
///
/// La question que `Cmd+W` pose avant de fermer. Elle ne dit pas *quoi* : nommer le
/// processus est le travail de la sonde d'ADR-0005, et l'« agent » de la spec §4.4
/// n'existera qu'au jalon J2.
#[tauri::command]
pub fn pty_has_foreground_process(
    registry: tauri::State<'_, Arc<PtyRegistry>>,
    tab_id: String,
) -> Result<bool, PtyError> {
    registry.has_foreground_process(&tab_id)
}

/// Ferme un onglet et termine son shell.
#[tauri::command]
pub fn pty_close(
    registry: tauri::State<'_, Arc<PtyRegistry>>,
    tab_id: String,
) -> Result<(), PtyError> {
    registry.close(&tab_id)
}

/// Démarre la boucle de sonde d'ADR-0005, et rend de quoi l'arrêter.
///
/// Le frontend ne demande rien : c'est le backend qui pousse, parce que c'est lui qui
/// détient l'état ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Un
/// `setInterval` côté webview aurait fait vivre la cadence du côté qui ne détient rien.
///
/// Un seul thread pour tous les onglets, et seuls les changements traversent la frontière.
///
/// `inhabited` reçoit à chaque passe les racines de worktree habitées par un onglet. C'est
/// le **rattachement** de la spec §5.3 : le composition root s'en sert pour aligner la
/// surveillance git sur les worktrees réellement ouverts. La feature `pty` ne connaît pas
/// `git` — elle passe une liste de chaînes, et ne sait pas qui la lit.
pub fn watch_tabs<R: Runtime>(
    app: AppHandle<R>,
    registry: &Arc<PtyRegistry>,
    inhabited: impl Fn(Vec<String>) + Send + 'static,
) -> Arc<Shutdown> {
    let shutdown = Arc::new(Shutdown::default());

    let stop = Arc::clone(&shutdown);
    let registry = Arc::downgrade(registry);
    std::thread::spawn(move || {
        sweep::run(
            &registry,
            &SystemTicker,
            &stop,
            &|changes| {
                // Échouer à émettre signifie qu'il n'y a plus de webview à prévenir : rien
                // à rattraper, et surtout pas de panique dans un thread de fond.
                let _ = app.emit(TAB_CHANGED_EVENT, changes);
            },
            &inhabited,
        );
    });

    shutdown
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
