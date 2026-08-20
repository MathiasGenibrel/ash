use std::fmt;
use std::path::PathBuf;

/// Erreurs de la feature `journal`.
///
/// Un type par feature, comme partout ailleurs dans ce crate. Une seule variante suffit, et
/// c'est un fait sur le produit plutôt qu'une économie : tout ce qui peut mal se passer ici
/// est un refus du disque. Un commit qu'on n'attribue pas n'est **pas** une erreur — c'est
/// le cas nominal d'ADR-0014, où la colonne `by` affiche le nom d'auteur git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    Io { path: PathBuf, why: String },
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalError::Io { path, why } => {
                write!(f, "journal {} : {why}", path.display())
            }
        }
    }
}

impl std::error::Error for JournalError {}
