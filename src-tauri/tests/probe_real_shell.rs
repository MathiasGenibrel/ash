//! Tests d'intégration de la sonde d'[ADR-0005] — avec un vrai shell dans un vrai PTY.
//!
//! Les tests unitaires vérifient les règles de repli derrière le trait `Probe`. Ceux-ci
//! vérifient ce qu'aucun double ne peut prouver : que `tcgetpgrp` + `proc_pidinfo`
//! disent la vérité sur un système vivant, assez vite, et **pendant qu'un programme
//! tourne** — le cas où OSC 7 se tait, et donc la raison d'être de la décision.
//!
//! Ils lancent `/bin/bash`, `sleep` et `less` — pas un agent, pas `claude`. Aucun fichier
//! de configuration shell n'est lu ni écrit : `bash` est lancé tel quel, et rien n'est
//! posé dans son environnement au-delà de ce que le PTY d'Ash y met déjà.
//!
//! [ADR-0005]: ../../docs/adr/0005-sonde-cwd-libproc.md

// `expect` est la façon normale d'échouer dans un test, et `clippy.toml` l'autorise déjà
// à ce titre — mais seulement à l'intérieur des `#[test]`. Clippy ne reconnaît pas comme
// « du test » les fonctions d'aide d'un test d'intégration ; on le dit donc ici, pour ce
// fichier seulement.
#![allow(clippy::expect_used)]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ash_lib::features::probe::{Probe, SystemProbe, TabObservation, TabWatch};
use ash_lib::features::pty::{PtySession, PtySpawner, PtySpec, SystemPtySpawner};

/// Au-delà, on considère que le shell ne répondra pas. Assez large pour une machine
/// chargée, assez court pour ne pas figer la suite.
const PATIENCE: Duration = Duration::from_secs(10);

/// Le budget du critère d'acceptation : un `cd` doit être vu en moins de 400 ms.
///
/// C'est la latence qu'ADR-0005 accepte (~300 ms de boucle), plus la marge d'une passe.
const CWD_BUDGET: Duration = Duration::from_millis(400);

/// Sonder ne demande rien au shell, mais un shell dont personne ne lit la sortie finit
/// par se bloquer sur un PTY plein. On vide, et on jette.
fn drain(mut reader: Box<dyn Read + Send>) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while matches!(reader.read(&mut buffer), Ok(read) if read > 0) {}
    });
}

/// Un répertoire à part, sous un vrai chemin canonique.
///
/// `proc_pidinfo` rend le chemin résolu par le noyau : sur macOS, `/tmp` est un lien vers
/// `/private/tmp`, et comparer sans canonicaliser ferait échouer le test pour une raison
/// qui n'a rien à voir avec la sonde.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ash-probe-{name}"));
    std::fs::create_dir_all(&dir).expect("le répertoire de travail doit être créable");
    std::fs::canonicalize(&dir).expect("le répertoire de travail doit être résolvable")
}

fn bash_in(cwd: &Path) -> PtySpec {
    PtySpec {
        shell: "/bin/bash".into(),
        cwd: cwd.to_path_buf(),
        cols: 80,
        rows: 24,
        // `TERM` pour que les TUI acceptent de démarrer. Rien n'est écrit sur le disque :
        // ADR-0005 interdit de toucher la configuration shell de l'utilisateur, pas de
        // décrire le terminal au processus qu'on lance.
        env: vec![("TERM".to_owned(), "xterm-256color".to_owned())],
    }
}

/// Ouvre un bash interactif sondable, et attend qu'il soit observable à son invite.
///
/// Un bash sans argument sur un PTY est interactif, donc son contrôle de tâches est
/// actif : c'est ce qui lui fait donner l'avant-plan à ses fils.
fn observable_shell(cwd: &Path) -> (Box<dyn PtySession>, TabWatch) {
    let (session, reader) = SystemPtySpawner
        .spawn(&bash_in(cwd))
        .expect("bash doit démarrer");
    drain(reader);

    let terminal = session
        .terminal()
        .expect("un PTY local doit exposer son master et le pid de son shell");
    let mut watch = TabWatch::new(terminal.master_fd, terminal.shell_pid);

    assert!(
        wait_until(&mut watch, PATIENCE, |seen| seen.cwd == cwd),
        "la sonde doit voir le shell dans son répertoire de départ avant qu'on le pilote"
    );
    (session, watch)
}

