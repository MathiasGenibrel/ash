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

//!
//! ## La fenêtre n'est plus supposée — elle est lue
//!
//! Il y avait ici une constante, `DEFAULT_CONTEXT_WINDOW = 200_000`, et son commentaire
//! annonçait déjà la panne : « une session de 1 M lira donc un pourcentage cinq fois trop
//! haut ». Elle disait vrai — la ligne de statut écrivait `ctx 28%` sur une conversation que
//! `/context` mesurait à 6 %, et croisait les deux seuils de couleur à 14 % et 18 % de place
//! réellement occupée.
//!
//! L'hypothèse qui l'avait fait naître, elle, reste **exacte** : le transcript nomme le
//! modèle (`"model":"claude-opus-5"`) sans jamais dire s'il tourne en 200 k ou en 1 M, et
//! aucun hook ne le dit non plus. Ce n'est donc pas là qu'il fallait chercher. Ce que le
//! transcript ne dit pas, la **configuration de l'outil** le dit — `"model": "opus[1m]"` — et
//! la lire est de la lecture au sens d'
//! [ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md) : aucun fichier
//! écrit, aucune autorisation macOS, aucun scan de disque, aucun appel réseau.
//!
//! Quatre règles en découlent, et elles gouvernent tout ce module :
//!
//! 1. **La table modèle → fenêtre appartient à l'adaptateur**
//!    ([`Adapter::context_window`]) : c'est lui, et lui seul, qui sait ce qu'`opus[1m]` veut
//!    dire. Un outil qui répond [`UsageSupport::None`] n'a ni table ni jauge.
//! 2. **Où le modèle est nommé appartient aussi à l'adaptateur**
//!    ([`Adapter::model_sources`]), et l'**ouverture** des fichiers à la feature
//!    ([`ToolConfig`]) — le partage exact d'`Instrumentation` et de [`Transcripts`].
//! 3. **Deux sources, un seul modèle.** La mesure vient du transcript et la fenêtre de la
//!    configuration : quand les deux ne nomment pas le même modèle, aucune fenêtre n'est
//!    posée. Un `settings.json` qui annonce `opus[1m]` pendant qu'un autre modèle tourne
//!    décrirait une session qui n'est pas celle qu'on mesure, et le pourcentage serait faux
//!    d'un facteur cinq — le bug de #162 sous un autre nom. C'est encore l'adaptateur qui
//!    arbitre, et **jusqu'où** il sait le faire lui appartient aussi : une configuration qui
//!    ne nomme qu'un alias (`opus[1m]`) n'a pas de version à confronter, et l'accord se fait
//!    alors sur la famille seule — la limite est écrite là où la règle vit, sur
//!    [`Adapter::context_window`].
//! 4. **Rien de reconnu ne vaut rien.** [`SessionUsage::window_tokens`] est une `Option`, et
//!    c'est elle qui rend « je ne sais pas » représentable. Sans elle, l'absence retomberait
//!    sur un défaut supposé, c'est-à-dire exactement sur le bug qu'on vient de corriger.
//!
//! ### Ce que cette lecture ne saura jamais
//!
//! **Un `/model` tapé en cours de session.** Claude Code n'en dit rien — ni dans le
//! transcript, ni sur le `stdin` d'un hook — et la configuration continue alors d'annoncer le
//! modèle du démarrage. C'est une limite documentée, pas un défaut à corriger : la
//! configuration est le meilleur signal qu'ADR-0006 et ADR-0007 autorisent, et lui en
//! inventer un autre serait retomber dans la devinette.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::adapter::Adapter;

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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    /// Les tokens que la conversation occupe — entrée, cache lu, cache écrit.
    ///
    /// **La sortie du dernier tour n'y est pas**, et c'est ce qui aligne ce nombre sur ce que
    /// `/context` affiche : la mesure est la taille du *prompt* de la dernière requête, pas
    /// celle de la réponse qui l'a suivie. Cette réponse entrera dans la fenêtre à la
    /// requête d'après, et sera comptée par les trois compteurs d'entrée. Il reste donc un
    /// **décalage d'un tour** — ce qui a été tapé et répondu depuis la dernière requête n'est
    /// pas encore mesuré —, et il se rattrape tout seul au tour suivant.
    ///
    /// **`number` et non `bigint`**, pour la raison écrite au long sur `state_since` : c'est
    /// un nombre JSON que la webview lit en `number`, et un compte de tokens ne s'approche
    /// pas de 2⁵³.
    #[cfg_attr(test, ts(type = "number"))]
    pub used_tokens: u64,
    /// La fenêtre dans laquelle ces tokens tiennent — **quand on la connaît**.
    ///
    /// `None` veut dire « Ash ne sait pas sur combien », et c'est une réponse à part entière.
    /// Deux chemins y mènent, que l'écran ne distingue pas : aucune source ne nomme de modèle
    /// reconnu, ou la configuration ne nomme **pas le modèle qui a tourné** — auquel cas sa
    /// fenêtre est celle d'une autre session que celle qu'on mesure. Dans les deux cas,
    /// l'écran montre la mesure sans la mettre en rapport (`ctx 57k`), sans barre et sans
    /// couleur de seuil. C'est le seul champ de tout le contrat dont
    /// l'absence a coûté un bug — un dénominateur supposé à 200 000 faisait lire `ctx 28%`
    /// sur une conversation qui occupait 6 % de sa fenêtre.
    ///
    /// Le numérateur, lui, **reste** : il est exact, et l'effacer avec le dénominateur serait
    /// perdre ce qu'Ash sait vraiment.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub window_tokens: Option<u64>,
    /// Le **nom court** du modèle qui a produit ce tour — `Opus 5`, `Opus 5 1M`.
    ///
    /// **Un nom, et pas un identifiant**, et c'est le seul champ du contrat qui traverse déjà
    /// mis en mots. La raison n'est pas la commodité de l'écran mais la propriété de la
    /// connaissance : `claude-opus-5` ne veut rien dire pour le cœur, et encore moins pour le
    /// frontend — c'est un mot de Claude Code, que seul son adaptateur sait traduire, à côté
    /// de la table qui traduit le même mot en fenêtre ([`Adapter::model_name`]). Faire
    /// traverser l'identifiant brut poserait la table dans le TypeScript, donc deux endroits
    /// qui devraient reconnaître les mêmes identifiants, et deux façons de se tromper le jour
    /// où un modèle change de nom.
    ///
    /// Ce n'est pas une **mise en forme** pour autant, et c'est ce qui le distingue du
    /// pourcentage qu'on a refusé de porter ici : l'écran ne peut rien recalculer d'un nom, il
    /// n'y a rien à en dériver, et aucune maquette future n'en voudra une autre écriture.
    ///
    /// `None` quand aucune source ne nomme de modèle, et quand le modèle nommé n'est reconnu
    /// par personne. Les deux effacent le segment entièrement — ni tiret, ni `unknown`, ni
    /// dernière valeur connue —, exactement comme une fenêtre inconnue efface la barre.
    #[cfg_attr(test, ts(type = "string | null"))]
    pub model: Option<String>,
}

