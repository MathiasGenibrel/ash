//! Le protocole du socket d'événements : son **adresse** et son **format**.
//!
//! Ce fichier est le seul endroit où le client et le serveur se rencontrent, et il est
//! délibérément compilé **deux fois** : une fois dans la bibliothèque, une fois dans le
//! binaire `ash-event`, qui l'inclut par `#[path]` (voir `src/bin/ash-event.rs`). C'est ce
//! qui garantit qu'ils ne peuvent pas diverger, sans que `ash-event` ait à dépendre de
//! `ash_lib` — le lier à Tauri coûterait à chaque hook le chargement de WebKit et d'AppKit
//! pour écrire une ligne sur un socket.
//!
//! Il ne dépend donc que de `std`, `serde` et `serde_json`, et de rien du reste du crate.
//!
//! Ce qui traverse ici est un **format de transport**, pas un état d'agent : `EventFrame`
//! porte ce qu'un hook a dit, tel quel. Le traduire en l'un des cinq mots d'`AgentState`
//! est le travail de l'adaptateur d'[ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md),
//! et la machine à états d'ADR-0007 §6.4 décide ensuite quoi en faire.

use std::path::PathBuf;

/// Taille maximale d'une trame, terminateur compris.
///
/// **C'est une frontière de sécurité, pas un réglage.** Le serveur lit sur un socket que
/// n'importe quel processus du même utilisateur peut ouvrir ; sans borne, un client
/// hostile ou simplement cassé ferait grossir un tampon jusqu'à épuiser la mémoire d'Ash
/// en envoyant une ligne sans fin. Une trame réelle — un ulid et un mot — pèse une
/// cinquantaine d'octets ; 8 Kio laissent donc deux ordres de grandeur de marge, et une
/// trame plus longue est rejetée sans être accumulée.
pub const MAX_FRAME_BYTES: usize = 8 * 1024;

/// Au-delà, une clé d'enfant n'en est plus une.
///
/// **C'est ce qui rend inatteignable le repli silencieux d'`ash-event`** : quand une trame
/// déborde [`MAX_FRAME_BYTES`], le client la repost sans son enfant plutôt que de perdre
/// l'état déclaré (`bin/ash-event.rs`), et une ligne fille disparaîtrait alors sans un mot.
/// Deux clés bornées à 256 octets pèsent au plus un demi-kilo-octet : la trame ne peut plus
/// déborder à cause d'elles, et le repli ne peut plus se déclencher pour cette raison-là.
///
/// Une clé plus longue est **écartée**, jamais tronquée : un `agent_id` coupé désignerait un
/// enfant qui n'existe pas, et deux frères tronqués au même préfixe se confondraient. Ce que
/// l'on perd en écartant est une ligne d'affichage ; ce que l'on perdrait en tronquant est
/// l'identité qui apparie un `SubagentStop` à sa ligne.
///
/// 256 octets sont deux ordres de grandeur au-dessus du réel : `agent_id` est un identifiant
/// d'outil, `agent_type` le nom d'un sous-agent — `code-reviewer`, `dev-integration`.
pub const MAX_CHILD_KEY_BYTES: usize = 256;

/// Au-delà, un chemin n'en est plus un — le transcript comme le `cwd`.
///
/// Même raisonnement que [`MAX_CHILD_KEY_BYTES`], et même conduite : un chemin plus long est
/// **écarté**, jamais tronqué — un chemin coupé désignerait un autre fichier, ou aucun, et
/// Ash l'ouvrirait. 1024 octets sont la limite d'un chemin sur macOS (`PATH_MAX`), donc tout
/// chemin qui existe réellement passe.
///
/// Il est borné ici pour la même raison que les clés d'enfant : que la trame ne puisse pas
/// déborder [`MAX_FRAME_BYTES`] à cause de lui, donc que le repli silencieux
/// d'`ash-event` ne puisse pas se déclencher pour cette raison-là.
pub const MAX_PATH_BYTES: usize = 1024;

