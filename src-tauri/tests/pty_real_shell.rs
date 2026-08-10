//! Tests d'intégration de la feature `pty` — avec un vrai shell dans un vrai PTY.
//!
//! Les tests unitaires vérifient les règles derrière le trait `PtySpawner`. Ceux-ci
//! vérifient ce qu'aucun double ne peut prouver : que le shell voit bien son
//! `ASH_TAB_ID`, et qu'un redimensionnement du master poste bien un `SIGWINCH`.
//!
//! Ils lancent `/bin/bash` — pas un agent, pas `claude`.

use std::io::Read;
use std::sync::mpsc;
use std::time::Duration;

use ash_lib::features::pty::{PtySession, PtySpawner, PtySpec, SystemPtySpawner};

/// Au-delà, on considère que le shell ne répondra pas. Assez large pour une machine
/// chargée, assez court pour ne pas figer la suite.
const PATIENCE: Duration = Duration::from_secs(10);

/// Lit le PTY dans un thread et rend tout ce qui est arrivé jusqu'à `needle`.
///
/// Le temps est une dépendance : plutôt qu'un `sleep` calibré à la louche, on attend un
/// marqueur précis, et on échoue sur un délai franc.
///
/// **Le marqueur ne doit jamais apparaître dans la commande envoyée.** Un PTY réaffiche
/// ce qu'on lui écrit : chercher « GOT_WINCH » alors que la commande contient
/// `echo GOT_WINCH` fait passer le test sur l'écho, sans que rien n'ait été signalé.
fn read_until(mut reader: Box<dyn Read + Send>, needle: &'static str) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let mut seen = String::new();
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    seen.push_str(&String::from_utf8_lossy(&buffer[..read]));
                    if seen.contains(needle) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(seen);
    });

    match rx.recv_timeout(PATIENCE) {
        Ok(seen) if seen.contains(needle) => Ok(seen),
        Ok(seen) => Err(format!(
            "le shell est sorti sans « {needle} ». Vu :\n{seen}"
        )),
        Err(_) => Err(format!("rien qui contienne « {needle} » en {PATIENCE:?}")),
    }
}

fn bash(env: Vec<(String, String)>) -> PtySpec {
    PtySpec {
        shell: "/bin/bash".into(),
        cwd: std::env::temp_dir(),
        cols: 80,
        rows: 24,
        env,
    }
}

#[test]
fn given_a_tab_id_in_the_spec_when_the_shell_starts_then_it_can_read_it_from_its_environment() {
    // Given
    let tab_id = "01JASHTABIDFORTEST";
    let spec = bash(vec![("ASH_TAB_ID".to_owned(), tab_id.to_owned())]);
    let (mut session, reader) = SystemPtySpawner.spawn(&spec).expect("bash doit démarrer");

    // When — le shell imprime la *valeur*, pas le nom. L'écho du PTY ne contient que
    // `$ASH_TAB_ID` : trouver la valeur prouve que la variable a été posée et développée.
    session
        .write(b"printf 'TAB=%s\\n' \"$ASH_TAB_ID\"; exit\n")
        .expect("l'écriture dans le PTY doit aboutir");

    // Then
    let seen = read_until(reader, tab_id).expect("le shell doit répondre");
    assert!(
        seen.contains(&format!("TAB={tab_id}")),
        "le shell n'a pas vu son ASH_TAB_ID. Sortie :\n{seen}"
    );
}

#[test]
fn given_a_shell_waiting_in_the_foreground_when_the_pty_is_resized_then_it_receives_a_sigwinch() {
    // Given — `read` est un builtin : bash reste lui-même en avant-plan, donc c'est bien
    // lui que le noyau signale. Avec un `sleep`, le signal irait au fils.
    let (mut session, reader) = SystemPtySpawner
        .spawn(&bash(Vec::new()))
        .expect("bash doit démarrer");
    // Deux précautions dans cette seule ligne :
    //   - le signal est nommé par son numéro (28 = SIGWINCH) et le marqueur est
    //     recomposé par `printf`, pour qu'aucune des deux chaînes cherchées n'existe
    //     dans l'écho du PTY ;
    //   - `$COLUMNS` n'est pas utilisé : bash ne le tient à jour que dans un shell
    //     interactif, et ce shell-ci ne l'est pas.
    session
        .write(b"trap 'printf \"W%s\\n\" INCH' 28; read -r _\n")
        .expect("l'écriture dans le PTY doit aboutir");
    // Laisser le `trap` être posé avant de signaler : redimensionner trop tôt enverrait
    // le signal à un shell qui ne l'attend pas encore.
    std::thread::sleep(Duration::from_millis(300));

    // When
    session
        .resize(120, 40)
        .expect("le redimensionnement doit aboutir");

    // Then
    let seen = read_until(reader, "WINCH").expect("bash doit recevoir le SIGWINCH");
    assert!(seen.contains("WINCH"), "sortie :\n{seen}");
}

/// Attend que l'avant-plan du terminal change de main, avec une échéance franche.
///
/// Sonder plutôt que dormir : on n'a aucun moyen de savoir *quand* bash aura appelé
/// `tcsetpgrp`, et un `sleep` calibré à la louche transformerait ce test en pile ou face
/// sur une machine chargée.
fn wait_for_foreground(session: &mut dyn PtySession, expected: bool) -> bool {
    let deadline = std::time::Instant::now() + PATIENCE;
    while std::time::Instant::now() < deadline {
        if matches!(session.has_foreground_process(), Ok(seen) if seen == expected) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn given_a_shell_that_launched_a_command_when_the_tab_is_questioned_then_it_reports_a_running_process(
) {
    // Given — un bash sans argument sur un PTY est *interactif*, donc son contrôle de
    // tâches est actif : c'est ce qui lui fait donner l'avant-plan à ses fils. Sans ça,
    // le fils resterait dans le groupe du shell et rien ne serait détectable.
    let (mut session, reader) = SystemPtySpawner
        .spawn(&bash(Vec::new()))
        .expect("bash doit démarrer");
    // Le marqueur est recomposé par `printf` : l'écho du PTY ne contient que « DLE ».
    session
        .write(b"printf 'I%s\\n' DLE\n")
        .expect("l'écriture dans le PTY doit aboutir");
    read_until(reader, "IDLE").expect("bash doit répondre");
    assert!(
        !session
            .has_foreground_process()
            .expect("le PTY doit savoir répondre"),
        "un shell à son invite ne tient rien d'autre en avant-plan"
    );

    // When
    session
        .write(b"sleep 5\n")
        .expect("l'écriture dans le PTY doit aboutir");

    // Then — c'est cette réponse-là qui déclenche la confirmation de `Cmd+W` (spec §4.4)
    assert!(
        wait_for_foreground(session.as_mut(), true),
        "le PTY doit signaler que quelque chose tourne dans l'onglet"
    );
}

#[test]
fn given_a_running_shell_when_it_is_killed_then_the_reader_reaches_end_of_file() {
    // Given
    let (mut session, mut reader) = SystemPtySpawner
        .spawn(&bash(Vec::new()))
        .expect("bash doit démarrer");

    // When
    session.kill().expect("terminer le shell doit aboutir");

    // Then — sans le `drop` du côté esclave à l'ouverture, ce `read` ne rendrait jamais 0
    // et l'onglet survivrait à son shell.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => continue,
            }
        }
        let _ = tx.send(());
    });
    assert!(
        rx.recv_timeout(PATIENCE).is_ok(),
        "le lecteur doit voir la fin du flux après la mort du shell"
    );
}
