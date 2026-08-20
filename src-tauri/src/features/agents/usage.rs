//! Ce qu'un outil consomme de sa fenêtre de contexte, et par où Ash l'apprend.
//!
//! **Ce n'est pas un état d'agent**, et rien ici n'a de chemin vers
//! [`AgentState`](super::state::AgentState) : un contexte plein ne rend pas un onglet
//! `error`, et un contexte vide ne le rend pas `idle`. La règle d'
//! [ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md) — un état vient d'un hook,
//! jamais d'une déduction — n'est pas en cause, mais elle donne le ton : ce module lit une
//! mesure que l'outil a écrite, il n'en infère rien d'autre.
//!
//! La capacité est **optionnelle**, sur le modèle exact de
//! [`SubagentSupport`](super::adapter::SubagentSupport) : un outil qui ne tient pas de
//! transcript répond [`UsageSupport::None`], et l'onglet n'a alors pas d'usage du tout —
//! pas une valeur à zéro, pas un tiret, rien
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)).
//!
//! **Le partage est le même que pour l'instrumentation** : l'adaptateur *interprète*, la
//! feature *lit le disque*. `Adapter` reste `Send + Sync` et sans effet de bord, donc
//! testable sans fichier ; [`Transcripts`] porte l'effet, avec son implémentation système et
//! sa doublure.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use super::adapter::Adapter;

/// La fenêtre de contexte qu'Ash suppose, faute que quiconque la lui dise.
///
/// **C'est une limite connue, pas une mesure.** Le transcript de Claude Code écrit bien le
/// modèle (`"model":"claude-opus-5"`), mais sans le suffixe qui distingue une session de
/// 1 M de tokens d'une session de 200 k — et le `stdin` d'un hook ne porte de `model` que
/// sur `SessionStart`, sans garantie de présence. Aucune des deux sources qu'ADR-0007
/// autorise ne dit donc la taille de la fenêtre.
///
/// Ce qui est mesuré exactement, c'est le **numérateur** : les tokens réellement consommés,
/// que le transcript écrit à chaque tour. Le dénominateur est cette constante, et une
/// session de 1 M lira donc un pourcentage cinq fois trop haut. Le jour où une source
/// fiable existera — un réglage, ou un champ que l'outil se met à écrire — c'est la seule
/// ligne à changer.
pub const DEFAULT_CONTEXT_WINDOW: u64 = 200_000;

/// Ce qu'Ash lit de la **fin** d'un transcript, et pas un octet de plus.
///
/// **C'est une borne de coût, et elle est la raison pour laquelle lire est gratuit.** Un
/// transcript de session longue pèse des dizaines de mégaoctets, et la mesure cherchée tient
/// dans sa dernière ligne d'assistant : la relire entière à chaque hook ferait payer à
/// chaque tour d'agent une lecture qui grossit avec la conversation.
///
/// 256 Kio couvrent largement les derniers tours, y compris quand l'un d'eux porte un gros
/// résultat d'outil. Si la queue ne contenait aucune ligne d'usage, l'onglet garde
/// simplement la valeur qu'il avait : une mesure manquante n'efface pas la précédente.
pub const TRANSCRIPT_TAIL_BYTES: u64 = 256 * 1024;

/// La place qu'une conversation occupe dans sa fenêtre.
///
/// Deux nombres, et pas un pourcentage : le calcul est un fait d'affichage, et le garder ici
/// laisserait le frontend incapable de dire `128k / 200k` le jour où la maquette le
/// demandera. C'est la même règle que pour
/// [`state_since`](crate::features::pty::TabInfo) — ce qui traverse est la donnée, pas sa
/// mise en forme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    /// Les tokens que la conversation occupe — entrée, cache lu, cache écrit, et la sortie
    /// du dernier tour.
    ///
    /// **`number` et non `bigint`**, pour la raison écrite au long sur `state_since` : c'est
    /// un nombre JSON que la webview lit en `number`, et un compte de tokens ne s'approche
    /// pas de 2⁵³.
    #[cfg_attr(test, ts(type = "number"))]
    pub used_tokens: u64,
    /// La fenêtre dans laquelle ces tokens tiennent — voir [`DEFAULT_CONTEXT_WINDOW`] pour
    /// ce que cette valeur sait, et ce qu'elle suppose.
    #[cfg_attr(test, ts(type = "number"))]
    pub window_tokens: u64,
}

/// Un outil dit-il la place qu'il consomme ?
///
/// La question que le cœur pose, et la seule qu'il ait besoin de poser pour décider s'il
/// peut afficher une jauge. Le *format* de la réponse, lui, ne sort pas de l'adaptateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageSupport {
    /// L'outil n'en dit rien. Aucune jauge, et rien qui suggère qu'il en manque une.
    None,
    /// L'outil tient un transcript, dont Ash lit la fin.
    Transcript,
}