/// Ce qu'un hook envoie à Ash, tel qu'il passe sur le fil.
///
/// Une ligne de JSON par événement, terminée par `\n` : le cadrage est le retour à la
/// ligne, ce qui rend le flux inspectable à l'œil pendant qu'on met au point un bloc de
/// hooks, et lisible sans état d'analyse côté serveur.
///
/// Les champs inconnus sont **ignorés** à la lecture : un `ash-event` plus récent qu'Ash —
/// le cas normal après une mise à jour, puisque le binaire est appelé depuis la
/// configuration de l'outil — ne doit pas rendre l'événement illisible.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EventFrame {
    /// L'onglet concerné, et la **seule** corrélation admise
    /// ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)) : ni le `cwd`, ni un
    /// horodatage, ni le pid. C'est `ASH_TAB_ID`, hérité par toute la descendance du shell.
    pub tab_id: String,
    /// Ce que le hook a déclaré — `working`, `waiting`, `done`… en toutes lettres.
    ///
    /// Une chaîne, pas une énumération, et c'est volontaire : le transport n'a pas à
    /// connaître le vocabulaire d'un outil. Un mot qu'Ash ne sait pas interpréter doit
    /// arriver quand même, pour que l'adaptateur puisse le refuser en connaissance de
    /// cause plutôt que de ne jamais le voir.
    pub kind: String,
    /// L'enfant qui a produit l'événement, quand il vient d'un sous-agent
    /// ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), amendement du
    /// 2026-08-13).
    ///
    /// **Facultatif, et subordonné à l'onglet** : `ASH_TAB_ID` reste la seule corrélation
    /// d'un événement à un onglet, et cette clé ne fait que désigner un enfant *à
    /// l'intérieur* d'un onglet déjà corrélé. Une trame sans elle est valide et concerne
    /// l'agent principal — c'est le cas de toutes celles qu'Ash a reçues jusqu'ici.
    ///
    /// Elle vient de l'entrée standard du hook, pas de sa ligne de commande : c'est l'outil
    /// qui la donne, et lui seul sait s'il y a un enfant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Le type de l'enfant tel que l'outil le nomme — `code-reviewer`, `general-purpose`.
    ///
    /// Facultatif pour la même raison, et jamais interprété ici : le transport le porte,
    /// il ne le traduit pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// Le transcript de la conversation, quand l'outil en tient un et le nomme.
    ///
    /// **C'est un chemin, jamais un contenu.** La trame est un format de transport, et faire
    /// lire un fichier à `ash-event` mettrait une lecture de disque sur le chemin d'un hook,
    /// c'est-à-dire dans le tour d'un agent. Ce qui traverse est donc l'adresse ; c'est
    /// `features/agents` qui décide s'il y a lieu de la lire, et son adaptateur qui sait ce
    /// qu'on y trouve (`usage.rs`).
    ///
    /// **Facultatif, et sans conséquence sur l'état** : il ne se traduit en rien qu'
    /// [`AgentState`](crate::features::agents::AgentState) connaisse. Une trame sans lui est
    /// valide — c'est le cas de toutes celles qu'Ash a reçues avant cette tranche, et de
    /// celles de tout outil qui n'écrit pas de transcript.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    /// Le dossier où l'outil tournait quand le hook est parti.
    ///
    /// **Ce champ n'est pas une corrélation, et il ne doit jamais en devenir une.**
    /// [ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md) est explicite : `ASH_TAB_ID`
    /// est « la seule corrélation admise : ni le `cwd`, ni un horodatage, ni le pid ». Rien
    /// n'est changé à cette règle. Ce qui voyage ici est une **donnée** que le hook a écrite
    /// sur son entrée standard, au même titre que `transcript_path` — une adresse qu'Ash
    /// pourra lire, jamais un moyen de retrouver un onglet. Une trame qui le porterait sans
    /// `tab_id` n'irait nulle part, exactement comme aujourd'hui.
    ///
    /// À quoi il sert : la fenêtre de contexte d'un agent est nommée dans la configuration de
    /// l'outil, et celle du **dépôt** (`.claude/settings.local.json`) l'emporte sur celle du
    /// foyer. Sans ce dossier, ces deux couches-là n'ont pas de chemin, et la jauge lit la
    /// fenêtre du foyer là où le projet en déclare une autre.
    ///
    /// Facultatif, et sans conséquence sur l'état : il ne se traduit en rien qu'
    /// [`AgentState`](crate::features::agents::AgentState) connaisse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// Ce qui peut clocher dans une ligne reçue.
