use std::fmt;
use std::path::PathBuf;

/// Erreurs de la feature `workspaces`.
///
/// Un type par feature, comme partout ailleurs. Il n'y a qu'une variante parce qu'il n'y a
/// qu'un effet système faillible : écrire `~/.ash/state.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspacesError {
    /// L'état n'a pas pu être écrit — disque plein, `~/.ash` non inscriptible.
    ///
    /// Ce n'est **pas** une raison de refuser l'épinglage : la ligne reste dans la colonne
    /// pour cette session, elle ne survivra simplement pas au redémarrage. Refuser un geste
    /// parce que le disque n'en veut pas serait incompréhensible — c'est la même conduite
    /// que `features::theme`.
    Io { path: PathBuf, why: String },
}

impl fmt::Display for WorkspacesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkspacesError::Io { path, why } => {
                write!(f, "écriture de {} impossible : {why}", path.display())
            }
        }
    }
}

impl std::error::Error for WorkspacesError {}
