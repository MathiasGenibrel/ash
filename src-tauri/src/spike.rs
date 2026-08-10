//! Banc de mesure du spike xterm.js — **code jetable**.
//!
//! Il n'appartient à aucune feature et ne doit pas servir de modèle. Son seul rôle est
//! de pousser de la sortie de terminal réaliste vers le frontend par le chemin que le
//! vrai PTY empruntera — un `Channel` Tauri — puis de recueillir les mesures.
//!
//! Mesurer depuis le frontend seul aurait donné le débit du moteur de rendu ; ce qui
//! nous intéresse est le débit de la chaîne complète, IPC comprise.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tokio::sync::Semaphore;

/// Taille des morceaux poussés dans le canal.
///
/// Un PTY rend au plus la taille de son tampon par lecture — 64 Kio est l'ordre de
/// grandeur de ce que `read()` sur un master PTY macOS rend sous forte charge.
const CHUNK: usize = 64 * 1024;

/// Sortie de test verbeuse : le cas cité par la spec. Lignes courtes, beaucoup de
/// retours chariot, un peu de couleur.
fn line_test(i: usize) -> String {
    let mut s = String::with_capacity(96);
    if i.is_multiple_of(7) {
        let _ = write!(
            s,
            "\x1b[32m✓\x1b[0m src/features/agents/state.test.ts > transition {i}\r\n"
        );
    } else {
        let _ = write!(
            s,
            "  ok {i} — given a working agent, when the hook fires\r\n"
        );
    }
    s
}

/// `cat` d'un fichier source : lignes plus longues, aucune séquence d'échappement.
fn line_cat(i: usize) -> String {
    format!("{i:6}\tlet resolved = registry.lookup(candidate).unwrap_or_default(); // {i}\r\n")
}

/// Sortie fortement colorée : le pire cas pour l'analyseur ANSI.
fn line_color(i: usize) -> String {
    let mut s = String::with_capacity(160);
    for c in 0..8 {
        let _ = write!(s, "\x1b[3{c};4{}m {i:04} \x1b[0m", (c + 4) % 8);
    }
    s.push_str("\r\n");
    s
}

/// Profil de charge demandé par le frontend.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Workload {
    /// `bun test` verbeux.
    Test,
    /// `cat` d'un gros fichier.
    Cat,
    /// Sortie saturée de couleurs.
    Color,
}

impl Workload {
    fn render(self, i: usize) -> String {
        match self {
            Workload::Test => line_test(i),
            Workload::Cat => line_cat(i),
            Workload::Color => line_color(i),
        }
    }
}

/// Ce que le canal transporte : un morceau, ou la fin du flux.
#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Frame {
    Chunk { data: String },
    Done { bytes: usize, lines: usize },
}

/// Nombre de morceaux qui peuvent être en vol sans acquittement.
///
/// **Pourquoi il en faut un.** Le premier jet du banc poussait tout sans attendre :
/// au-delà de 50 Mo de données non consommées, `Terminal.write()` lève
/// « write data discarded, use flow control to avoid losing data » et jette la sortie.
/// Ce n'est pas une bizarrerie du banc — c'est le régime qu'un `cat` d'un gros fichier
/// impose à un vrai PTY.
///
/// Huit morceaux de 64 Kio font 512 Kio en vol : assez pour que le canal ne soit jamais
/// à sec, cent fois sous le seuil.
const WINDOW: usize = 8;

/// Crédits d'émission de la diffusion en cours.
///
/// Recréé à chaque `spike_stream` : une mesure ne doit rien hériter de la précédente.
#[derive(Default)]
pub struct Flow(std::sync::Mutex<Option<Arc<Semaphore>>>);

/// Pousse `lines` lignes du profil demandé dans le canal, par morceaux de 64 Kio, en
/// respectant la fenêtre d'acquittement.
///
/// Aucune temporisation par ailleurs : on cherche le plafond, pas un rythme réaliste.
#[tauri::command]
pub async fn spike_stream(
    channel: Channel<Frame>,
    flow: tauri::State<'_, Flow>,
    workload: Workload,
    lines: usize,
) -> Result<(), String> {
    let credits = Arc::new(Semaphore::new(WINDOW));
    *flow.0.lock().map_err(|_| "verrou de flux empoisonné")? = Some(Arc::clone(&credits));

    let mut buffer = String::with_capacity(CHUNK + 256);
    let mut bytes = 0usize;

    // Le permis est consommé à l'émission et rendu par `spike_ack`, pas à la fin de
    // cette portée : c'est le frontend qui décide quand il a digéré le morceau.
    let send = async |data: String| -> Result<(), String> {
        Arc::clone(&credits)
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?
            .forget();
        channel
            .send(Frame::Chunk { data })
            .map_err(|e| e.to_string())
    };

    for i in 0..lines {
        buffer.push_str(&workload.render(i));
        if buffer.len() >= CHUNK {
            bytes += buffer.len();
            send(std::mem::take(&mut buffer)).await?;
            buffer.reserve(CHUNK + 256);
        }
    }

    if !buffer.is_empty() {
        bytes += buffer.len();
        send(buffer).await?;
    }

    channel
        .send(Frame::Done { bytes, lines })
        .map_err(|e| e.to_string())
}

/// Rend un crédit d'émission : le frontend a fini d'écrire un morceau dans xterm.js.
#[tauri::command]
pub fn spike_ack(flow: tauri::State<'_, Flow>) -> Result<(), String> {
    if let Some(credits) = flow
        .0
        .lock()
        .map_err(|_| "verrou de flux empoisonné")?
        .as_ref()
    {
        credits.add_permits(1);
    }
    Ok(())
}

/// Écrit le rapport de mesure à côté du crate, pour qu'il soit lisible hors de la
/// fenêtre — un chiffre lu sur une capture d'écran n'est pas une mesure.
#[tauri::command]
pub fn spike_report(report: serde_json::Value) -> Result<String, String> {
    let path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("le crate n'a pas de dossier parent")?
        .join("spike-results.json");

    let pretty = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(&path, pretty).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}
