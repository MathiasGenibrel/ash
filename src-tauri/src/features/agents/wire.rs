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
/// en envoyant une ligne sans fin. 8 Kio laissent trois ordres de grandeur au-dessus d'une
/// trame réelle — un ulid et un mot — et une trame plus longue est rejetée sans être
/// accumulée.
pub const MAX_FRAME_BYTES: usize = 8 * 1024;

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
        }
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

        let frame: EventFrame = serde_json::from_str(line).map_err(|_| WireError::Malformed)?;
        if frame.tab_id.trim().is_empty() || frame.kind.trim().is_empty() {
            return Err(WireError::Empty);
        }
        Ok(frame)
    }
}

/// Le dossier privé d'Ash, celui d'`~/.ash/config.toml`
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
