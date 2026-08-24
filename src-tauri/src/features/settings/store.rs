//! Où les outils déclarés se gardent d'une session à l'autre — `~/.ash/tools.json`.
//!
//! Le quatrième effet système de la feature, et le premier qui écrive **chez Ash** plutôt
//! que chez l'utilisateur : `HookBlocks` pose des entrées dans un `settings.json` qui ne
//! nous appartient pas, celui-ci écrit un fichier dont Ash est seul propriétaire. Les deux
//! passent par un trait, pour la même raison — vérifier qu'une déclaration survit au
//! redémarrage ne doit pas demander d'écrire dans le `$HOME` de qui lance `cargo test`.
//!
//! **Le format est JSON, et non le TOML que la spec §9 décrivait.** Les quatre magasins qui
//! existaient déjà — `theme.json`, `notifications.json`, `shortcuts.json`, `state.json` —
//! sont en JSON, `serde_json` est là, et un cinquième format aurait demandé une dépendance
//! de plus pour écrire quatre champs. La spec est corrigée en conséquence (§9, §10).

use std::path::PathBuf;

use super::error::SettingsError;
use super::persisted::PersistedTools;

/// Le magasin des outils déclarés.
///
/// `load` ne rend pas d'erreur, et c'est une décision : un fichier absent, vide, illisible
/// ou abîmé se lit « aucune entrée retenue ». Rien de ce qui est écrit ici ne vaut
/// d'empêcher une fenêtre d'ouvrir — même règle que `~/.ash/state.json`.
pub trait ToolStore: Send + Sync {
    fn load(&self) -> PersistedTools;
    fn save(&self, tools: &PersistedTools) -> Result<(), SettingsError>;
}

/// `~/.ash/tools.json` (spec §9.2, §10).
///
/// Le dossier existe déjà : le socket d'events, `theme.json` et `state.json` y vivent. Le
/// fichier est lisible à l'œil nu et s'édite à la main — c'est ce que la spec §9 promet de
/// la liste des commandes reconnues, et ce que la spec §10 promet du retrait : « suppression
/// du dossier ».
pub struct FileToolStore {
    path: PathBuf,
}

impl FileToolStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::at(home.join(".ash").join("tools.json"))
    }
}

impl ToolStore for FileToolStore {
    fn load(&self) -> PersistedTools {
        std::fs::read_to_string(&self.path)
            .ok()
            .map(|content| decode(&content))
            .unwrap_or_default()
    }

    fn save(&self, tools: &PersistedTools) -> Result<(), SettingsError> {
        let io = |why: std::io::Error| SettingsError::NotSaved(why.to_string());

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&self.path, encode(tools)).map_err(io)
    }
}

/// Ce que le fichier dit, ou rien du tout.
///
/// **Tolérante à tout.** Un fichier tronqué, vidé, ou rempli d'autre chose rend une liste
/// vide ; une entrée abîmée au milieu d'entrées valides tombe seule
/// ([`PersistedTools`]). Ce qui reste est ensuite jugé par les mêmes constructeurs que ce
/// qu'on tape dans le formulaire — voir [`NewTool::restore`](super::tool::NewTool::restore).
fn decode(content: &str) -> PersistedTools {
    serde_json::from_str(content).unwrap_or_default()
}