/// Ce qu'un **tour d'assistant** du transcript déclare, lu en une seule fois.
///
/// Les deux champs viennent de la **même ligne**, et c'est la raison d'être de ce type : le
/// transcript écrit `"model":"claude-opus-5"` à côté du `usage` d'où viennent les tokens, si
/// bien que les lire séparément ferait deux parcours de la queue — et, pire, autoriserait le
/// nom d'un tour et la mesure d'un autre à se retrouver dans le même `SessionUsage`. Un tour
/// est une unité ; il se lit comme telle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// Les tokens que la conversation occupait **au moment où ce tour a été demandé**.
    ///
    /// Le prompt de la requête, et non la réponse : voir
    /// [`SessionUsage::used_tokens`], qui porte le raisonnement.
    pub used_tokens: u64,
    /// L'identifiant **brut** du modèle qui a produit ce tour, tel que l'outil l'a écrit.
    ///
    /// Brut parce que c'est ce que la ligne contient, et que le traduire est une autre
    /// question — celle d'[`Adapter::model_name`], qui a besoin de la configuration en plus.
    ///
    /// `None` quand la ligne ne le nomme pas : un format qui change, un tour qui ne porte que
    /// son `usage`. La mesure vaut alors toujours, et c'est seulement le nom qui manque.
    pub model: Option<String>,
}

