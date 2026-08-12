use std::path::{Path, PathBuf};

use super::block::Document;
use super::ports::ConfigFiles;

/// Le vrai disque.
///
/// Rien de métier ici : toute la prudence de la feature — sauvegarde, marqueurs, refus —
/// vit au-dessus, dans [`super::install`]. Ce fichier n'apporte qu'une chose, mais elle ne
/// se délègue pas : l'**atomicité** de l'écriture.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemConfigFiles;

impl ConfigFiles for SystemConfigFiles {
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

    /// Écrit à côté, puis renomme.
    ///
    /// `rename` sur le même système de fichiers est atomique : le `settings.json` de
    /// l'utilisateur est soit l'ancien, soit le nouveau, jamais un demi-fichier. Une
    /// écriture directe laisserait, sur une coupure au mauvais moment, une configuration
    /// tronquée que Claude Code refuserait de lire — et l'utilisateur n'aurait aucune
    /// raison de soupçonner Ash.
    ///
    /// Le fichier temporaire est **dans le même dossier** que la cible, sans quoi le
    /// renommage traverserait deux systèmes de fichiers et ne serait plus atomique.
    fn write(&self, path: &Path, content: &Document) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or_else(|| "chemin sans dossier parent".to_owned())?;
        std::fs::create_dir_all(parent).map_err(|why| why.to_string())?;

        let temporary = temporary_beside(path);
        std::fs::write(&temporary, content.as_str()).map_err(|why| why.to_string())?;
        std::fs::rename(&temporary, path).map_err(|why| {
            // Le temporaire n'a rien à faire à côté de la configuration de l'utilisateur si
            // le renommage a échoué.
            let _ = std::fs::remove_file(&temporary);
            why.to_string()
        })
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), String> {
        std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|why| why.to_string())
    }

    fn remove(&self, path: &Path) -> Result<(), String> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(why) => Err(why.to_string()),
        }
    }
}

/// Un nom de fichier temporaire voisin de la cible, propre à ce processus.
///
/// Le pid suffit : deux Ash lancés en même temps sont déjà exclus par le socket
/// ([`crate::features::agents::AgentError::AlreadyListening`]), et deux installations d'un
/// même Ash sont séquentielles.
fn temporary_beside(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "settings.json".to_owned());
    path.with_file_name(format!(".{name}.ash-{}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_settings_file_when_ash_rewrites_it_then_it_is_never_seen_half_written() {
        // Given — l'atomicité est une promesse du port, pas un confort : un `settings.json`
        // tronqué est une session de l'utilisateur qui ne démarre plus. On la vérifie par
        // ce qu'elle implique — le contenu final est complet, et rien ne traîne à côté.
        let directory = std::env::temp_dir().join(format!("ash-hooks-{}", std::process::id()));
        let file = directory.join("settings.json");
        let files = SystemConfigFiles;

        // When
        files
            .write(&file, &Document::verbatim("{\n  \"a\": 1\n}\n"))
            .unwrap();
        files
            .write(&file, &Document::verbatim("{\n  \"a\": 2\n}\n"))
            .unwrap();

        // Then
        assert_eq!(
            files.read(&file).unwrap(),
            Some("{\n  \"a\": 2\n}\n".to_owned())
        );
        let leftovers: Vec<_> = std::fs::read_dir(&directory)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "settings.json")
            .collect();
        assert_eq!(leftovers, Vec::<String>::new());

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn given_a_config_dir_without_a_settings_file_when_it_is_read_then_the_absence_is_not_an_error()
    {
        // Given — un dossier de configuration tout neuf, cas nominal d'une première
        // installation. Le distinguer d'une panne de lecture est ce qui laisse Ash créer le
        // fichier au lieu de refuser.
        let files = SystemConfigFiles;

        // When
        let read = files.read(&std::env::temp_dir().join("ash-hooks-absent/settings.json"));

        // Then
        assert_eq!(read, Ok(None));
    }
}