#[derive(Debug, PartialEq, Eq)]
pub enum WireError {
    /// La ligne n'est pas du JSON, ou pas la forme attendue.
    Malformed,
    /// Trame vide, ou dont un champ obligatoire est vide.
    Empty,
    /// Plus de [`MAX_FRAME_BYTES`] : rejetée sans être accumulée.
    TooLong,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Malformed => write!(f, "trame illisible"),
            WireError::Empty => write!(f, "trame vide"),
            WireError::TooLong => write!(f, "trame trop longue"),
        }
    }
}

impl EventFrame {
    pub fn new(kind: impl Into<String>, tab_id: impl Into<String>) -> Self {
        Self {
            tab_id: tab_id.into(),
            kind: kind.into(),
            agent_id: None,
            agent_type: None,
            transcript_path: None,
            cwd: None,
        }
    }

    /// La même trame, en nommant le dossier où l'outil tournait.
    ///
    /// Un dossier vide, blanc ou démesuré vaut « on ne sait pas où » — et la fenêtre se lira
    /// alors sans les deux couches du dépôt, ce qui est une dégradation, pas une supposition.
    #[must_use]
    pub fn with_cwd(mut self, cwd: Option<&str>) -> Self {
        self.cwd = path(cwd);
        self
    }

    /// La même trame, en nommant le transcript que l'outil a désigné.
    ///
    /// Un chemin vide, blanc ou démesuré vaut « pas de transcript » : Ash n'ouvre que ce
    /// qu'on lui a réellement nommé.
    #[must_use]
    pub fn with_transcript(mut self, value: Option<&str>) -> Self {
        self.transcript_path = path(value);
        self
    }

    /// La même trame, en nommant l'enfant qui l'a produite.
    ///
    /// Une chaîne vide ou blanche vaut « pas d'enfant » : un outil qui pose la clé sans la
    /// remplir ne doit pas faire exister un sous-agent anonyme, que rien ne saurait
    /// distinguer d'un autre.
    #[must_use]
    pub fn with_subagent(mut self, agent_id: Option<&str>, agent_type: Option<&str>) -> Self {
        self.agent_id = named(agent_id);
        self.agent_type = named(agent_type);
        self
    }

    /// La même trame, dépouillée de ce qui n'est pas l'état déclaré.
    ///
    /// C'est le repli quand la trame déborde [`MAX_FRAME_BYTES`] : l'état d'un onglet est
    /// ce qu'un hook existe pour transporter, et ni un `agent_type` démesuré ni un chemin de
    /// transcript ne doivent l'emporter avec eux dans le fossé (voir `bin/ash-event.rs`).
    ///
    /// Ce qu'on perd en dépouillant est une ligne fille et une jauge ; ce qu'on perdrait en
    /// laissant la trame déborder est l'état lui-même.
    #[must_use]
    pub fn without_extras(mut self) -> Self {
        self.agent_id = None;
        self.agent_type = None;
        self.transcript_path = None;
        self.cwd = None;
        self
    }

    /// La trame telle qu'elle part sur le fil, terminateur compris.
    ///
    /// Rend une erreur plutôt qu'une ligne vide si la sérialisation échoue : `ash-event`
    /// n'a pas le droit de paniquer dans un hook.
    pub fn to_line(&self) -> Result<String, WireError> {
        let mut line = serde_json::to_string(self).map_err(|_| WireError::Malformed)?;
        line.push('\n');
        (line.len() <= MAX_FRAME_BYTES)
            .then_some(line)
            .ok_or(WireError::TooLong)
    }

