use std::collections::BTreeMap;
use std::path::PathBuf;

use super::combination::Combination;
use super::error::ShortcutError;

/// Ce que le fichier garde d'une session à l'autre : **les écarts, pas la liste**.
///
/// Une table complète aurait figé les défauts du jour où elle a été écrite : le jour où un
/// défaut d'Ash change, chaque installation garderait l'ancien sans que personne ne l'ait
/// choisi. Ici, une action absente du fichier suit son défaut, et une action présente à
/// `null` n'a **aucun** raccourci — c'est ce que `⌫` pose, et il faut pouvoir le distinguer
/// de « pas d'entrée ».
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredBindings {
    /// Clé : l'identifiant d'entrée de menu (`tab:new`, `view:toggle-sidebar`).
    #[serde(default)]
    pub bindings: BTreeMap<String, Option<Combination>>,
}

/// Où les liaisons se gardent d'une session à l'autre.
///
/// Un trait, comme tous les effets système de ce dépôt : sans lui, vérifier qu'un raccourci
/// survit au redémarrage demanderait d'écrire dans le `$HOME` de qui lance les tests.
pub trait BindingStore: Send + Sync {
    /// Ce qui est gardé, ou `None` — première ouverture, fichier absent, fichier abîmé.
    fn load(&self) -> Option<StoredBindings>;
    fn save(&self, bindings: &StoredBindings) -> Result<(), ShortcutError>;
}

/// Les liaisons dans `~/.ash/shortcuts.json`.
///
/// À côté de `theme.json` et de `state.json`, et pour les mêmes raisons : `~/.ash` existe
/// déjà, un JSON de quelques lignes y est moins cher qu'une dépendance de préférences, et il
/// se relit à l'œil nu — ce qui compte pour un fichier qu'on éditera à la main le jour où
/// une capture aura posé une combinaison qu'on n'arrive plus à reproduire.
///
/// Un fichier **par sorte de préférence**, et non un `settings.json` d'Ash : chacun a son
/// détenteur, et un fichier unique les ferait s'écrire par-dessus au premier changement
/// simultané.
pub struct FileBindingStore {
    path: PathBuf,
}

impl FileBindingStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// `~/.ash/shortcuts.json`.
    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::at(home.join(".ash").join("shortcuts.json"))
    }
}

impl BindingStore for FileBindingStore {
    /// **Tolérante à tout.** Un fichier absent, tronqué, vide ou rempli d'autre chose rend
    /// `None`, et Ash repart des défauts. Une préférence de raccourci n'est jamais une raison
    /// d'empêcher une fenêtre d'ouvrir — et sans les défauts, il n'y aurait plus de menu.
    fn load(&self) -> Option<StoredBindings> {
        decode(&std::fs::read_to_string(&self.path).ok()?)
    }

    fn save(&self, bindings: &StoredBindings) -> Result<(), ShortcutError> {
        let io = |why: std::io::Error| ShortcutError::Io {
            path: self.path.clone(),
            why: why.to_string(),
        };

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&self.path, encode(bindings)).map_err(io)
    }
}

/// Le contenu du fichier, ou `None` s'il ne dit rien qu'on comprenne.
fn decode(content: &str) -> Option<StoredBindings> {
    serde_json::from_str::<StoredBindings>(content).ok()
}

fn encode(bindings: &StoredBindings) -> String {
    // `to_string_pretty`, contrairement à `theme.json` : ce fichier-ci a une ligne par
    // raccourci changé, et c'est celui qu'on ouvrira pour défaire une combinaison qu'on
    // n'arrive plus à reproduire au clavier.
    format!(
        "{}\n",
        serde_json::to_string_pretty(bindings).unwrap_or_else(|_| String::from("{}"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(action: &str, accelerator: Option<&str>) -> StoredBindings {
        let mut bindings = BTreeMap::new();
        bindings.insert(
            action.to_owned(),
            accelerator.map(|written| Combination::parse(written).unwrap()),
        );
        StoredBindings { bindings }
    }

    #[test]
    fn given_a_stored_binding_when_it_is_read_back_then_it_is_the_same_binding() {
        // Given — le fichier est le seul lien entre deux sessions ; sa forme est un contrat
        // avec la version d'Ash de demain
        let written = encode(&one("tab:new", Some("Cmd+Shift+KeyJ")));

        // When
        let read = decode(&written);

        // Then
        assert_eq!(read, Some(one("tab:new", Some("Cmd+Shift+KeyJ"))));
    }

    #[test]
    fn given_a_line_left_without_any_shortcut_when_the_file_is_read_back_then_it_stays_without_one()
    {
        // Given — `⌫` pose « aucun raccourci », qui n'est pas « pas de choix » : les
        // confondre ferait revenir le défaut au redémarrage
        let written = encode(&one("tab:clear", None));

        // When
        let read = decode(&written);

        // Then
        assert!(written.contains("null"));
        assert_eq!(read, Some(one("tab:clear", None)));
    }

    #[test]
    fn given_a_file_written_by_a_later_ash_when_it_is_read_then_the_bindings_it_understands_survive(
    ) {
        // Given — un fichier portant un champ de plus, ou lu par une version antérieure.
        // Revenir d'une version à la précédente n'a rien d'hypothétique : il suffit de
        // rebasculer de branche
        let later = "{\"bindings\":{\"tab:new\":\"Cmd+KeyJ\"},\"chords\":[]}";

        // When
        let read = decode(later);

        // Then — un champ qu'on ne connaît pas se laisse tomber ; c'est ce que
        // `deny_unknown_fields` détruirait, et c'est pour ça qu'il n'est nulle part ici
        assert_eq!(read, Some(one("tab:new", Some("Cmd+KeyJ"))));
    }

    #[test]
    fn given_a_shortcuts_file_that_says_nothing_understandable_when_it_is_read_then_ash_falls_back_to_its_defaults(
    ) {
        // Given — un fichier tronqué par une coupure, vidé, ou édité à la main de travers.
        // La dernière entrée est le piège : un accélérateur que `muda` ne saurait pas jouer
        let broken = [
            "",
            "{",
            "null",
            "{\"bindings\":42}",
            "{\"bindings\":{\"tab:new\":\"Hyper+KeyJ\"}}",
        ];

        // When
        let read: Vec<Option<StoredBindings>> = broken.iter().map(|c| decode(c)).collect();

        // Then — sans les défauts, il n'y aurait plus de menu du tout
        assert_eq!(read, vec![None; broken.len()]);
    }

    #[test]
    fn given_a_binding_saved_to_disk_when_a_new_session_loads_it_then_it_survived_the_restart() {
        // Given
        let path = std::env::temp_dir()
            .join(format!("ash-shortcuts-{}", std::process::id()))
            .join("shortcuts.json");
        let store = FileBindingStore::at(path.clone());
        let chosen = one("tab:new", Some("Ctrl+Cmd+KeyJ"));

        // When
        store.save(&chosen).unwrap();
        let next_session = FileBindingStore::at(path.clone()).load();

        // Then
        assert_eq!(next_session, Some(chosen));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn given_no_shortcuts_file_at_all_when_it_is_loaded_then_nothing_is_invented() {
        // Given — la première ouverture d'Ash sur une machine
        let store = FileBindingStore::at(std::env::temp_dir().join("ash-shortcuts-absent/x.json"));

        // When / Then
        assert_eq!(store.load(), None);
    }
}
