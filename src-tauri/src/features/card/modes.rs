//! Ce que l'utilisateur a choisi pour chaque worktree, et où ce choix est gardé.
//!
//! `~/.ash/cards.json` — à côté de `theme.json`, `state.json`, `shortcuts.json` et
//! `notifications.json`, et pour la même raison : c'est une préférence, elle survit au
//! redémarrage, et elle est détenue par le backend
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). **Le fichier ne
//! contient que les écarts** — les worktrees dont l'emplacement a été choisi à la main —,
//! comme `shortcuts.json` ne contient que les raccourcis qui s'écartent du défaut.
//!
//! Il est écrit avec `std::fs` derrière un trait, comme `FileJournalStore` : c'est un
//! fichier **d'Ash**, dans le dossier d'Ash. Le port [`CardFiles`](super::CardFiles), lui,
//! est réservé aux fichiers de l'utilisateur, et son `write` n'accepte pour cette raison
//! qu'un [`CardDocument`](super::document::CardDocument) — un JSON de préférences n'en est
//! pas un, et ce n'est pas un accident.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::place::CardMode;

/// Le magasin des choix explicites.
pub trait ModeStore: Send + Sync {
    /// Ce qui a été choisi pour ce worktree, `None` si rien ne l'a été.
    fn chosen(&self, worktree_root: &Path) -> Option<CardMode>;
    /// Retient un choix. `None` revient au défaut — donc à la détection.
    fn choose(&self, worktree_root: &Path, mode: Option<CardMode>);
}

/// La forme sur le disque : un objet, une clé par worktree, `"repo"` ou `"local"`.
///
/// Lisible et modifiable à la main (spec §10). Un fichier illisible est traité comme un
/// fichier vide : personne n'a rien choisi, la détection reprend la main — ce qui est
/// toujours moins grave que de refuser d'afficher une fiche.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Choices {
    #[serde(default)]
    worktrees: BTreeMap<String, String>,
}

pub struct FileModeStore {
    path: PathBuf,
    /// Ce que le fichier dit, tenu en mémoire : la fiche est relue à chaque affichage du
    /// panneau, et un `open` par lecture ne se justifie pas pour trois lignes de JSON.
    choices: Mutex<Choices>,
}

impl FileModeStore {
    pub fn at(path: PathBuf) -> Self {
        let choices = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            choices: Mutex::new(choices),
        }
    }

    /// `~/.ash/cards.json`.
    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::at(home.join(".ash").join("cards.json"))
    }

    /// Écrit le fichier. **Un échec n'échoue nulle part** : le choix vaut pour la session,
    /// et un `~/.ash` en lecture seule ne doit coûter ni la fiche, ni le panneau.
    fn save(&self, choices: &Choices) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(choices) {
            let _ = std::fs::write(&self.path, format!("{text}\n"));
        }
    }
}

impl ModeStore for FileModeStore {
    fn chosen(&self, worktree_root: &Path) -> Option<CardMode> {
        let choices = self.choices.lock().ok()?;
        match choices
            .worktrees
            .get(&worktree_root.to_string_lossy().into_owned())?
            .as_str()
        {
            "local" => Some(CardMode::Local),
            "repo" => Some(CardMode::Repo),
            _ => None,
        }
    }

    fn choose(&self, worktree_root: &Path, mode: Option<CardMode>) {
        let Ok(mut choices) = self.choices.lock() else {
            return;
        };
        let key = worktree_root.to_string_lossy().into_owned();
        match mode {
            Some(CardMode::Local) => {
                choices.worktrees.insert(key, "local".to_owned());
            }
            Some(CardMode::Repo) => {
                choices.worktrees.insert(key, "repo".to_owned());
            }
            None => {
                choices.worktrees.remove(&key);
            }
        }
        self.save(&choices);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_choices_file_edited_by_hand_when_it_is_read_back_then_it_says_what_it_says() {
        // Given — le fichier est fait pour être ouvert (spec §10). Le relire est ce qui rend
        // le mode local persistant d'une session à l'autre, et c'est le seul comportement de
        // ce module qui puisse casser sans qu'on le voie.
        let dir = std::env::temp_dir().join(format!("ash-cards-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("cards.json");
        let _ = std::fs::write(
            &path,
            "{\"worktrees\":{\"/dev/ash\":\"local\",\"/dev/autre\":\"n'importe quoi\"}}",
        );

        // When
        let store = FileModeStore::at(path.clone());

        // Then — ce qui se comprend est rendu, ce qui ne se comprend pas retombe sur la
        // détection plutôt que de faire échouer la lecture de la fiche.
        assert_eq!(store.chosen(Path::new("/dev/ash")), Some(CardMode::Local));
        assert_eq!(store.chosen(Path::new("/dev/autre")), None);
        assert_eq!(store.chosen(Path::new("/dev/jamais-vu")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
