use std::path::Path;

use super::document::CardDocument;
use super::ports::CardFiles;

/// Le vrai disque, choisi et injecté depuis la composition root.
///
/// Deux exigences du port se tiennent ici, et nulle part ailleurs : les dossiers manquants
/// sont créés — `.ash/` n'existe pas avant la première fiche — et l'écriture passe par un
/// fichier temporaire suivi d'un renommage, pour qu'une coupure ne laisse jamais la fiche de
/// l'utilisateur à moitié écrite.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCardFiles;

impl CardFiles for SystemCardFiles {
    fn read(&self, path: &Path) -> Result<Option<String>, String> {
        match std::fs::read_to_string(path) {
            Ok(content) => Ok(Some(content)),
            Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(why) => Err(why.to_string()),
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn write(&self, path: &Path, content: &CardDocument) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|why| why.to_string())?;
        }
        // Le temporaire est **à côté** du fichier, donc sur le même système : un renommage
        // n'est atomique qu'entre deux chemins du même volume.
        let temporary = path.with_extension("md.ash-tmp");
        std::fs::write(&temporary, content.as_str()).map_err(|why| why.to_string())?;
        std::fs::rename(&temporary, path).map_err(|why| {
            let _ = std::fs::remove_file(&temporary);
            why.to_string()
        })
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), String> {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|why| why.to_string())?;
        }
        std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|why| why.to_string())
    }
}
