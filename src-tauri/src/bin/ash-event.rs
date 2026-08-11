//! `ash-event` — le client du socket d'événements
//! ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)).
//!
//! ```text
//! ash-event working --tab $ASH_TAB_ID
//! ```
//!
//! C'est ce que le bloc délimité écrit dans le `settings.json` de chaque outil appelle, à
//! **chaque** hook. Trois conséquences, et elles gouvernent tout ce fichier :
//!
//! - **Il doit démarrer vite.** Il ne dépend donc pas d'`ash_lib` : lier Tauri ferait
//!   charger WebKit et AppKit pour écrire une ligne sur un socket. Le format et l'adresse
//!   sont partagés par inclusion du **même fichier source** que la bibliothèque, ce qui
//!   les tient ensemble sans les faire dépendre l'un de l'autre. Son revers assumé : les
//!   tests de `wire` sont exécutés deux fois, une fois par côté du fil — et c'est aussi
//!   ce qui prouve que le module compile hors de la bibliothèque.
//! - **Il ne doit jamais faire échouer un hook.** Ash fermé, socket absent, écriture
//!   refusée : il sort en 0, sans un mot. Un hook qui casse la session de l'utilisateur
//!   parce qu'Ash n'est pas lancé serait pire que pas de hook du tout.
//! - **Il ne doit jamais paniquer.** Pas d'`unwrap`, pas d'`expect` : une trace de panique
//!   au milieu d'une session d'agent est du bruit que personne ne saura relier à Ash.
//!
//! Une invocation *mal formée*, elle, sort en 1 avec un message : le bloc de hooks est
//! écrit par Ash, donc une erreur d'usage est un défaut d'Ash, et la taire la rendrait
//! introuvable. Le code 2 est délibérément évité — Claude Code s'en sert pour *bloquer*
//! l'outil et renvoyer `stderr` à l'agent.

// Le format et l'adresse du socket, partagés avec la bibliothèque sans en dépendre.
//
// Le client n'écrit que des trames, il n'en lit jamais : la moitié « lecture » du module
// est donc morte de ce côté-ci du fil, et c'est le prix — voulu — d'un format qui n'existe
// qu'en un seul exemplaire.
#[allow(dead_code)]
#[path = "../features/agents/wire.rs"]
mod wire;

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use wire::{socket_path, EventFrame};

/// Une écriture d'une ligne ne doit pas retenir un hook. Si Ash est à ce point figé, se
/// taire vaut mieux qu'attendre.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

const USAGE: &str = "usage : ash-event <état> --tab <id> [--sock <chemin>]";

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    let invocation = match parse(&arguments) {
        Ok(invocation) => invocation,
        Err(why) => {
            eprintln!("ash-event: {why}\n{USAGE}");
            std::process::exit(1);
        }
    };

    // À partir d'ici, plus rien n'a le droit d'échouer bruyamment.
    post(&invocation);
}

/// Ce qu'une invocation demande.
#[derive(Debug, PartialEq, Eq)]
struct Invocation {
    frame: EventFrame,
    socket: PathBuf,
}