/// Où un outil peut nommer le modèle avec lequel il tourne.
///
/// **C'est une adresse, jamais un contenu** : l'adaptateur la décrit, la feature la lit. Le
/// même partage que pour [`super::adapter::Instrumentation`], qui dit ce qu'il faut écrire
/// sans jamais écrire, et pour [`Transcripts`], qui ouvre ce que l'adaptateur sait ensuite
/// lire. Un adaptateur reste ainsi `Send + Sync` et sans effet de bord, donc testable sans
/// qu'aucun `cargo test` n'aille voir le vrai `~/.claude` de qui le lance.
///
/// Les deux variantes couvrent les deux formes qu'une configuration prend réellement : une
/// variable d'environnement, dont la valeur *est* l'identifiant, et un fichier JSON, dont une
/// clé le porte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    /// Une variable d'environnement — sa valeur est l'identifiant du modèle, telle quelle.
    Variable(String),
    /// Un fichier JSON, dont une clé de premier niveau nomme le modèle.
    ///
    /// La clé est portée par la variante plutôt que supposée : `model` est le mot de Claude
    /// Code, pas une convention du cœur, et le cœur n'a pas à le connaître pour descendre
    /// jusqu'à lui.
    JsonKey { path: PathBuf, key: String },
}

impl ModelSource {
    pub fn variable(name: impl Into<String>) -> Self {
        Self::Variable(name.into())
    }

    pub fn json_key(path: impl Into<PathBuf>, key: impl Into<String>) -> Self {
        Self::JsonKey {
            path: path.into(),
            key: key.into(),
        }
    }
}

/// Au-delà, un fichier de configuration n'en est plus un.
///
/// Même raisonnement que [`TRANSCRIPT_TAIL_BYTES`], et il compte pour la même raison : ce
/// chemin est parcouru à chaque hook, et un fichier de configuration qui aurait grossi — ou
/// qu'on aurait fait pointer sur autre chose — ne doit pas faire lire un mégaoctet dans le
/// tour d'un agent. Un `settings.json` réel pèse quelques centaines d'octets.
pub const MAX_CONFIG_BYTES: u64 = 64 * 1024;

/// Par où Ash lit la configuration d'un outil — et **rien qu'elle**.
///
/// Un trait pour la raison habituelle du dépôt : lire l'environnement et le disque sont des
/// effets système, et sans port, aucun scénario ne pourrait décrire un utilisateur dont le
/// `~/.claude/settings.json` porte `opus[1m]` sans toucher au foyer de qui lance les tests.
///
/// **Trois lectures, et aucune écriture.** C'est ce qui fait tenir la promesse d'ADR-0006 :
/// reconnaître est de la lecture. Rien ici n'ouvre de dialogue macOS, ne crée de fichier, ni
/// ne sort sur le réseau.
pub trait ToolConfig: Send + Sync {
    /// La valeur d'une variable d'environnement, ou rien.
    fn variable(&self, name: &str) -> Option<String>;

    /// Le contenu d'un fichier de configuration, borné à [`MAX_CONFIG_BYTES`], ou rien.
    ///
    /// `None` couvre toute la gamme des absences — fichier inexistant, droits refusés,
    /// dossier illisible. Aucune n'est une erreur : c'est une source qui ne dit rien, et la
    /// suivante a son tour.
    fn read(&self, path: &Path) -> Option<String>;

