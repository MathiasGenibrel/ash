use std::fmt;
use std::path::PathBuf;

/// Erreurs de la feature `git`.
///
/// Un type par feature, comme pour [`crate::features::pty`]. Les variantes disent ce qui
/// est cassé **dans le dépôt de l'utilisateur**, pas où le code a échoué : c'est la seule
/// information dont l'appelant puisse faire quelque chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    /// Le chemin de départ n'existe pas, ou n'est pas traversable.
    UnreadablePath(PathBuf),
    /// Un fichier de contrôle attendait un chemin et n'en porte pas.
    ///
    /// Concerne le `.git` d'un worktree lié (`gitdir: …` absent ou vide) et le
    /// `commondir` d'un dossier git de worktree.
    Malformed(PathBuf),
    /// Un fichier de contrôle désigne un dossier qui n'existe pas.
    ///
    /// Le cas courant est un worktree dont le dépôt a été déplacé ou supprimé : le
    /// dossier est encore là, le `gitdir:` ne mène plus nulle part.
    Dangling { at: PathBuf, target: PathBuf },
    /// Lecture refusée par le système.
    Io { path: PathBuf, why: String },
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::UnreadablePath(path) => {
                write!(f, "chemin illisible : {}", path.display())
            }
            GitError::Malformed(path) => {
                write!(f, "fichier de contrôle git illisible : {}", path.display())
            }
            GitError::Dangling { at, target } => write!(
                f,
                "{} désigne {}, qui n'existe pas",
                at.display(),
                target.display()
            ),
            GitError::Io { path, why } => {
                write!(f, "lecture de {} impossible : {why}", path.display())
            }
        }
    }
}

impl std::error::Error for GitError {}