/// Sonde jusqu'à ce que l'observation satisfasse `expected`, ou jusqu'au délai.
///
/// Sonder plutôt que dormir : rien ne dit *quand* bash aura fini de traiter la ligne
/// qu'on lui a envoyée, et un `sleep` calibré à la louche transformerait ces tests en
/// pile ou face sur une machine chargée.
fn wait_until(
    watch: &mut TabWatch,
    budget: Duration,
    expected: impl Fn(&TabObservation) -> bool,
) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if matches!(watch.observe(&SystemProbe), Ok(seen) if expected(&seen)) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn given_a_shell_at_its_prompt_when_it_changes_directory_then_the_probe_follows_within_the_budget()
{
    // Given
    let start = scratch("cd-start");
    let target = scratch("cd-target");
    let (mut session, mut watch) = observable_shell(&start);

    // When — `cd` est un builtin : c'est le shell lui-même qui se déplace, et rien
    // n'apparaît dans sa sortie. Aucun écho ne peut donc faire passer ce test à tort.
    session
        .write(format!("cd {}\n", target.display()).as_bytes())
        .expect("l'écriture dans le PTY doit aboutir");

    // Then
    let followed = wait_until(&mut watch, CWD_BUDGET, |seen| seen.cwd == target);
    let _ = session.kill();
    assert!(
        followed,
        "la sonde doit voir le nouveau répertoire en moins de {CWD_BUDGET:?}"
    );
}

#[test]
fn given_a_program_running_in_another_directory_when_probing_then_the_tab_is_where_that_program_works(
) {
    // Given — le cas central d'ADR-0005 : le shell n'est pas revenu à son invite, donc
    // OSC 7 n'aurait rien émis depuis le lancement de l'onglet.
    let start = scratch("running-start");
    let target = scratch("running-target");
    let (mut session, mut watch) = observable_shell(&start);
    let shell = session
        .terminal()
        .expect("le PTY doit rester observable")
        .shell_pid;

    // When — un sous-shell en avant-plan, qui se déplace puis occupe le terminal
    session
        .write(format!("(cd {} && sleep 30)\n", target.display()).as_bytes())
        .expect("l'écriture dans le PTY doit aboutir");

    // Then
    let moved = wait_until(&mut watch, PATIENCE, |seen| {
        seen.cwd == target && !seen.foreground.is_shell
    });
    let shell_cwd = SystemProbe.inspect(shell).map(|info| info.cwd);
    let _ = session.kill();

    assert!(
        moved,
        "la sonde doit rendre le répertoire du programme en avant-plan"
    );
    assert_eq!(
        shell_cwd.as_deref().map(Path::to_path_buf),
        Ok(start),
        "le shell, lui, n'a pas bougé : c'est bien l'avant-plan qui a été sondé"
    );
}

#[test]
fn given_a_full_screen_program_holding_the_terminal_when_probing_then_it_is_named_correctly() {
    // Given — `less` prend l'écran et garde l'avant-plan tant qu'on ne le quitte pas :
    // c'est la TUI la plus sûre à convoquer, elle est livrée avec macOS.
    let dir = scratch("tui");
    let (mut session, mut watch) = observable_shell(&dir);

    // When
    session
        .write(b"less /etc/hosts\n")
        .expect("l'écriture dans le PTY doit aboutir");

    // Then — c'est ce nom-là que la découverte d'agents (ADR-0006) comparera aux
    // commandes reconnues ; le lire faux, c'est ne jamais voir naître un agent.
    let named = wait_until(&mut watch, PATIENCE, |seen| {
        seen.foreground.name == "less" && !seen.foreground.is_shell
    });
    let _ = session.kill();
    assert!(named, "la sonde doit nommer le programme en avant-plan");
}
