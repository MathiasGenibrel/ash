use std::fmt;
use std::path::PathBuf;

/// Ce qui peut empêcher Ash d'écrire, ou de retirer, ses entrées.
///
/// Aucune de ces variantes n'est un incident technique à avaler : elles décrivent toutes un
/// fichier **de l'utilisateur** qu'Ash a refusé de toucher, et chacune doit finir sous ses
/// yeux avec de quoi décider (spec §10, écran de réglages #16).
#[derive(Debug, PartialEq, Eq)]
pub enum HookError {
    /// Ash ne saurait pas où écrire : le fichier n'est pas un objet JSON, ou une clé du
    /// chemin est occupée par autre chose qu'un conteneur.
    ///
    /// **C'est le seul refus qui reste**, et c'est voulu : depuis l'amendement du
    /// 2026-08-12 d'[ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), des hooks qui
    /// ne sont pas ceux d'Ash sont un conflit qui se montre et se tranche, pas une impasse.
    /// Un fichier qu'on ne sait pas lire, lui, ne se devine pas.
    NotAnObject { file: PathBuf },

    /// Le disque a dit non.
    Io { path: PathBuf, why: String },
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::NotAnObject { file } => write!(
                f,
                "{} is not a JSON object ash can write into: it wrote nothing",
                file.display()
            ),
            HookError::Io { path, why } => write!(f, "{} : {why}", path.display()),
        }
    }
}

impl std::error::Error for HookError {}