fn encode(tools: &PersistedTools) -> String {
    // `to_string_pretty`, comme `state.json` : ce fichier grandit avec le nombre d'outils
    // déclarés, et la spec §9 dit qu'il s'édite à la main.
    format!(
        "{}\n",
        serde_json::to_string_pretty(tools).unwrap_or_else(|_| String::from("{}"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::settings::persisted::PersistedTool;

    /// Test Data Builder : une entrée persistée dont on ne décrit que ce qui compte.
    fn entry(command: &str) -> PersistedTool {
        PersistedTool {
            command: command.to_owned(),
            label: None,
            adapter: "claude-code".to_owned(),
            config: Some("~/.claude".to_owned()),
            last_valid_config: None,
        }
    }

    #[test]
    fn given_a_declared_tool_when_the_file_is_written_then_it_holds_the_declaration_and_its_memory_and_nothing_else(
    ) {
        // Given — une **garantie de non-écriture** : le jour où quelqu'un ajoutera au
        // fichier le résultat des quatre tests ou l'état des hooks, ce test tombe. Les deux
        // sont des faits datés sur la machine (ADR-0007) : relus d'un fichier, ils seraient
        // un souvenir présenté comme une lecture
        let mut kept = entry("claude");
        kept.label = Some("Pro".to_owned());
        kept.last_valid_config = Some("~/.claude".to_owned());

        // When
        let written = encode(&PersistedTools { tools: vec![kept] });

        // Then
        let parsed: serde_json::Value =
            serde_json::from_str(&written).expect("le fichier écrit est du JSON");
        let mut keys: Vec<&str> = parsed["tools"][0]
            .as_object()
            .expect("une entrée est un objet")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["adapter", "command", "config", "label", "last_valid_config"]
        );
    }

    #[test]
    fn given_an_entry_without_label_or_memory_when_it_is_written_then_the_empty_keys_are_absent_not_null(
    ) {
        // Given — le fichier s'édite à la main (spec §9) : une clé à `null` fait chercher
        // ce qu'elle veut dire, là où son absence dit « rien à ce sujet »
        let plain = PersistedTools {
            tools: vec![entry("claude")],
        };

        // When
        let written = encode(&plain);

        // Then
        assert!(!written.contains("null"), "{written}");
        assert!(!written.contains("label"), "{written}");
    }

    #[test]
    fn given_a_file_one_hand_edited_entry_broke_when_it_is_read_then_the_entries_beside_it_survive()
    {
        // Given — la spec §9 autorise à éditer ce fichier à la main. Une entrée à qui il
        // manque son `command` ferait échouer la lecture du tableau entier si les entrées
        // étaient relues d'un bloc : l'utilisateur perdrait des déclarations qu'il n'a pas
        // touchées
        let hand_edited = r#"{"tools":[
            {"adapter":"claude-code","config":"~/.claude"},
            {"command":"claude-perso","adapter":"claude-code"},
            "n'importe quoi"
        ]}"#;

        // When
        let read = decode(hand_edited);

        // Then
        assert_eq!(
            read.tools
                .iter()
                .map(|tool| tool.command.as_str())
                .collect::<Vec<_>>(),
            vec!["claude-perso"]
        );
    }

    #[test]
    fn given_a_tools_file_that_says_nothing_understandable_when_it_is_read_then_ash_opens_without_declarations(
    ) {
        // Given — un fichier tronqué par une coupure, vidé, ou remplacé par autre chose
        let broken = ["", "{", "null", "tools", "{\"tools\":3}"];

        // When
        let read: Vec<usize> = broken
            .iter()
            .map(|content| decode(content).tools.len())
            .collect();

        // Then — un fichier de réglages n'empêche jamais une fenêtre d'ouvrir
        assert_eq!(read, vec![0; broken.len()]);
    }

    #[test]
    fn given_a_file_written_by_a_later_ash_when_it_is_read_then_what_this_version_knows_survives() {
        // Given — revenir d'une version à la précédente n'a rien d'hypothétique : il suffit
        // de rebasculer de branche. Un champ inconnu ne doit pas coûter la déclaration
        let later = r#"{"tools":[{"command":"claude","adapter":"claude-code","priority":3}],"sorted_by":"name"}"#;

        // When
        let read = decode(later);

        // Then
        assert_eq!(read.tools.len(), 1);
        assert_eq!(read.tools[0].adapter, "claude-code");
    }

    #[test]
    fn given_a_declaration_saved_to_disk_when_a_new_session_loads_it_then_it_survived_the_restart()
    {
        // Given — le fichier est le seul lien entre deux sessions
        let path = std::env::temp_dir()
            .join(format!("ash-tools-{}", std::process::id()))
            .join("tools.json");
        let store = FileToolStore::at(path.clone());
        let kept = PersistedTools {
            tools: vec![entry("claude-perso")],
        };

        // When
        store
            .save(&kept)
            .expect("le dossier temporaire est écrivable");
        let next_session = FileToolStore::at(path.clone()).load();

        // Then
        assert_eq!(next_session, kept);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn given_no_tools_file_at_all_when_it_is_loaded_then_nothing_is_invented() {
        // Given — la première ouverture d'Ash sur une machine
        let store = FileToolStore::at(std::env::temp_dir().join("ash-tools-absent/tools.json"));

        // When / Then
        assert_eq!(store.load(), PersistedTools::default());
    }
}