    /// L'inverse, avec la validation que le serveur doit faire **avant** de livrer quoi que
    /// ce soit : une trame sans onglet ou sans verbe n'a rien à faire plus loin.
    pub fn from_line(line: &str) -> Result<Self, WireError> {
        if line.len() > MAX_FRAME_BYTES {
            return Err(WireError::TooLong);
        }
        let line = line.trim();
        if line.is_empty() {
            return Err(WireError::Empty);
        }

        let mut frame: EventFrame = serde_json::from_str(line).map_err(|_| WireError::Malformed)?;
        if frame.tab_id.trim().is_empty() || frame.kind.trim().is_empty() {
            return Err(WireError::Empty);
        }
        // Une clé d'enfant vide n'en est pas une : la normaliser ici évite que chaque
        // lecteur ait à se demander si `Some("")` désigne quelqu'un.
        frame.agent_id = named(frame.agent_id.as_deref());
        frame.agent_type = named(frame.agent_type.as_deref());
        frame.transcript_path = path(frame.transcript_path.as_deref());
        frame.cwd = path(frame.cwd.as_deref());
        Ok(frame)
    }
}

/// Une valeur qui désigne réellement quelque chose, ou rien.
///
/// Le seul normalisateur des deux clés d'enfant, appelé des **deux** côtés du fil : ce qui
/// est vide ne désigne personne, et ce qui est démesuré n'est plus un identifiant (voir
/// [`MAX_CHILD_KEY_BYTES`]).
fn named(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= MAX_CHILD_KEY_BYTES)
        .map(str::to_owned)
}

/// Un chemin qui désigne réellement quelque chose de possible, ou rien.
///
/// Le pendant de [`named`] pour les deux clés de chemin — le transcript et le `cwd` —, et
/// appelé des **deux** côtés du fil pour la même raison : ce qui est vide ne désigne rien, et
/// ce qui dépasse [`MAX_PATH_BYTES`] n'est plus un chemin.
fn path(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= MAX_PATH_BYTES)
        .map(str::to_owned)
}

/// Le dossier privé d'Ash, celui d'`~/.ash/tools.json` et du socket
/// ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
pub fn ash_directory() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".ash")
}