/// L'analyse des arguments, à la main.
///
/// Cinq lignes contre une bibliothèque d'analyse : la surface est un verbe et deux options,
/// et le temps de démarrage est le seul critère qui compte ici.
fn parse(arguments: &[String]) -> Result<Invocation, String> {
    let mut kind: Option<&str> = None;
    let mut tab: Option<&str> = None;
    let mut socket: Option<&str> = None;

    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        // Les deux écritures d'une option sont acceptées : le bloc de hooks est un fichier
        // que l'utilisateur peut relire et retoucher, et `--tab=$ASH_TAB_ID` est la forme
        // que beaucoup écriront de mémoire.
        let (name, inlined) = match argument.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (argument.as_str(), None),
        };

        match name {
            "--tab" | "--sock" => {
                let value = match inlined {
                    Some(value) => value,
                    None => arguments
                        .next()
                        .map(String::as_str)
                        .ok_or_else(|| format!("{name} attend une valeur"))?,
                };
                if name == "--tab" {
                    tab = Some(value);
                } else {
                    socket = Some(value);
                }
            }
            other if other.starts_with('-') => return Err(format!("option inconnue : {other}")),
            other => {
                if kind.is_some() {
                    return Err(format!("état déjà donné, et « {other} » en plus"));
                }
                kind = Some(other);
            }
        }
    }

    let kind = kind.ok_or_else(|| "état manquant".to_owned())?;
    // La corrélation se fait par `ASH_TAB_ID` et par rien d'autre (ADR-0007). Le shell
    // développe `$ASH_TAB_ID` en chaîne vide quand la variable n'est pas posée — un agent
    // lancé hors d'Ash — et une trame sans onglet n'a nulle part où aller.
    let tab = tab.ok_or_else(|| "onglet manquant : --tab est obligatoire".to_owned())?;
    if tab.trim().is_empty() {
        return Err("onglet vide : ASH_TAB_ID n'était pas posé".to_owned());
    }

    Ok(Invocation {
        frame: EventFrame::new(kind, tab),
        socket: socket
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("ASH_SOCK").map(PathBuf::from))
            .unwrap_or_else(socket_path),
    })
}

/// Poste la trame, et se tait quoi qu'il arrive.
fn post(invocation: &Invocation) {
    let Ok(line) = invocation.frame.to_line() else {
        return;
    };
    let Ok(mut stream) = UnixStream::connect(&invocation.socket) else {
        // Ash n'est pas lancé : le cas le plus courant, et il est normal.
        return;
    };
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.write_all(line.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(line: &[&str]) -> Vec<String> {
        line.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn given_the_invocation_written_in_the_hook_block_when_it_is_parsed_then_the_event_names_its_tab(
    ) {
        // Given — la ligne exacte que le bloc délimité d'ADR-0007 écrit dans le
        // `settings.json` de l'outil, une fois `$ASH_TAB_ID` remplacé par le shell.
        let written = arguments(&["working", "--tab", "01J0TAB"]);

        // When
        let parsed = parse(&written);

        // Then
        assert_eq!(
            parsed.map(|invocation| invocation.frame),
            Ok(EventFrame::new("working", "01J0TAB"))
        );
    }

    #[test]
    fn given_the_option_written_with_an_equals_sign_when_it_is_parsed_then_it_means_the_same_thing()
    {
        // Given — le bloc est un fichier que l'utilisateur relit et retouche ; refuser une
        // des deux écritures ferait perdre des états sans jamais dire pourquoi.
        let written = arguments(&["waiting", "--tab=01J0TAB", "--sock=/tmp/ailleurs.sock"]);

        // When
        let parsed = parse(&written);

        // Then
        assert_eq!(
            parsed,
            Ok(Invocation {
                frame: EventFrame::new("waiting", "01J0TAB"),
                socket: PathBuf::from("/tmp/ailleurs.sock"),
            })
        );
    }

    #[test]
    fn given_an_invocation_without_a_tab_when_it_is_parsed_then_it_is_refused_rather_than_guessed()
    {
        // Given — un agent lancé hors d'Ash n'a pas d'`ASH_TAB_ID`, et le shell développe
        // alors `--tab $ASH_TAB_ID` en une option vide. Deviner l'onglet par le `cwd` ou
        // par un horodatage est précisément ce qu'ADR-0007 interdit : il n'y a rien à
        // envoyer, et le dire est la seule conduite honnête.
        let outside_ash = [
            arguments(&["working"]),
            arguments(&["working", "--tab", ""]),
        ];

        // When
        let parsed = outside_ash.map(|written| parse(&written));

        // Then
        assert!(parsed.iter().all(Result::is_err), "{parsed:?}");
    }
}
