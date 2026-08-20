use std::fmt;

/// Erreurs de la feature `merge`.
///
/// Un type par feature, comme ailleurs : le frontend ne doit pas avoir à distinguer un
/// onglet de merge inconnu d'un PTY mort, et cette feature ne connaît ni l'un ni l'autre.
#[derive(Debug, PartialEq, Eq)]
pub enum MergeError {
    /// L'onglet demandé n'existe pas, ou n'existe plus.
    UnknownTab(String),
    /// Le worktree de cet onglet n'a plus d'opération arrêtée.
    ///
    /// Ce n'est pas une panne : c'est ce qui arrive quand le rebase a été terminé ou
    /// abandonné ailleurs — dans un terminal, par un agent. L'onglet reste ouvert et le
    /// dit ; il ne se referme pas tout seul (ADR-0010).
    NothingStopped(String),
    /// Le fichier en conflit n'a pas pu être lu — effacé, ou pas de l'UTF-8.
    Unreadable(String),
    /// Le fichier n'a pas pu être réécrit. Rien n'a été mis dans l'index.
    NotWritten(String),
    /// Le rang de hunk demandé n'existe plus dans ce fichier.
    ///
    /// Le fichier a changé sous les doigts de l'utilisateur. Écrire quand même
    /// remplacerait un autre conflit que celui qu'il regardait.
    HunkMoved(String),
}

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeError::UnknownTab(id) => write!(f, "onglet de merge inconnu : {id}"),
            MergeError::NothingStopped(root) => {
                write!(f, "plus aucune opération arrêtée dans {root}")
            }
            MergeError::Unreadable(path) => write!(f, "conflit illisible : {path}"),
            MergeError::NotWritten(why) => write!(f, "le fichier n'a pas pu être écrit : {why}"),
            MergeError::HunkMoved(path) => write!(
                f,
                "ce conflit a bougé dans {path} : le fichier a changé, rien n'a été écrit"
            ),
        }
    }
}

impl std::error::Error for MergeError {}

// Le frontend reçoit un message, pas une structure — la même règle que `PtyError`.
impl serde::Serialize for MergeError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
