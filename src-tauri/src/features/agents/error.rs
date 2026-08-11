use std::fmt;
use std::path::PathBuf;

/// Erreurs de la feature `agents`.
///
/// Elles ne concernent que la **mise en place** du transport : une fois le socket ouvert,
/// une trame qui cloche est ignorée, jamais remontée. Un hook mal formé ne doit pas
/// pouvoir faire tomber l'écoute pour tous les autres onglets.
#[derive(Debug)]
pub enum AgentError {
    /// Le dossier `~/.ash/` n'a pas pu être préparé.
    Directory(PathBuf, String),
    /// Le socket n'a pas pu être ouvert.
    Bind(PathBuf, String),
    /// Un autre Ash écoute déjà sur ce socket.
    ///
    /// Distinct de [`AgentError::Bind`] parce que la conduite n'est pas la même : ce n'est
    /// pas une panne du système, c'est une seconde instance, et lui prendre son socket
    /// couperait les hooks de tous ses onglets.
    AlreadyListening(PathBuf),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::Directory(path, why) => {
                write!(f, "dossier {} inutilisable : {why}", path.display())
            }
            AgentError::Bind(path, why) => {
                write!(f, "socket {} impossible à ouvrir : {why}", path.display())
            }
            AgentError::AlreadyListening(path) => {
                write!(f, "un autre Ash écoute déjà sur {}", path.display())
            }
        }
    }
}

impl std::error::Error for AgentError {}