    /// Le dossier personnel, ou `None` si l'environnement n'en désigne aucun.
    ///
    /// Ici et pas dans l'adaptateur : `~` est une convention de shell, pas un dossier, et
    /// la résoudre est déjà toucher au monde.
    fn home(&self) -> Option<PathBuf>;
}

/// Le vrai lecteur : celui qui touche l'environnement et le disque.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemToolConfig;

impl ToolConfig for SystemToolConfig {
    fn variable(&self, name: &str) -> Option<String> {
        std::env::var_os(name)?.into_string().ok()
    }

    fn read(&self, path: &Path) -> Option<String> {
        let file = File::open(path).ok()?;
        let mut text = String::new();
        file.take(MAX_CONFIG_BYTES).read_to_string(&mut text).ok()?;
        Some(text)
    }

    fn home(&self) -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
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
    config: &dyn ToolConfig,
    transcript_path: Option<&str>,
    cwd: Option<&Path>,
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

    // L'adaptateur qui a **mesuré** est celui qu'on interroge sur la fenêtre et sur le nom, et
    // pas un autre : le numérateur et le dénominateur d'un même pourcentage ne peuvent pas
    // venir de deux outils différents, et le modèle qui a produit le tour non plus.
    let (adapter, turn) = readers.find_map(|adapter| {
        adapter
            .read_turn(&tail)
            .map(|turn| (adapter.as_ref(), turn))
    })?;

    // Lue **une fois** pour les deux questions qu'elle sert. C'est ce qui tient la promesse
    // « aucune lecture de fichier au-delà de celles d'avant » : nommer le modèle ne rouvre
    // rien, il relit ce que la fenêtre avait déjà fait ouvrir.
    let configured = configured_model(adapter, config, cwd);

    Some(SessionUsage {
        used_tokens: turn.used_tokens,
        // La fenêtre se lit dans la configuration — le transcript nomme le modèle sans jamais
        // dire s'il tourne en 200 k ou en 1 M, et c'est le suffixe `[1m]` d'un fichier de
        // réglages qui tranche —, mais elle se lit **contre** le modèle qui a réellement
        // tourné : les deux moitiés du pourcentage viennent de deux sources, et rien
        // n'oblige les deux à parler du même modèle. C'est l'adaptateur qui arbitre, parce
        // que lui seul sait qu'`opus[1m]` et `claude-opus-5` sont deux écritures d'une même
        // chose ([ADR-0008]).
        window_tokens: adapter.context_window(turn.model.as_deref(), configured.as_deref()),
        // Le nom, lui, vient d'abord du **transcript** : c'est ce qui a réellement tourné, donc
        // ce qui suit un `/model` changé en cours de session — au tour suivant, quand la
        // configuration, elle, n'aurait rien vu passer.
        model: turn
            .model
            .as_deref()
            .and_then(|ran| adapter.model_name(ran, configured.as_deref())),
    })
}

/// Le modèle que la configuration de l'outil nomme, ou rien.
///
/// **Le premier qui nomme répond**, du plus spécifique au moins spécifique — c'est l'ordre
/// que l'adaptateur a posé dans [`Adapter::model_sources`], et il reproduit celui de l'outil
/// lui-même. Une source qui nomme un modèle que l'adaptateur ne reconnaît pas **arrête** la
/// recherche : elle a répondu, et retomber sur la source suivante rendrait la configuration
/// que l'utilisateur a explicitement remplacée.
///
/// Ce qui sort est l'identifiant **brut**, pas une fenêtre : deux questions le lisent
/// désormais — la taille de la fenêtre et le suffixe `[1m]` du nom court —, et les faire
/// ouvrir chacune les mêmes fichiers doublerait le coût d'un chemin parcouru à chaque hook.
fn configured_model(
    adapter: &dyn Adapter,
    config: &dyn ToolConfig,
    cwd: Option<&Path>,
) -> Option<String> {
    let home = config.home();
    adapter
        .model_sources(cwd, home.as_deref())
        .iter()
        .find_map(|source| model_named_by(source, config))
}

