use std::path::{Path, PathBuf};

use super::ports::{Entry, FileSystem};

/// Le vrai système de fichiers.
///
/// Choisi et injecté depuis la composition root ; le reste de la feature ne connaît que
/// le trait.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemFileSystem;

impl FileSystem for SystemFileSystem {
    fn entry(&self, path: &Path) -> Option<Entry> {
        // `metadata` suit les liens : un `.git` qui est un lien vers un dossier doit
        // compter comme un dossier, pas comme un fichier `gitdir:`.
        let metadata = std::fs::metadata(path).ok()?;
        Some(if metadata.is_dir() {
            Entry::Directory
        } else {
            Entry::File
        })
    }

    fn read_to_string(&self, path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|why| why.to_string())
    }

    fn has_entries(&self, path: &Path) -> bool {
        std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_some())
    }

    fn canonicalize(&self, path: &Path) -> Option<PathBuf> {
        std::fs::canonicalize(path).ok()
    }
}