/// L'adresse du socket d'événements, et la valeur qu'`ASH_SOCK` porte dans chaque onglet.
///
/// **La spec §6.3 dessine `/tmp/ash-<uid>.sock` ; on retient `~/.ash/ash.sock`.** Le
/// suffixe `<uid>` de la spec n'existe que parce que `/tmp` est partagé entre tous les
/// utilisateurs de la machine : c'est un contournement d'un problème qu'on peut ne pas
/// avoir. `~/.ash/` est privé par construction, il porte déjà la configuration d'ADR-0007,
/// et surtout il donne gratuitement la protection qu'on cherche — le dossier est créé en
/// `0700`, donc aucun autre utilisateur ne peut même traverser jusqu'au socket, ce qui
/// ferme la fenêtre entre le `bind` et la pose des permissions du fichier. Un socket dans
/// `/tmp` ne survivrait pas non plus au nettoyage périodique du système.
///
/// La limite d'un chemin de socket unix est de 104 octets sur macOS ; `~/.ash/ash.sock`
/// laisse largement la place, y compris pour un nom d'utilisateur long.
pub fn socket_path() -> PathBuf {
    ash_directory().join("ash.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_an_event_posted_by_a_hook_when_it_crosses_the_socket_then_it_arrives_with_the_same_tab_and_verb(
    ) {
        // Given — les deux côtés du fil sont compilés séparément, `ash-event` n'ayant pas
        // le droit de dépendre de la bibliothèque. Ce test est ce qui tient l'aller et le
        // retour ensemble.
        let sent = EventFrame::new("working", "01J0TAB");

        // When
        let line = sent.to_line().unwrap();
        let received = EventFrame::from_line(&line).unwrap();

        // Then
        assert!(line.ends_with('\n'), "le cadrage est le retour à la ligne");
        assert_eq!(received, sent);
    }

    #[test]
    fn given_a_frame_from_a_newer_ash_event_when_it_is_read_then_its_unknown_fields_do_not_hide_the_event(
    ) {
        // Given — `ash-event` est appelé depuis la configuration de l'outil, donc il peut
        // être plus récent qu'Ash après une mise à jour. Un champ ajouté demain ne doit pas
        // faire disparaître un `waiting` d'aujourd'hui.
        let line = r#"{"tab_id":"01J0TAB","kind":"waiting","session":"abc"}"#;

        // When
        let frame = EventFrame::from_line(line);

        // Then
        assert_eq!(frame, Ok(EventFrame::new("waiting", "01J0TAB")));
    }

    #[test]
    fn given_a_frame_without_a_tab_when_it_is_read_then_it_is_refused_before_delivery() {
        // Given — la corrélation se fait par `ASH_TAB_ID` et par rien d'autre (ADR-0007) :
        // une trame qui n'en porte pas ne peut pas être rattrapée par une devinette.
        let lines = [
            r#"{"tab_id":"","kind":"working"}"#,
            r#"{"tab_id":"01J0TAB","kind":"  "}"#,
            r#"{"kind":"working"}"#,
            "",
            "pas du json",
        ];

        // When
        let read: Vec<_> = lines
            .iter()
            .map(|line| EventFrame::from_line(line))
            .collect();

        // Then
        assert!(read.iter().all(|frame| frame.is_err()), "{read:?}");
    }

    #[test]
    fn given_an_event_produced_inside_a_subagent_when_it_crosses_the_socket_then_the_child_is_named_under_its_tab(
    ) {
        // Given — l'amendement du 2026-08-13 à ADR-0007 : l'onglet reste la seule
        // corrélation d'un événement à un onglet, et `agent_id` ne fait qu'y désigner un
        // enfant. Les deux voyagent donc **ensemble**, jamais l'un sans l'autre.
        let sent = EventFrame::new("working", "01J0TAB")
            .with_subagent(Some("agent-7"), Some("code-reviewer"));

        // When
        let line = sent.to_line().unwrap();
        let received = EventFrame::from_line(&line).unwrap();

        // Then
        assert_eq!(received, sent);
        assert_eq!(received.tab_id, "01J0TAB");
    }

    #[test]
    fn given_a_frame_from_the_main_agent_when_it_crosses_the_socket_then_it_carries_no_child_at_all(
    ) {
        // Given — la moitié qu'il ne faut pas casser : c'est la trame que tous les hooks
        // d'aujourd'hui envoient. Elle doit rester **identique** sur le fil, sans quoi un
        // Ash plus ancien qu'`ash-event` lirait des clés vides là où il n'y a personne.
        let alone = EventFrame::new("waiting", "01J0TAB");

        // When
        let line = alone.to_line().unwrap();

        // Then
        assert_eq!(line, "{\"tab_id\":\"01J0TAB\",\"kind\":\"waiting\"}\n");
        assert_eq!(EventFrame::from_line(&line), Ok(alone));
    }

    #[test]
    fn given_a_frame_whose_child_keys_are_blank_when_it_is_read_then_no_anonymous_child_is_invented(
    ) {
        // Given — un outil qui pose la clé sans la remplir. Un enfant sans identité ne se
        // distingue d'aucun autre : le représenter serait pire que de l'ignorer.
        let line = r#"{"tab_id":"01J0TAB","kind":"working","agent_id":"","agent_type":"  "}"#;

        // When
        let frame = EventFrame::from_line(line);

        // Then
        assert_eq!(frame, Ok(EventFrame::new("working", "01J0TAB")));
    }

    #[test]
    fn given_a_child_key_far_longer_than_any_identifier_when_the_frame_is_built_then_it_never_reaches_the_wire(
    ) {
        // Given — sans borne ici, une clé démesurée ferait déborder la trame, et `ash-event`
        // la reposterait **sans l'enfant** pour sauver l'état déclaré : la ligne fille
        // disparaîtrait sans que rien ne l'explique. Borner la clé rend ce repli
        // inatteignable pour cette raison-là, et l'écart se voit ici plutôt que nulle part.
        let frame = EventFrame::new("working", "01J0TAB")
            .with_subagent(Some("agent-7"), Some(&"z".repeat(MAX_CHILD_KEY_BYTES + 1)));

        // When
        let line = frame.to_line();

        // Then — l'enfant reste identifié, et seul le nom démesuré est écarté ; écarté et
        // non tronqué, parce qu'un identifiant coupé désignerait quelqu'un d'autre
        assert_eq!(frame.agent_id.as_deref(), Some("agent-7"));
        assert_eq!(frame.agent_type, None);
        assert!(line.is_ok(), "{line:?}");
    }

    #[test]
    fn given_a_forged_frame_whose_child_key_is_gigantic_when_the_server_reads_it_then_the_child_is_dropped_and_the_state_still_arrives(
    ) {
        // Given — le socket est ouvert à tout processus du même utilisateur, et `to_line`
        // n'est pas sur son chemin : une trame forgée à la main n'a jamais vu `named()` côté
        // client. La borne doit donc mordre **des deux côtés du fil**, sinon un `agent_id` de
        // dix kilo-octets deviendrait une ligne fille indélogeable dans la colonne.
        let line = format!(
            r#"{{"tab_id":"01J0TAB","kind":"working","agent_id":"{}","agent_type":"explore"}}"#,
            "z".repeat(MAX_CHILD_KEY_BYTES + 1)
        );

        // When
        let frame = EventFrame::from_line(&line);

        // Then — l'identité est écartée, jamais tronquée, et l'état déclaré arrive quand même.
        // Sans `agent_id`, plus rien ne désigne un enfant : aucune ligne fille ne peut naître
        // de cette trame, quoi qu'elle porte par ailleurs.
        let frame = frame.unwrap_or_else(|why| panic!("{why}"));
        assert_eq!(frame.agent_id, None);
        assert_eq!(
            (frame.tab_id.as_str(), frame.kind.as_str()),
            ("01J0TAB", "working")
        );
    }

    #[test]
    fn given_a_hook_that_says_where_it_ran_when_it_crosses_the_socket_then_the_folder_travels_without_ever_correlating(
    ) {
        // Given — ADR-0007 est explicite : `ASH_TAB_ID` est « la seule corrélation admise :
        // ni le `cwd`, ni un horodatage, ni le pid ». Ce champ ne change rien à cette règle —
        // il transporte une **donnée**, l'adresse d'un dossier dont Ash lira la configuration
        // pour connaître la fenêtre de contexte de l'agent.
        let sent = EventFrame::new("waiting", "01J0TAB")
            .with_transcript(Some("/tmp/t.jsonl"))
            .with_cwd(Some("/dev/ash"));

        // When
        let received = EventFrame::from_line(&sent.to_line().unwrap());
        let orphaned = EventFrame::from_line(r#"{"tab_id":"","kind":"waiting","cwd":"/dev/ash"}"#);

        // Then — le dossier fait l'aller-retour intact, et une trame qui ne porterait *que*
        // lui n'a toujours nulle part où aller : le `cwd` n'a pas ouvert une seconde façon de
        // retrouver un onglet.
        assert_eq!(received, Ok(sent));
        assert_eq!(orphaned, Err(WireError::Empty));
    }

    #[test]
    fn given_a_frame_that_overflows_the_wire_when_it_is_stripped_then_the_folder_leaves_with_the_rest(
    ) {
        // Given — le repli d'`ash-event` : l'état est ce qu'un hook existe pour transporter,
        // et tout ce qui n'est pas lui tombe plutôt que de l'emporter dans le fossé. Le `cwd`
        // en fait partie, comme le transcript et les clés d'enfant.
        let frame = EventFrame::new("waiting", "01J0TAB")
            .with_transcript(Some("/tmp/t.jsonl"))
            .with_cwd(Some("/dev/ash"))
            .with_subagent(Some("agent-7"), Some("explore"));

        // When
        let stripped = frame.without_extras();

        // Then
        assert_eq!(stripped, EventFrame::new("waiting", "01J0TAB"));
    }

    #[test]
    fn given_a_gigantic_line_when_it_is_read_then_it_is_refused_instead_of_being_parsed() {
        // Given — un client du même utilisateur peut écrire ce qu'il veut sur le socket ;
        // la borne est ce qui empêche une ligne sans fin de faire grossir Ash.
        let line = format!(
            r#"{{"tab_id":"01J0TAB","kind":"{}"}}"#,
            "w".repeat(MAX_FRAME_BYTES)
        );

        // When
        let frame = EventFrame::from_line(&line);

        // Then
        assert_eq!(frame, Err(WireError::TooLong));
    }
}
