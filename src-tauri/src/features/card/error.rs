use std::path::PathBuf;

/// Ce qui peut mal tourner quand Ash touche à la fiche.
///
/// **Un refus n'est pas une erreur** : c'est une réponse, et elle vit dans
/// [`LogWrite::Refused`](super::LogWrite). Un bloc édité à la main ou parti en conflit est un
/// cas nominal du produit — il se raconte à l'écran, avec un diff, et l'utilisateur tranche.
/// Ne restent ici que les pannes du disque, dont l'écran ne peut rien dire d'autre que
/// « ça n'a pas pu se lire ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardError {
    Io { path: PathBuf, why: String },
}

impl std::fmt::Display for CardError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CardError::Io { path, why } => write!(out, "{}: {why}", path.display()),
        }
    }
}
