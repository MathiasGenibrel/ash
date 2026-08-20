use std::fmt;

/// Erreurs de la feature `pty`.
///
/// Un type par feature : le frontend ne doit pas avoir à distinguer une erreur de PTY
/// d'une erreur de sonde, et le cœur ne doit pas connaître les erreurs de `portable-pty`.
#[derive(Debug)]
pub enum PtyError {
    /// Le shell n'a pas pu être lancé.
    Spawn(String),
    /// L'onglet demandé n'existe pas, ou n'existe plus.
    UnknownTab(String),
    /// Il n'y a rien à mettre en pause dans cet onglet.
    ///
    /// Un shell à son invite, ou un onglet que le système ne rend pas observable. Ce n'est
    /// pas une panne : c'est le refus d'arrêter le shell de l'utilisateur, qui rendrait
    /// l'onglet muet au clavier sans que rien n'en explique la raison.
    NothingToPause(String),
    /// Écriture, redimensionnement ou terminaison refusés par le système.
    Io(String),
}

impl fmt::Display for PtyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PtyError::Spawn(why) => write!(f, "impossible de lancer le shell : {why}"),
            PtyError::UnknownTab(id) => write!(f, "onglet inconnu : {id}"),
            PtyError::NothingToPause(id) => write!(
                f,
                "rien ne tourne dans l'onglet {id} : il n'y a rien à mettre en pause"
            ),
            PtyError::Io(why) => write!(f, "erreur d'entrée-sortie sur le PTY : {why}"),
        }
    }
}

impl std::error::Error for PtyError {}

// Le frontend reçoit un message, pas une structure : il n'a aucune décision à prendre
// sur la variante, et exposer l'énumération figerait un contrat qu'on ne veut pas tenir.
impl serde::Serialize for PtyError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
