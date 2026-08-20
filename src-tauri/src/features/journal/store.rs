//! Où le journal vit, derrière un trait que la feature possède.
//!
//! Sans lui, vérifier qu'un rebase ne perd pas une attribution demanderait d'écrire dans le
//! `$HOME` de qui lance les tests — et de le purger, ce qui est justement le geste qu'on
//! veut prouver.

use std::path::{Path, PathBuf};

use super::error::JournalError;

/// Le dossier du journal, fichier par fichier.
///
/// Les noms de fichiers sont ceux qu'[`super::entry::file_name`] fabrique : le magasin ne
/// décide pas comment un dépôt se nomme, il range et il rend.
pub trait JournalStore: Send + Sync {
    /// Ajoute une ligne **à la fin** d'un fichier, qu'il existe ou non.
    ///
    /// Append-only : c'est la seule écriture du journal, et il n'y en aura pas d'autre.
    fn append(&self, file: &str, line: &str) -> Result<(), JournalError>;

    /// Le contenu d'un fichier, ou une chaîne vide s'il n'existe pas.
    ///
    /// Un dépôt dont Ash n'a jamais rien vu n'est pas une erreur : c'est le cas de tous les
    /// dépôts, avant le premier commit observé.
    fn read(&self, file: &str) -> String;

    /// Les fichiers présents. Vide si le dossier n'existe pas encore.
    fn files(&self) -> Vec<String>;

    /// Efface **tout** le journal (spec §10). Ce que le dossier ne contient pas, elle ne
    /// touche pas.
    fn purge(&self) -> Result<(), JournalError>;
}

/// Le journal dans `~/.ash/journal/` (spec §9.2).
///
/// Un fichier par dépôt, du texte, lisible à l'œil nu et supprimable à la main — c'est ce
/// qu'ADR-0014 retient contre une base de données, pour un fichier qui contient des prompts.
pub struct FileJournalStore {
    dir: PathBuf,
}

impl FileJournalStore {
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// `~/.ash/journal/`.
    pub fn in_home() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::at(home.join(".ash").join("journal"))
    }

    fn path(&self, file: &str) -> PathBuf {
        self.dir.join(file)
    }

    fn io(path: &Path) -> impl Fn(std::io::Error) -> JournalError + '_ {
        move |why| JournalError::Io {
            path: path.to_owned(),
            why: why.to_string(),
        }
    }
}

impl JournalStore for FileJournalStore {
    fn append(&self, file: &str, line: &str) -> Result<(), JournalError> {
        use std::io::Write;

        let path = self.path(file);
        let io = Self::io(&path);
        std::fs::create_dir_all(&self.dir).map_err(Self::io(&self.dir))?;
        // Ouvert en ajout à chaque ligne plutôt que gardé ouvert : un descripteur retenu
        // pour toute la session empêcherait l'utilisateur de supprimer le dossier sous les
        // pieds d'Ash — ce que la spec §10 lui promet explicitement.
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(&io)?;
        file.write_all(line.as_bytes()).map_err(&io)
    }

    fn read(&self, file: &str) -> String {
        std::fs::read_to_string(self.path(file)).unwrap_or_default()
    }

    fn files(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().to_str()?.to_owned();
                name.ends_with(".jsonl").then_some(name)
            })
            .collect()
    }

    fn purge(&self) -> Result<(), JournalError> {
        // Les fichiers du journal, un par un — et **pas** le dossier : `~/.ash/journal/` ne
        // contient rien d'autre aujourd'hui, mais supprimer un dossier entier parce qu'on
        // sait ce qu'on y a mis est la façon dont on efface un jour ce qu'un autre y a posé.
        for file in self.files() {
            let path = self.path(&file);
            std::fs::remove_file(&path).map_err(Self::io(&path))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un dossier de journal jetable, supprimé à la fin du test, réussi ou non.
    struct Sandbox {
        dir: PathBuf,
    }

    impl Sandbox {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("ash-journal-{label}-{}", std::process::id()))
                .join("journal");
            let _ = std::fs::remove_dir_all(&dir);
            Self { dir }
        }

        fn store(&self) -> FileJournalStore {
            FileJournalStore::at(self.dir.clone())
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn given_a_journal_with_entries_when_more_arrive_then_nothing_already_written_is_touched() {
        // Given — append-only : une ligne écrite est un fait observé, et un fait observé ne
        // se réécrit pas. Ce test tombe le jour où quelqu'un remplace l'ajout par une
        // réécriture du fichier entier, ce qui rendrait une interruption destructrice.
        let sandbox = Sandbox::new("append");
        let store = sandbox.store();
        store.append("ash.jsonl", "un\n").expect("dossier neuf");

        // When
        store
            .append("ash.jsonl", "deux\n")
            .expect("dossier existant");

        // Then
        assert_eq!(store.read("ash.jsonl"), "un\ndeux\n");
    }

    #[test]
    fn given_a_journal_of_several_repositories_when_it_is_purged_then_no_prompt_is_left_behind() {
        // Given — le geste de la spec §10, sur ce que le journal a de plus sensible.
        let sandbox = Sandbox::new("purge");
        let store = sandbox.store();
        store.append("ash.jsonl", "un\n").expect("dossier neuf");
        store.append("autre.jsonl", "deux\n").expect("dossier neuf");

        // When
        store.purge().expect("le dossier appartient à Ash");

        // Then
        assert!(store.files().is_empty());
        assert_eq!(store.read("ash.jsonl"), "");
    }

    #[test]
    fn given_no_journal_at_all_when_it_is_read_or_purged_then_both_answer_without_failing() {
        // Given — l'état de tous les Ash au premier lancement : le dossier n'existe pas.
        let sandbox = Sandbox::new("absent");
        let store = sandbox.store();

        // When / Then — lire un dépôt jamais vu est un cas nominal, pas une panne
        assert_eq!(store.read("ash.jsonl"), "");
        assert!(store.files().is_empty());
        assert!(store.purge().is_ok());
    }
}
