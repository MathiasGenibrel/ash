use std::fmt;
use std::path::PathBuf;

/// Erreurs de la feature `theme`.
///
/// Un type par feature, comme partout ailleurs. Il n'y a qu'une variante parce qu'il n'y a
/// qu'un effet système : écrire la préférence quelque part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeError {
    /// Le choix n'a pas pu être écrit — disque plein, `~/.ash` non inscriptible.
    ///
    /// Ce n'est **pas** une raison de refuser le changement : le thème s'applique tout de
    /// suite, il ne survivra simplement pas au redémarrage.
    Io { path: PathBuf, why: String },
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeError::Io { path, why } => {
                write!(f, "écriture de {} impossible : {why}", path.display())
            }
        }
    }
}

impl std::error::Error for ThemeError {}
