//! La lecture des fichiers de contrôle de git.
//!
//! `.git` d'un worktree lié, `commondir`, `HEAD`, `rebase-merge/onto`, `msgnum` : tous
//! portent la même forme — **une** ligne, parfois un préfixe, quelques dizaines d'octets.
//! La règle de lecture est écrite ici une fois pour toutes plutôt qu'une fois par fichier,
//! pour que le prochain soit lu à l'identique et échoue de la même manière.

use std::path::Path;

use super::error::GitError;
use super::ports::FileSystem;

/// La ligne utile d'un fichier de contrôle **attendu**.
///
/// Absent, illisible ou vide est une erreur : se taire ferait passer un dépôt cassé pour
/// un dépôt ordinaire.
pub fn control_line(fs: &dyn FileSystem, path: &Path) -> Result<String, GitError> {
    let content = fs.read_to_string(path).map_err(|why| GitError::Io {
        path: path.to_owned(),
        why,
    })?;
    let line = first_line(&content);
    if line.is_empty() {
        return Err(GitError::Malformed(path.to_owned()));
    }
    Ok(line)
}

/// La ligne utile d'un fichier de contrôle **facultatif**.
///
/// `None` pour un fichier absent comme pour un fichier vide : git en écrit de vides
/// (`rebase-merge/interactive` est un drapeau sans contenu), et un état affiché à partir
/// d'une chaîne vide serait un mensonge silencieux.
pub fn optional_line(fs: &dyn FileSystem, path: &Path) -> Option<String> {
    let content = fs.read_to_string(path).ok()?;
    let line = first_line(&content);
    (!line.is_empty()).then_some(line)
}

fn first_line(content: &str) -> String {
    content.lines().next().unwrap_or_default().trim().to_owned()
}