/// Par où Ash lit la fin d'un transcript.
///
/// Un trait parce que c'est un effet système, et la convention du dépôt les fait tous passer
/// par un port que la feature possède : la suite du superviseur se joue alors sans écrire un
/// seul fichier, et sans dépendre de ce qu'un transcript réel contient le jour où on la
/// lance.
pub trait Transcripts: Send + Sync {
    /// Les derniers [`TRANSCRIPT_TAIL_BYTES`] du fichier nommé, ou rien.
    ///
    /// `None` couvre tout ce qui peut clocher — chemin absent, droits refusés, fichier
    /// effacé entre le hook et la lecture. Ce n'est pas une erreur à remonter : c'est une
    /// absence de mesure, et l'onglet garde ce qu'il savait.
    fn tail(&self, path: &Path) -> Option<String>;
}

/// Le vrai lecteur : celui qui touche le disque.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileTranscripts;

impl Transcripts for FileTranscripts {
    /// Ouvre, se place à `len - TRANSCRIPT_TAIL_BYTES`, lit jusqu'au bout.
    ///
    /// La première ligne rendue est **écartée** dès que la lecture a commencé au milieu du
    /// fichier : le saut tombe presque toujours au milieu d'une ligne, et une demi-ligne de
    /// JSON n'est pas un objet — la laisser passer ferait échouer une analyse pour une
    /// raison qui n'a rien à voir avec le contenu.
    fn tail(&self, path: &Path) -> Option<String> {
        let mut file = File::open(path).ok()?;
        let length = file.metadata().ok()?.len();
        let from = length.saturating_sub(TRANSCRIPT_TAIL_BYTES);
        file.seek(SeekFrom::Start(from)).ok()?;

        let mut bytes = Vec::with_capacity(TRANSCRIPT_TAIL_BYTES as usize);
        file.take(TRANSCRIPT_TAIL_BYTES)
            .read_to_end(&mut bytes)
            .ok()?;

        // `from_utf8_lossy` et non `from_utf8` : le saut peut couper un caractère multioctet
        // en deux, et perdre la queue entière pour un accent tronqué serait absurde. La ligne
        // partielle qui le porte est écartée juste après, de toute façon.
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Some(if from == 0 {
            text
        } else {
            after_first_line(&text).to_owned()
        })
    }
}

/// Ce qui suit le premier retour à la ligne — vide s'il n'y en a aucun.
fn after_first_line(text: &str) -> &str {
    text.split_once('\n').map_or("", |(_, rest)| rest)
}

