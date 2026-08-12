//! Le double du port [`ConfigFiles`] : un disque en mémoire qui **retient l'ordre des
//! gestes**.
//!
//! Retenir le contenu final ne suffirait pas. La règle qui compte ici est temporelle — « la
//! sauvegarde vient avant l'écriture » — et un double qui ne garde que l'état final la
//! laisserait passer : un `.bak` écrit après coup a exactement la même trace qu'un `.bak`
//! écrit avant.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::document::Document;
use super::ports::ConfigFiles;

pub struct FakeConfigFiles {
    disk: Mutex<BTreeMap<PathBuf, String>>,
    journal: Mutex<Vec<String>>,
}

impl FakeConfigFiles {
    pub fn new() -> Self {
        Self {
            disk: Mutex::new(BTreeMap::new()),
            journal: Mutex::new(Vec::new()),
        }
    }

    /// Un fichier déjà présent — celui de l'utilisateur, avant qu'Ash n'existe.
    #[must_use]
    pub fn carrying(self, path: &str, content: &str) -> Self {
        self.replace(Path::new(path), content);
        self
    }

    /// Une écriture qui ne vient pas d'Ash : l'utilisateur qui édite son fichier.
    ///
    /// Elle ne passe pas par le journal, sans quoi les tests ne pourraient plus distinguer
    /// ce qu'Ash a fait de ce qu'on lui a préparé.
    pub fn replace(&self, path: &Path, content: &str) {
        if let Ok(mut disk) = self.disk.lock() {
            disk.insert(path.to_owned(), content.to_owned());
        }
    }

    pub fn content_of(&self, path: &Path) -> Option<String> {
        self.disk.lock().ok()?.get(path).cloned()
    }

    /// Les gestes d'Ash, dans l'ordre.
    pub fn journal(&self) -> Vec<String> {
        self.journal
            .lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }

    pub fn forget_the_journal(&self) {
        if let Ok(mut journal) = self.journal.lock() {
            journal.clear();
        }
    }

    fn note(&self, entry: String) {
        if let Ok(mut journal) = self.journal.lock() {
            journal.push(entry);
        }
    }
}

impl ConfigFiles for FakeConfigFiles {
    fn read(&self, path: &Path) -> Result<Option<String>, String> {
        self.note(format!("read {}", path.display()));
        Ok(self.content_of(path))
    }

    fn exists(&self, path: &Path) -> bool {
        self.content_of(path).is_some()
    }

    fn write(&self, path: &Path, content: &Document) -> Result<(), String> {
        self.note(format!("write {}", path.display()));
        self.replace(path, content.as_str());
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), String> {
        self.note(format!("copy {} -> {}", from.display(), to.display()));
        let content = self
            .content_of(from)
            .ok_or_else(|| format!("{} n'existe pas", from.display()))?;
        self.replace(to, &content);
        Ok(())
    }

    fn remove(&self, path: &Path) -> Result<(), String> {
        self.note(format!("remove {}", path.display()));
        if let Ok(mut disk) = self.disk.lock() {
            disk.remove(path);
        }
        Ok(())
    }
}
