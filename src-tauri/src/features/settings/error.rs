use std::fmt;

/// Erreurs de la feature `settings`.
///
/// Un type par feature, comme `PtyError` : le frontend n'a pas à distinguer une erreur de
/// PTY d'un refus de déclaration, et le cœur n'a pas à connaître les erreurs du système.
#[derive(Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// Une entrée sans commande ne peut être associée à aucun processus (ADR-0006).
    EmptyCommand,
    /// Une commande est un **nom de processus**, pas une ligne de commande ni un chemin :
    /// c'est ce que la sonde d'ADR-0005 lit dans `tcgetpgrp`, et rien d'autre ne peut
    /// jamais correspondre.
    NotACommandName(String),
    /// `match` est la clé de la spec §9 : deux entrées homonymes désigneraient le même
    /// processus, et Ash ne saurait laquelle instrumenter.
    DuplicateCommand(String),
    /// L'adaptateur nommé n'est pas de ceux que cette version d'Ash embarque
    /// ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)).
    UnknownAdapter(String),
    /// L'entrée à oublier n'est pas — ou n'est plus — déclarée.
    UnknownTool(String),
    /// L'entrée n'a jamais été valide : la réinitialisation n'a rien où revenir (spec §9.1).
    NothingToRestore(String),
    /// Rien n'a été réinitialisé, donc rien à annuler.
    NothingToUndo(String),
    /// L'entrée ne désigne aucun dossier, et son adaptateur n'en propose pas.
    NoConfigFolder(String),
    /// Ash a refusé d'écrire, et dit pourquoi.
    ///
    /// C'est le refus le plus important du produit : il porte la phrase que la ligne `hooks`
    /// affichait déjà, parce que le backend refuse pour **la même raison** qui avait éteint
    /// le bouton ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
    HooksRefused(String),
    /// Le registre a été empoisonné par la panique d'un autre fil.
    Poisoned,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::EmptyCommand => write!(f, "une entrée doit nommer une commande"),
            SettingsError::NotACommandName(command) => {
                write!(f, "« {command} » n'est pas un nom de commande")
            }
            SettingsError::DuplicateCommand(command) => {
                write!(f, "« {command} » est déjà déclarée")
            }
            SettingsError::UnknownAdapter(adapter) => write!(f, "adaptateur inconnu : {adapter}"),
            SettingsError::UnknownTool(command) => write!(f, "outil inconnu : {command}"),
            SettingsError::NothingToRestore(command) => write!(
                f,
                "{command} n'a jamais été vérifiée : il n'y a aucun dossier où revenir"
            ),
            SettingsError::NothingToUndo(command) => {
                write!(f, "{command} n'a pas été réinitialisée")
            }
            SettingsError::NoConfigFolder(command) => {
                write!(f, "{command} ne désigne aucun dossier de configuration")
            }
            SettingsError::HooksRefused(why) => write!(f, "{why}"),
            SettingsError::Poisoned => write!(f, "registre des outils empoisonné"),
        }
    }
}

impl std::error::Error for SettingsError {}

// Le frontend reçoit un message, pas une structure : il n'a aucune décision à prendre sur
// la variante — il affiche la raison à côté du bouton, comme le demande la maquette.
impl serde::Serialize for SettingsError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
