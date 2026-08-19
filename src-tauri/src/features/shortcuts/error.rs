use std::fmt;
use std::path::PathBuf;

/// Erreurs de la feature `shortcuts`.
///
/// Un type par feature, comme partout ailleurs. Les trois premières variantes sont des
/// **refus de capture**, et leur message part tel quel dans le bloc de capture : c'est le
/// backend qui possède la règle, donc c'est lui qui l'écrit
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutError {
    /// Une touche qu'Ash ne sait pas lier — `F13`, une touche morte, un modificateur seul.
    UnusableKey { code: String },
    /// Une touche sans `⌘`, `⌃` ni `⌥` : la lier prendrait une touche nue au shell.
    BareKey,
    /// Une action que le menu ne déclare pas. Le frontend envoie un identifiant en clair,
    /// et rien ne le vérifie à la compilation.
    UnknownAction { action: String },
    /// L'action existe, mais son raccourci n'est pas à donner — voir `rebindable`.
    FixedBinding { action: String },
    /// Les liaisons n'ont pas pu être écrites — disque plein, `~/.ash` non inscriptible.
    ///
    /// Ce n'est **pas** une raison de refuser le changement : le raccourci s'applique tout
    /// de suite, il ne survivra simplement pas au redémarrage. Même règle que `theme`.
    Io { path: PathBuf, why: String },
}

impl fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Les trois premiers messages s'affichent dans le bloc de capture, donc en
            // anglais et en minuscules, comme tout ce que la fenêtre de réglages écrit.
            ShortcutError::UnusableKey { code } => {
                write!(f, "ash cannot bind {code}")
            }
            ShortcutError::BareKey => {
                write!(f, "add ⌘, ⌃ or ⌥ — a bare key belongs to the shell")
            }
            ShortcutError::FixedBinding { action } => {
                write!(f, "{action} is not rebindable")
            }
            ShortcutError::UnknownAction { action } => {
                write!(f, "action inconnue : {action}")
            }
            ShortcutError::Io { path, why } => {
                write!(f, "écriture de {} impossible : {why}", path.display())
            }
        }
    }
}

impl std::error::Error for ShortcutError {}
