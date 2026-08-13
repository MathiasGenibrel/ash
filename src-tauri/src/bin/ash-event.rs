//! `ash-event` — le client du socket d'événements
//! ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)).
//!
//! ```text
//! ash-event working --tab $ASH_TAB_ID
//! ```
//!
//! Depuis l'amendement du 2026-08-13 à ADR-0007, il lit aussi ce que l'outil lui donne sur
//! son **entrée standard** — l'objet JSON que tout hook de Claude Code écrit dans le
//! processus qu'il lance, et qui porte `agent_id` et `agent_type` dès que le hook part d'un
//! sous-agent. Voir [`subagent`] pour ce que ça coûte, et pour ce qui garantit qu'il
//! n'attend jamais.
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

use std::io::{IsTerminal, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use wire::{socket_path, EventFrame};

/// Une écriture d'une ligne ne doit pas retenir un hook. Si Ash est à ce point figé, se
/// taire vaut mieux qu'attendre.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// Ce qu'on accorde à une entrée standard qui a été promise mais qui ne vient pas.
///
/// Un hook qui ne rend pas la main **bloque l'agent** : le budget est donc court devant
/// l'humain qui attend son agent, et large devant l'écriture d'un objet déjà en mémoire par
/// un processus qui vient de nous lancer. Passé ce délai, l'état déclaré part quand même,
/// sans la clé d'enfant.
const STDIN_BUDGET: Duration = Duration::from_millis(250);

/// Au-delà, on ne lit plus : deux champs courts ne justifient pas de suivre un flux sans
/// fin, et la trame est de toute façon bornée à [`wire::MAX_FRAME_BYTES`].
///
/// L'objet d'un hook de Claude Code pèse quelques centaines d'octets ; le plus gros connu
/// — un `PostToolUse` qui recopie l'entrée et la sortie d'un outil — reste très en deçà.
const MAX_PAYLOAD_BYTES: u64 = 64 * 1024;

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

    // L'entrée standard est lue **après** les arguments, et jamais avant : une invocation
    // mal formée est un défaut d'Ash, et elle doit le dire tout de suite plutôt qu'après
    // avoir attendu un objet qu'elle n'utilisera pas.
    let invocation = invocation.enriched_with(subagent());

    // À partir d'ici, plus rien n'a le droit d'échouer bruyamment.
    post(&invocation);
}

/// Ce qu'une invocation demande.
#[derive(Debug, PartialEq, Eq)]
struct Invocation {
    frame: EventFrame,
    socket: PathBuf,
}

impl Invocation {
    /// La même invocation, en nommant l'enfant si l'outil en a nommé un.
    fn enriched_with(mut self, subagent: Option<Subagent>) -> Self {
        let Some(subagent) = subagent else {
            return self;
        };
        self.frame = self
            .frame
            .with_subagent(subagent.agent_id.as_deref(), subagent.agent_type.as_deref());
        self
    }
}

/// L'enfant que l'outil a nommé sur l'entrée standard, s'il y en a un.
///
/// Les champs inconnus de l'objet sont ignorés : on ne lit ici que ce qu'ADR-0007 autorise
/// à transporter, et un hook en dit bien davantage — le `cwd`, la session, le transcript.
#[derive(Debug, Default, PartialEq, Eq, serde::Deserialize)]
struct Subagent {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    agent_type: Option<String>,
}

/// L'enfant lu sur l'entrée standard — et rien du tout au moindre doute.
///
/// **Ce chemin ne peut pas bloquer, et c'est sa seule vraie contrainte** ; trois cas, trois
/// conduites :
///
/// - **un terminal** — quelqu'un lance `ash-event` à la main pour voir. On ne lit rien du
///   tout : un `read` sur un tty attend une frappe, c'est-à-dire pour toujours ;
/// - **un tube, ou un fichier** — le cas des hooks. La lecture se fait sur un fil, et
///   l'attente est bornée par [`STDIN_BUDGET`] : un tube ouvert mais muet — un lanceur qui
///   oublie de fermer son extrémité d'écriture — laisse donc partir l'état déclaré au lieu
///   de retenir l'agent. Le fil resté dans son `read` ne retient rien non plus : la fin de
///   `main` termine le processus, fils compris ;
/// - **une entrée fermée** — le `read` échoue aussitôt, et il n'y a rien à en tirer.
///
/// Une entrée illisible, tronquée ou qui n'est pas du JSON tombe dans le même `None` : ce
/// n'est pas une erreur, c'est une absence d'information. L'état déclaré, lui, part dans
/// tous les cas.
fn subagent() -> Option<Subagent> {
    subagent_of(&read_stdin()?)
}

/// L'enfant que porte un objet de hook, si cet objet en nomme un.
///
/// Séparée de la lecture parce que c'est la seule moitié qui décide quelque chose : ce qui
/// entre est du texte, et ce qui sort est ce qu'ADR-0007 autorise à transporter.
fn subagent_of(payload: &str) -> Option<Subagent> {
    let subagent: Subagent = serde_json::from_str(payload).ok()?;
    (subagent != Subagent::default()).then_some(subagent)
}

