use std::path::PathBuf;

use super::error::SidebarError;
use super::persisted::Persisted;

/// Où l'état de la colonne se garde d'une session à l'autre.
///
/// Un trait, comme tous les effets système de ce dépôt : sans lui, vérifier qu'une épingle
/// survit au redémarrage demanderait d'écrire dans le `$HOME` de qui lance les tests.
pub trait SidebarStore: Send + Sync {
    /// Ce qui est gardé, ou `None` — première ouverture, fichier absent, fichier abîmé.
    fn load(&self) -> Option<Persisted>;
    fn save(&self, state: &Persisted) -> Result<(), SidebarError>;
}

/// L'état dans `~/.ash/state.json` (spec §9.2).
///
/// `~/.ash` existe déjà — le socket d'events et `theme.json` y vivent. Le fichier est lisible
/// à l'œil nu et se supprime à la main : c'est ce que promet la spec §10, « suppression du
/// dossier ».
pub struct FileSidebarStore {
    path: PathBuf,
}

impl FileSidebarStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// `~/.ash/state.json`.
    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::at(home.join(".ash").join("state.json"))
    }
}

impl SidebarStore for FileSidebarStore {
    /// **Tolérante à tout.** Un fichier absent, tronqué, vide ou rempli d'autre chose rend
    /// `None`, et Ash repart sur une colonne sans épingle. Un fichier d'état n'est jamais une
    /// raison d'empêcher une fenêtre d'ouvrir.
    fn load(&self) -> Option<Persisted> {
        decode(&std::fs::read_to_string(&self.path).ok()?)
    }

    fn save(&self, state: &Persisted) -> Result<(), SidebarError> {
        let io = |why: std::io::Error| SidebarError::Io {
            path: self.path.clone(),
            why: why.to_string(),
        };

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&self.path, encode(state)).map_err(io)
    }
}

/// Le contenu du fichier, ou `None` s'il ne dit rien qu'on comprenne.
fn decode(content: &str) -> Option<Persisted> {
    serde_json::from_str::<Persisted>(content).ok()
}

fn encode(state: &Persisted) -> String {
    // `to_string_pretty` ici, et non le `to_string` de `theme.json` : ce fichier-ci grandit
    // avec le nombre de projets épinglés, et la spec §10 promet qu'Ash se retire de la
    // machine sans mystère — un fichier qu'on relit à l'œil nu tient cette promesse.
    format!(
        "{}\n",
        serde_json::to_string_pretty(state).unwrap_or_else(|_| String::from("{}"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(pinned: &[&str], collapsed: &[&str]) -> Persisted {
        Persisted {
            pinned: pinned.iter().map(|root| (*root).to_owned()).collect(),
            collapsed: collapsed.iter().map(|key| (*key).to_owned()).collect(),
        }
    }

    #[test]
    fn given_a_pinned_and_collapsed_state_when_it_is_written_then_the_file_holds_nothing_else() {
        // Given — le troisième critère de la tâche, et la règle de la spec §3.1 : *Ash
        // persiste ce que les agents ont fait, jamais ce qu'ils étaient en train de faire*.
        // Ce test est une garantie de **non-écriture** : il tombe le jour où quelqu'un ajoute
        // au fichier la liste des onglets, le worktree courant, ou un état d'agent.
        //
        // Il a déjà servi une fois : la largeur de la colonne et son repli (#129) survivent
        // eux aussi au redémarrage, et leur place évidente semblait être ici. Ils sont partis
        // dans `~/.ash/theme.json` avec le thème et la taille de police — ce sont des
        // préférences d'**apparence** (spec §9) —, et ce fichier-ci n'a pas grossi.
        let kept = state(&["/wt/ash-sidebar"], &["repo:/dev/ash/.git"]);

        // When
        let written = encode(&kept);

        // Then — deux clés, et pas une de plus
        let parsed: serde_json::Value =
            serde_json::from_str(&written).expect("le fichier écrit est du JSON");
        let object = parsed.as_object().expect("le fichier écrit est un objet");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["collapsed", "pinned"]);
    }

    #[test]
    fn given_a_stored_state_when_it_is_read_back_then_it_is_the_same_state() {
        // Given — le fichier est le seul lien entre deux sessions ; sa forme est un contrat
        // avec la version d'Ash de demain
        let kept = state(&["/wt/ash-sidebar", "/dev/ash"], &["/dev/ash"]);

        // When
        let read = decode(&encode(&kept));

        // Then
        assert_eq!(read, Some(kept));
    }

    #[test]
    fn given_a_state_file_written_by_a_later_ash_when_it_is_read_then_the_two_known_facts_survive()
    {
        // Given — un fichier portant un troisième fait, ou lu par une version d'Ash qui ne le
        // connaît pas encore. Revenir d'une version à la précédente n'a rien d'hypothétique :
        // il suffit de rebasculer de branche.
        let later = "{\"pinned\":[\"/wt/a\"],\"collapsed\":[],\"sorting\":\"by-name\"}";

        // When
        let read = decode(later);

        // Then — un champ qu'on ne connaît pas se laisse tomber ; il ne fait pas perdre les
        // épingles. C'est ce que `deny_unknown_fields` détruirait.
        assert_eq!(read, Some(state(&["/wt/a"], &[])));
    }

    #[test]
    fn given_a_state_file_that_says_nothing_understandable_when_it_is_read_then_ash_opens_without_pins(
    ) {
        // Given — un fichier tronqué par une coupure, vidé, ou édité à la main
        let broken = ["", "{", "null", "state", "{\"pinned\":3}"];

        // When
        let read: Vec<Option<Persisted>> = broken.iter().map(|content| decode(content)).collect();

        // Then — un fichier d'état n'empêche jamais une fenêtre d'ouvrir
        assert_eq!(read, vec![None; broken.len()]);
    }

    #[test]
    fn given_a_state_saved_to_disk_when_a_new_session_loads_it_then_the_pins_survived_the_restart()
    {
        // Given
        let path = std::env::temp_dir()
            .join(format!("ash-sidebar-rows-{}", std::process::id()))
            .join("state.json");
        let store = FileSidebarStore::at(path.clone());
        let kept = state(&["/wt/ash-sidebar"], &["repo:/dev/ash/.git"]);

        // When
        store.save(&kept).unwrap();
        let next_session = FileSidebarStore::at(path.clone()).load();

        // Then
        assert_eq!(next_session, Some(kept));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn given_no_state_file_at_all_when_it_is_loaded_then_nothing_is_invented() {
        // Given — la première ouverture d'Ash sur une machine
        let store =
            FileSidebarStore::at(std::env::temp_dir().join("ash-sidebar-rows-absent/state.json"));

        // When / Then
        assert_eq!(store.load(), None);
    }
}