/// Ce que le transcript nommé par un hook dit de la place consommée, ou rien.
///
/// L'ordre des opérations **est** la règle, et c'est pourquoi elle vit ici plutôt que chez
/// son unique appelant : la queue est tirée une seule fois par le port, puis présentée aux
/// adaptateurs qui ont déclaré savoir la lire, et le premier qui répond répond. La forme est
/// celle des deux autres portes du trait (`translate`, `child_event`) ; ce qui la distingue
/// est qu'elle lit un **fichier** avant d'interroger qui que ce soit.
///
/// **Les adaptateurs muets sont écartés avant la lecture, et c'est ce qui rend la capacité
/// gratuite** : un onglet servi par `generic` ne fait ouvrir aucun fichier, même quand la
/// trame porte un chemin. [`UsageSupport::None`] n'est donc pas seulement une promesse de ne
/// rien rendre — c'est une promesse de ne rien coûter, et le test qui la garde compte les
/// ouvertures.
///
/// `None` couvre toute la gamme des absences, et aucune n'est une erreur : pas de chemin dans
/// la trame, fichier illisible ou effacé, queue sans un seul tour d'assistant. Rien n'est
/// journalisé, rien ne remonte : l'onglet garde ce qu'il savait.
pub(super) fn measure(
    adapters: &[Arc<dyn Adapter>],
    transcripts: &dyn Transcripts,
    transcript_path: Option<&str>,
) -> Option<SessionUsage> {
    let path = transcript_path?;

    // `peekable` et non un `collect` : savoir *s'il y a* un lecteur suffit à décider s'il
    // faut toucher au disque, et l'itérateur repart ensuite au même endroit.
    let mut readers = adapters
        .iter()
        .filter(|adapter| adapter.usage() != UsageSupport::None)
        .peekable();
    readers.peek()?;

    let tail = transcripts.tail(Path::new(path))?;
    readers.find_map(|adapter| adapter.read_usage(&tail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::adapters::{ClaudeCodeAdapter, GenericAdapter};
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Un transcript décrit par le scénario, qui **compte** les fois où on le lui demande.
    ///
    /// Le compteur est ce qui distingue cette doublure de [`super::super::fakes::FakeTranscripts`] :
    /// la promesse d'`UsageSupport::None` porte sur ce qui ne se produit pas, et une absence
    /// de lecture ne se lit dans aucune valeur de retour.
    #[derive(Debug, Default)]
    struct CountingTranscripts {
        tails: Mutex<std::collections::HashMap<std::path::PathBuf, String>>,
        reads: AtomicUsize,
    }

    impl CountingTranscripts {
        fn holding(path: &str, tail: &str) -> Self {
            let this = Self::default();
            this.tails
                .lock()
                .unwrap()
                .insert(std::path::PathBuf::from(path), tail.to_owned());
            this
        }

        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }
    }

    impl Transcripts for CountingTranscripts {
        fn tail(&self, path: &Path) -> Option<String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.tails.lock().unwrap().get(path).cloned()
        }
    }

    const TRANSCRIPT: &str = "/Users/x/.claude/projects/ash/session.jsonl";

    /// Une queue qu'un adaptateur sachant lire le format de Claude Code comprendrait.
    const A_TURN: &str = r#"{"type":"assistant","message":{"usage":{"input_tokens":900}}}"#;

    /// Les adaptateurs tels que le composition root les pose.
    fn claude_code() -> Vec<Arc<dyn Adapter>> {
        vec![
            Arc::new(GenericAdapter),
            Arc::new(ClaudeCodeAdapter::new(std::path::PathBuf::from(
                "/Applications/Ash.app/Contents/MacOS/ash-event",
            ))),
        ]
    }

    #[test]
    fn given_no_adapter_that_declares_usage_when_a_transcript_is_named_then_nothing_is_ever_opened()
    {
        // Given — le socle d'ADR-0008 seul, et un transcript parfaitement lisible à côté.
        let adapters: Vec<Arc<dyn Adapter>> = vec![Arc::new(GenericAdapter)];
        let transcripts = CountingTranscripts::holding(TRANSCRIPT, A_TURN);

        // When
        let measured = measure(&adapters, &transcripts, Some(TRANSCRIPT));

        // Then — pas de mesure, et surtout **pas d'ouverture** : `UsageSupport::None` promet
        // aussi de ne rien coûter, et un onglet servi par `generic` ne paye pas une
        // entrée-sortie par hook.
        assert_eq!(measured, None);
        assert_eq!(transcripts.reads(), 0);
    }

    #[test]
    fn given_a_hook_that_names_no_transcript_when_it_is_measured_then_nothing_is_opened() {
        // Given — le cas de tous les hooks d'avant cette tranche, et de tout outil qui n'en
        // écrit pas : la trame n'a pas de chemin.
        let transcripts = CountingTranscripts::holding(TRANSCRIPT, A_TURN);

        // When
        let measured = measure(&claude_code(), &transcripts, None);

        // Then
        assert_eq!(measured, None);
        assert_eq!(transcripts.reads(), 0);
    }

    #[test]
    fn given_an_adapter_that_reads_transcripts_when_one_is_named_then_it_is_read_once() {
        // Given — la queue est tirée par le port, puis présentée : c'est ce qui garantit
        // qu'ajouter un adaptateur lecteur n'ajoute pas une lecture de disque par hook.
        let transcripts = CountingTranscripts::holding(TRANSCRIPT, A_TURN);

        // When
        let measured = measure(&claude_code(), &transcripts, Some(TRANSCRIPT));

        // Then
        assert_eq!(
            measured,
            Some(SessionUsage {
                used_tokens: 900,
                window_tokens: DEFAULT_CONTEXT_WINDOW,
            })
        );
        assert_eq!(transcripts.reads(), 1);
    }

    /// Un transcript sur le disque, dans un dossier que le test emporte avec lui.
    fn transcript_of(name: &str, content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("ash-usage-{name}.jsonl"));
        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn given_a_short_transcript_when_its_tail_is_read_then_every_line_comes_back() {
        // Given — plus court que la borne : il n'y a rien à couper, donc rien à écarter.
        let path = transcript_of("short", "{\"a\":1}\n{\"b\":2}\n");

        // When
        let tail = FileTranscripts.tail(&path);

        // Then — la première ligne est bien là : la garder ou l'écarter est décidé par le
        // fait d'avoir sauté, pas par une heuristique sur son contenu.
        assert_eq!(tail.as_deref(), Some("{\"a\":1}\n{\"b\":2}\n"));
    }

    #[test]
    fn given_a_transcript_longer_than_the_tail_when_it_is_read_then_the_line_cut_in_half_is_dropped(
    ) {
        // Given — une ligne de bourrage plus longue que la borne, puis la ligne qui compte.
        let padding = "x".repeat(TRANSCRIPT_TAIL_BYTES as usize + 64);
        let path = transcript_of("long", &format!("{padding}\n{{\"last\":true}}\n"));

        // When
        let tail = FileTranscripts.tail(&path).unwrap();

        // Then — la queue commence après le premier `\n` rencontré, donc sur une ligne
        // entière : ce qui reste est analysable, et la moitié de bourrage a disparu.
        assert_eq!(tail, "{\"last\":true}\n");
    }

    #[test]
    fn given_a_path_that_does_not_exist_when_its_tail_is_read_then_it_is_an_absence_not_an_error() {
        // Given — le cas courant d'un transcript effacé, ou d'un chemin d'un autre poste.
        let path = std::env::temp_dir().join("ash-usage-nowhere.jsonl");
        let _ = std::fs::remove_file(&path);

        // When
        let tail = FileTranscripts.tail(&path);

        // Then
        assert_eq!(tail, None);
    }
}