fn read_stdin() -> Option<String> {
    if std::io::stdin().is_terminal() {
        return None;
    }

    let (read, payload) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let outcome = std::io::stdin()
            .take(MAX_PAYLOAD_BYTES)
            .read_to_string(&mut buffer)
            .map(|_| buffer);
        let _ = read.send(outcome.ok());
    });

    payload.recv_timeout(STDIN_BUDGET).ok().flatten()
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
    let Some(line) = line_of(&invocation.frame) else {
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

/// La ligne à écrire, quitte à **abandonner l'enfant** pour tenir dans la trame.
///
/// La borne du fil (`wire::MAX_FRAME_BYTES`, 8 Kio) est une frontière de sécurité du
/// serveur : au-delà, la ligne est refusée sans être accumulée, donc l'état déclaré serait
/// perdu. L'état est la seule chose qu'un hook existe pour transporter : la clé d'enfant
/// tombe donc la première, et la trame repart sans elle.
///
/// **Ce repli est silencieux, et c'est pourquoi il ne doit plus pouvoir se déclencher pour
/// une clé d'enfant** : une ligne fille qui disparaîtrait ici ne laisserait aucune trace.
/// C'est le rôle de `wire::MAX_CHILD_KEY_BYTES`, qui écarte une clé démesurée **là où elle
/// entre**, bien avant que la trame ne puisse déborder à cause d'elle. Ce qui reste ici ne
/// couvre plus qu'un `<état>` ou un `--tab` absurdes, que le bloc d'Ash n'écrit pas.
fn line_of(frame: &EventFrame) -> Option<String> {
    frame
        .to_line()
        .or_else(|_| frame.clone().without_subagent().to_line())
        .ok()
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

    /// L'objet qu'un hook de Claude Code écrit sur l'entrée standard, réduit à ce qui nous
    /// concerne — mais gardé **entier** dans sa forme : les clés que nous ignorons sont là,
    /// parce que les ignorer est précisément ce qui doit être vérifié.
    fn hook_payload(child: &str) -> String {
        format!(
            r#"{{"session_id":"abc","transcript_path":"/tmp/t.jsonl","cwd":"/dev/ash",
             "hook_event_name":"PreToolUse","tool_name":"Read"{child}}}"#
        )
    }

    #[test]
    fn given_a_hook_fired_inside_a_subagent_when_its_payload_is_read_then_the_child_is_named() {
        // Given — l'information arrive déjà aujourd'hui, à chaque hook, et part à la
        // poubelle : Claude Code pose `agent_id` et `agent_type` dès que le hook se
        // déclenche dans un sous-agent (ADR-0007, amendement du 2026-08-13).
        let written = hook_payload(r#","agent_id":"agent-7","agent_type":"code-reviewer""#);

        // When
        let child = subagent_of(&written);

        // Then
        assert_eq!(
            child,
            Some(Subagent {
                agent_id: Some("agent-7".to_owned()),
                agent_type: Some("code-reviewer".to_owned()),
            })
        );
    }

    #[test]
    fn given_a_standard_input_that_names_no_child_when_it_is_read_then_the_event_stays_what_it_was()
    {
        // Given — les quatre entrées que l'on rencontrera vraiment : le hook de l'agent
        // principal, une entrée vide, un flux coupé au milieu, et quelque chose qui n'est
        // pas du JSON. Aucune n'est une erreur : ce sont des absences d'information, et
        // l'état déclaré doit partir dans les quatre cas.
        let entrances = [
            hook_payload(""),
            String::new(),
            r#"{"session_id":"abc","agent_i"#.to_owned(),
            "Erreur : jq introuvable\n".to_owned(),
        ];

        // When
        let children: Vec<Option<Subagent>> = entrances
            .iter()
            .map(|written| subagent_of(written))
            .collect();

        // Then
        assert_eq!(children, vec![None, None, None, None]);
    }

    #[test]
    fn given_a_child_whose_type_is_absurdly_long_when_it_is_posted_then_the_child_is_still_named_and_nothing_is_dropped_in_silence(
    ) {
        // Given — 8 Kio est une frontière de sécurité du serveur, pas un réglage : une ligne
        // plus longue est refusée sans être accumulée, et `line_of` la reposterait alors
        // **sans l'enfant**, en silence. Une ligne fille qui s'évanouirait ainsi serait
        // introuvable. La borne des clés (`MAX_CHILD_KEY_BYTES`) écarte le nom démesuré là
        // où il entre, donc l'enfant garde son identité et le repli ne se déclenche pas.
        let frame = EventFrame::new("waiting", "01J0TAB")
            .with_subagent(Some("agent-7"), Some(&"z".repeat(16 * 1024)));

        // When
        let line = line_of(&frame);

        // Then
        assert_eq!(
            line,
            EventFrame::new("waiting", "01J0TAB")
                .with_subagent(Some("agent-7"), None)
                .to_line()
                .ok()
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
