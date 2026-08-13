//! Tests d'intégration d'`ash-event` — le vrai binaire, lancé comme un hook le lance.
//!
//! Les tests unitaires du binaire vérifient ce qu'il **décide** : ce qu'il tire d'un objet
//! de hook, et ce qu'il abandonne quand la trame déborde. Ceux-ci vérifient ce qu'aucun
//! double ne peut prouver — que le processus, avec sa vraie entrée standard, écrit
//! réellement sur un socket unix et **rend la main**. C'est la seule façon de mettre à
//! l'épreuve le cas qui coûterait le plus cher : une entrée standard promise et jamais
//! écrite, qui retiendrait l'agent aussi longtemps qu'elle se tait
//! ([ADR-0007](../../docs/adr/0007-etats-par-hooks.md)).
//!
//! Rien d'Ash n'est lancé ici, et aucun agent non plus : un socket, un processus, une
//! ligne.

// `expect` est la façon normale d'échouer dans un test, et `clippy.toml` l'autorise déjà à
// ce titre — mais seulement à l'intérieur des `#[test]`. Clippy ne reconnaît pas comme « du
// test » les fonctions d'aide d'un test d'intégration ; on le dit donc ici, pour ce fichier
// seulement.
#![allow(clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use ash_lib::features::agents::EventFrame;

/// Au-delà, on considère que le hook n'a pas rendu la main.
///
/// Très large devant le budget d'entrée standard du binaire (250 ms), pour qu'une machine
/// chargée ne fasse pas échouer un test qui parle d'autre chose. Ce que ce délai attrape,
/// c'est l'attente **sans fin** — la seule qui bloquerait un agent.
const PATIENCE: Duration = Duration::from_secs(10);

/// Un socket qui écoute, et les trames qui y arrivent.
///
/// L'attente est un canal, jamais un sommeil : un test qui dort pour « laisser le temps »
/// devient bruyant sur une machine chargée, puis désactivé.
struct Listening {
    path: PathBuf,
    frames: mpsc::Receiver<EventFrame>,
}

impl Listening {
    fn received(&self) -> Option<EventFrame> {
        self.frames.recv_timeout(PATIENCE).ok()
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Un chemin de socket court : la limite d'un chemin unix est de 104 octets, et le `TMPDIR`
/// par utilisateur de macOS en mange déjà la moitié.
fn a_socket_path() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    PathBuf::from(format!(
        "/tmp/ash-event-{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ))
}

fn listening() -> Listening {
    let path = a_socket_path();
    let listener = UnixListener::bind(&path).expect("le socket de test doit s'ouvrir");
    let (sender, frames) = mpsc::channel();

    std::thread::spawn(move || {
        for connection in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(connection).read_line(&mut line).is_err() {
                continue;
            }
            if let Ok(frame) = EventFrame::from_line(&line) {
                if sender.send(frame).is_err() {
                    return;
                }
            }
        }
    });

    Listening { path, frames }
}

/// Le binaire tel qu'un bloc de hooks l'appelle, avec l'entrée standard qu'on lui donne.
fn ash_event(socket: &Path, stdin: Stdio) -> Child {
    Command::new(env!("CARGO_BIN_EXE_ash-event"))
        .args(["waiting", "--tab", "01J0TAB", "--sock"])
        .arg(socket)
        .stdin(stdin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ash-event doit se lancer")
}

#[test]
fn given_a_hook_invoked_without_any_standard_input_when_it_runs_then_ash_receives_exactly_what_it_received_before(
) {
    // Given — la moitié qu'il ne faut pas casser. Un hook qui ne donne rien sur l'entrée
    // standard, ou un utilisateur qui tape la commande à la main : c'est la seule forme
    // qu'Ash ait connue jusqu'ici, et elle doit rester identique jusqu'à l'octet.
    let ash = listening();

    // When
    let mut hook = ash_event(&ash.path, Stdio::null());

    // Then
    assert_eq!(ash.received(), Some(EventFrame::new("waiting", "01J0TAB")));
    assert!(hook.wait().expect("le hook doit se terminer").success());
}

#[test]
fn given_a_hook_fired_inside_a_subagent_when_it_runs_then_the_child_reaches_ash_under_its_tab() {
    // Given — l'objet que Claude Code écrit sur l'entrée standard de chaque hook. `agent_id`
    // n'apparaît que dans un sous-agent, et il reste **subordonné** à l'onglet : c'est
    // `ASH_TAB_ID` qui corrèle, lui ne fait que désigner un enfant (ADR-0007, amendement du
    // 2026-08-13).
    let ash = listening();
    let payload = br#"{"session_id":"abc","hook_event_name":"PreToolUse",
                       "agent_id":"agent-7","agent_type":"code-reviewer"}"#;

    // When
    let mut hook = ash_event(&ash.path, Stdio::piped());
    hook.stdin
        .take()
        .expect("l'entrée standard doit être ouverte")
        .write_all(payload)
        .expect("l'objet du hook doit s'écrire");

    // Then
    assert_eq!(
        ash.received(),
        Some(
            EventFrame::new("waiting", "01J0TAB")
                .with_subagent(Some("agent-7"), Some("code-reviewer"))
        )
    );
    assert!(hook.wait().expect("le hook doit se terminer").success());
}

#[test]
fn given_a_hook_whose_standard_input_is_promised_and_never_written_when_it_runs_then_it_still_declares_its_state(
) {
    // Given — le cas qui coûterait le plus cher : un tube ouvert que personne n'écrit et que
    // personne ne ferme. Un hook qui ne rend pas la main **bloque l'agent** ; attendre cette
    // entrée-là reviendrait à suspendre `claude` sur une information dont on peut se passer.
    let ash = listening();

    // When — l'extrémité d'écriture est gardée **ouverte** pendant tout le test
    let mut hook = ash_event(&ash.path, Stdio::piped());
    let _write_end = hook
        .stdin
        .take()
        .expect("l'entrée standard doit être ouverte");

    // Then — l'état part quand même, et sans l'enfant qu'on n'a pas reçu
    assert_eq!(ash.received(), Some(EventFrame::new("waiting", "01J0TAB")));
    assert!(hook.wait().expect("le hook doit se terminer").success());
}

#[test]
fn given_a_hook_whose_standard_input_is_not_json_when_it_runs_then_the_declared_state_leaves_anyway(
) {
    // Given — un lanceur de hooks qui redirige un message d'erreur, un `jq` absent, un flux
    // coupé au milieu. Ce n'est pas une panne d'Ash : c'est une absence d'information, et
    // l'état déclaré, lui, ne dépend que de la ligne de commande.
    let ash = listening();

    // When
    let mut hook = ash_event(&ash.path, Stdio::piped());
    hook.stdin
        .take()
        .expect("l'entrée standard doit être ouverte")
        .write_all(b"Erreur : jq introuvable\n")
        .expect("le bruit doit s'écrire");

    // Then
    assert_eq!(ash.received(), Some(EventFrame::new("waiting", "01J0TAB")));
    assert!(hook.wait().expect("le hook doit se terminer").success());
}