/// Le modèle qu'une source nomme, s'il y en a un.
///
/// La seule moitié qui touche au monde, et la raison pour laquelle [`ModelSource`] est une
/// donnée : ce que l'adaptateur a décrit, la feature l'ouvre — un fichier absent, vide, qui
/// n'est pas du JSON, ou dont la clé est autre chose qu'une chaîne, ne nomme simplement rien.
fn model_named_by(source: &ModelSource, config: &dyn ToolConfig) -> Option<String> {
    let named = match source {
        ModelSource::Variable(name) => config.variable(name)?,
        ModelSource::JsonKey { path, key } => {
            let text = config.read(path)?;
            let object: serde_json::Value = serde_json::from_str(&text).ok()?;
            object.get(key)?.as_str()?.to_owned()
        }
    };

    let named = named.trim();
    (!named.is_empty()).then(|| named.to_owned())
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

    /// Le foyer de l'utilisateur du scénario — **jamais** celui de qui lance les tests.
    const HOME: &str = "/Users/x";

    /// Le dossier où l'agent tourne, tel que le `cwd` d'un hook le nomme.
    const CWD: &str = "/dev/ash";

    /// Le même, sous la forme que [`measure`] attend.
    fn cwd() -> Option<&'static Path> {
        Some(Path::new(CWD))
    }

    /// La configuration d'un outil que le scénario décrit, et qui **compte** ses lectures.
    ///
    /// Le compteur sert la même promesse que celui de [`CountingTranscripts`] : un onglet
    /// servi par un adaptateur muet ne doit ouvrir aucun fichier, et une absence d'ouverture
    /// ne se lit dans aucune valeur de retour.
    #[derive(Debug, Default)]
    struct CountingConfig {
        home: Option<std::path::PathBuf>,
        variables: std::collections::HashMap<String, String>,
        files: Mutex<std::collections::HashMap<std::path::PathBuf, String>>,
        reads: AtomicUsize,
    }

    impl CountingConfig {
        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }

        #[must_use]
        fn declaring(self, path: &str, model: &str) -> Self {
            self.files.lock().unwrap().insert(
                std::path::PathBuf::from(path),
                format!(r#"{{"model":"{model}"}}"#),
            );
            self
        }

        #[must_use]
        fn with_variable(mut self, name: &str, value: &str) -> Self {
            self.variables.insert(name.to_owned(), value.to_owned());
            self
        }
    }

    impl ToolConfig for CountingConfig {
        fn variable(&self, name: &str) -> Option<String> {
            self.variables.get(name).cloned()
        }

        fn read(&self, path: &Path) -> Option<String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.files.lock().unwrap().get(path).cloned()
        }

        fn home(&self) -> Option<std::path::PathBuf> {
            self.home.clone()
        }
    }

    /// Un utilisateur dont un seul fichier nomme un modèle.
    fn configured(path: &str, model: &str) -> CountingConfig {
        let path = path.replace('~', HOME);
        CountingConfig {
            home: Some(std::path::PathBuf::from(HOME)),
            ..CountingConfig::default()
        }
        .declaring(&path, model)
    }

    /// La mesure telle que le superviseur la demande, avec les vrais adaptateurs.
    fn measured_with(config: &CountingConfig, tail: &str) -> Option<SessionUsage> {
        let transcripts = CountingTranscripts::holding(TRANSCRIPT, tail);
        measure(
            &claude_code(),
            &transcripts,
            config,
            Some(TRANSCRIPT),
            cwd(),
        )
    }

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
        let config = CountingConfig::default();
        let measured = measure(&adapters, &transcripts, &config, Some(TRANSCRIPT), cwd());

        // Then — pas de mesure, et surtout **pas d'ouverture** : `UsageSupport::None` promet
        // aussi de ne rien coûter, et un onglet servi par `generic` ne paye pas une
        // entrée-sortie par hook. Ni pour le transcript, ni pour la configuration : la
        // fenêtre ne sert qu'à une jauge qu'il a déclaré ne pas avoir.
        assert_eq!(measured, None);
        assert_eq!((transcripts.reads(), config.reads()), (0, 0));
    }

    #[test]
    fn given_a_hook_that_names_no_transcript_when_it_is_measured_then_nothing_is_opened() {
        // Given — le cas de tous les hooks d'avant cette tranche, et de tout outil qui n'en
        // écrit pas : la trame n'a pas de chemin.
        let transcripts = CountingTranscripts::holding(TRANSCRIPT, A_TURN);

        // When
        let measured = measure(
            &claude_code(),
            &transcripts,
            &CountingConfig::default(),
            None,
            cwd(),
        );

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
        let measured = measure(
            &claude_code(),
            &transcripts,
            &configured("~/.claude/settings.json", "sonnet"),
            Some(TRANSCRIPT),
            cwd(),
        );

        // Then
        assert_eq!(
            measured,
            Some(SessionUsage {
                used_tokens: 900,
                window_tokens: Some(200_000),
                // La queue de ce scénario ne nomme aucun modèle : la mesure vaut, le nom
                // manque, et les deux absences sont indépendantes.
                model: None,
            })
        );
        assert_eq!(transcripts.reads(), 1);
    }

    /// Une queue qui déclare exactement 57 200 tokens — le chiffre du ticket.
    const FIFTY_SEVEN_THOUSAND: &str =
        r#"{"type":"assistant","message":{"usage":{"input_tokens":57200}}}"#;

    #[test]
    fn given_a_home_settings_naming_a_million_token_model_when_the_tab_is_measured_then_the_window_is_a_million(
    ) {
        // Given — le scénario exact du ticket : `~/.claude/settings.json` porte
        // `"model": "opus[1m]"`, et la session tourne donc dans une fenêtre d'un million. Le
        // transcript, lui, n'écrit que `claude-opus-5` : il ne dira jamais lequel des deux.
        let config = configured("~/.claude/settings.json", "opus[1m]");

        // When
        let measured = measured_with(&config, FIFTY_SEVEN_THOUSAND);

        // Then — 57 200 / 1 000 000, soit les 6 % que `/context` affiche dans cette session.
        // Le même transcript lu avec l'ancien dénominateur supposé en annonçait 29.
        assert_eq!(
            measured,
            Some(SessionUsage {
                used_tokens: 57_200,
                window_tokens: Some(1_000_000),
                model: None,
            })
        );
    }

    #[test]
    fn given_a_repository_that_names_its_own_model_when_the_tab_is_measured_then_it_beats_the_home_settings(
    ) {
        // Given — l'utilisateur tourne en `opus[1m]` partout, sauf dans ce dépôt-ci, dont le
        // `settings.local.json` déclare `sonnet`. Le fichier le plus proche du travail est
        // celui qui décrit le travail.
        let config = configured("~/.claude/settings.json", "opus[1m]")
            .declaring("/dev/ash/.claude/settings.local.json", "sonnet");

        // When
        let measured = measured_with(&config, FIFTY_SEVEN_THOUSAND);

        // Then
        assert_eq!(
            measured.and_then(|usage| usage.window_tokens),
            Some(200_000)
        );
    }

    #[test]
    fn given_the_model_variable_in_the_environment_when_the_tab_is_measured_then_it_beats_every_file(
    ) {
        // Given — les trois couches à la fois, chacune disant autre chose. L'ordre n'est pas
        // une préférence d'Ash : c'est celui de Claude Code, et la variable l'emporte.
        let config = configured("~/.claude/settings.json", "sonnet")
            .declaring("/dev/ash/.claude/settings.json", "sonnet")
            .declaring("/dev/ash/.claude/settings.local.json", "sonnet")
            .with_variable("ANTHROPIC_MODEL", "claude-opus-5[1m]");

        // When
        let measured = measured_with(&config, FIFTY_SEVEN_THOUSAND);

        // Then — et la forme longue de l'identifiant porte le suffixe aussi bien que l'alias
        // court : les deux existent en vrai dans les fichiers des utilisateurs.
        assert_eq!(
            measured.and_then(|usage| usage.window_tokens),
            Some(1_000_000)
        );
    }

    #[test]
    fn given_no_source_naming_a_recognized_model_when_the_tab_is_measured_then_the_window_is_unknown_and_the_measure_stays(
    ) {
        // Given — un utilisateur qui n'a jamais posé de `model` nulle part, ce qui est le cas
        // par défaut. C'est **le** scénario que l'ancien `DEFAULT_CONTEXT_WINDOW` traitait en
        // supposant 200 000, et qui faisait mentir la jauge d'un facteur cinq.
        let config = CountingConfig {
            home: Some(std::path::PathBuf::from(HOME)),
            ..CountingConfig::default()
        };

        // When
        let measured = measured_with(&config, FIFTY_SEVEN_THOUSAND);

        // Then — pas de dénominateur, donc pas de pourcentage ; mais le numérateur reste, et
        // l'écran lira `ctx 57k`. L'effacer serait perdre ce qu'Ash sait vraiment.
        assert_eq!(
            measured,
            Some(SessionUsage {
                used_tokens: 57_200,
                window_tokens: None,
                model: None,
            })
        );
    }

    #[test]
    fn given_a_repository_that_names_a_model_ash_does_not_know_when_the_tab_is_measured_then_the_search_stops_there(
    ) {
        // Given — le dépôt déclare un modèle qu'aucune table ne reconnaît, et le foyer un
        // `opus[1m]` parfaitement lisible. Retomber sur le foyer rendrait la fenêtre d'une
        // configuration que l'utilisateur a **explicitement** remplacée : la source la plus
        // spécifique a parlé, et ce qu'elle dit, Ash ne le comprend pas.
        let config = configured("~/.claude/settings.json", "opus[1m]")
            .declaring("/dev/ash/.claude/settings.json", "un-modèle-maison");

        // When
        let measured = measured_with(&config, FIFTY_SEVEN_THOUSAND);

        // Then — aucun pourcentage plutôt qu'un pourcentage calculé sur la mauvaise fenêtre.
        assert_eq!(
            measured.map(|usage| (usage.used_tokens, usage.window_tokens)),
            Some((57_200, None))
        );
    }

    #[test]
    fn given_a_hook_that_does_not_say_where_it_ran_when_the_tab_is_measured_then_only_the_home_settings_answer(
    ) {
        // Given — une trame sans `cwd` : un outil qui ne l'écrit pas, ou une trame dépouillée
        // par la borne du fil. Les deux couches du dépôt n'ont alors pas de chemin, et c'est
        // une dégradation honnête — pas une supposition.
        let config = configured("~/.claude/settings.json", "opus[1m]")
            .declaring("/dev/ash/.claude/settings.json", "sonnet");
        let transcripts = CountingTranscripts::holding(TRANSCRIPT, FIFTY_SEVEN_THOUSAND);

        // When
        let measured = measure(
            &claude_code(),
            &transcripts,
            &config,
            Some(TRANSCRIPT),
            None,
        );

        // Then — le foyer répond, et le `sonnet` du dépôt n'a même pas été ouvert.
        assert_eq!(
            measured.and_then(|usage| usage.window_tokens),
            Some(1_000_000)
        );
        assert_eq!(config.reads(), 1);
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

    /// Une queue dont le dernier tour nomme le modèle qui l'a produit — la forme réelle.
    fn turn_by(model: &str, used: u64) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"model":"{model}","usage":{{"input_tokens":{used}}}}}}}"#
        )
    }

    #[test]
    fn given_a_transcript_naming_opus_and_a_home_settings_carrying_the_suffix_when_the_tab_is_measured_then_it_carries_opus_five_one_m(
    ) {
        // Given — le scénario du ticket, de bout en bout : le transcript dit ce qui a tourné,
        // le `~/.claude/settings.json` porte `opus[1m]`, et aucune des deux sources ne
        // suffirait seule.
        let config = configured("~/.claude/settings.json", "opus[1m]");

        // When
        let measured = measured_with(&config, &turn_by("claude-opus-5", 57_200));

        // Then
        assert_eq!(
            measured,
            Some(SessionUsage {
                used_tokens: 57_200,
                window_tokens: Some(1_000_000),
                model: Some("Opus 5 1M".to_owned()),
            })
        );
    }

    #[test]
    fn given_a_transcript_naming_a_model_ash_cannot_name_when_the_tab_is_measured_then_neither_the_name_nor_the_window_is_borrowed(
    ) {
        // Given — la configuration nomme un modèle parfaitement connu, et le transcript un
        // modèle qui ne l'est pas — `claude-fable-5` existe vraiment dans des transcripts.
        // Retomber sur la configuration annoncerait un modèle qui n'a pas tourné : ni son nom,
        // ni sa fenêtre ne décrivent la conversation qu'on est en train de mesurer.
        let config = configured("~/.claude/settings.json", "opus[1m]");

        // When
        let measured = measured_with(&config, &turn_by("claude-fable-5", 57_200));

        // Then — ni tiret, ni `unknown`, ni pourcentage calculé sur la fenêtre d'un autre ; la
        // mesure, elle, reste — c'est la seule chose qu'Ash sache vraiment.
        assert_eq!(
            measured,
            Some(SessionUsage {
                used_tokens: 57_200,
                window_tokens: None,
                model: None,
            })
        );
    }

    #[test]
    fn given_a_session_running_a_model_the_configuration_does_not_announce_when_the_tab_is_measured_then_the_gauge_disappears(
    ) {
        // Given — le désaccord observé en vrai : le `~/.claude/settings.json` annonce
        // `opus[1m]`, et le transcript que le hook a nommé tourne en `claude-sonnet-5`. Ça
        // arrive à chaque session que l'utilisateur n'a pas configurée lui-même — une revue
        // de sécurité, un agent qui choisit son modèle — et à chaque `/model` changé en cours
        // de route. Les deux moitiés du pourcentage parlent alors de deux modèles.
        let config = configured("~/.claude/settings.json", "opus[1m]");

        // When
        let measured = measured_with(&config, &turn_by("claude-sonnet-5", 133_670));

        // Then — pas de dénominateur : `133 670 / 1 000 000` aurait affiché `ctx 13%` là où
        // `/context` en montre 67. La mesure et le nom du modèle qui a tourné, eux, restent.
        assert_eq!(
            measured,
            Some(SessionUsage {
                used_tokens: 133_670,
                window_tokens: None,
                // Et le nom ne porte pas de `1M` : ce suffixe vient d'une configuration qui
                // ne parle pas de ce modèle, exactement comme la fenêtre qu'il décrit.
                model: Some("Sonnet 5".to_owned()),
            })
        );
    }

    #[test]
    fn given_a_tab_whose_model_is_named_when_it_is_measured_then_no_file_is_opened_beyond_the_two_the_gauge_already_needed(
    ) {
        // Given — le critère qui interdit à cette tranche de coûter une entrée-sortie : le nom
        // se lit dans la queue déjà tirée, et son suffixe dans la configuration déjà ouverte.
        let config = configured("~/.claude/settings.json", "opus[1m]");
        let transcripts = CountingTranscripts::holding(TRANSCRIPT, &turn_by("claude-opus-5", 900));

        // When
        let measured = measure(
            &claude_code(),
            &transcripts,
            &config,
            Some(TRANSCRIPT),
            cwd(),
        );

        // Then — une lecture de transcript, et les trois fichiers de configuration que la
        // fenêtre faisait déjà ouvrir (les deux du dépôt, puis celui du foyer). Nommer le
        // modèle n'en a ajouté aucun.
        assert_eq!(
            measured.and_then(|usage| usage.model).as_deref(),
            Some("Opus 5 1M")
        );
        assert_eq!((transcripts.reads(), config.reads()), (1, 3));
    }
}
