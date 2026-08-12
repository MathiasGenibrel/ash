use std::fmt;
use std::path::PathBuf;

/// Ce qui peut empêcher Ash d'écrire, ou de retirer, son bloc.
///
/// Aucune de ces variantes n'est un incident technique à avaler : elles décrivent toutes un
/// fichier **de l'utilisateur** qu'Ash a refusé de toucher, et chacune doit finir sous ses
/// yeux avec de quoi décider (spec §10, écran de réglages #16).
#[derive(Debug, PartialEq, Eq)]
pub enum HookError {
    /// Le bloc a été modifié à la main : Ash ne réécrit pas par-dessus.
    ///
    /// Porte le diff, parce que la spec §10 ne demande pas seulement de refuser — elle
    /// demande de **signaler, proposer le diff, et demander**. Refuser sans montrer ce qui
    /// diffère ne laisse à l'utilisateur que le choix de tout effacer.
    HandEdited { file: PathBuf, diff: String },

    /// Le fichier porte déjà une clé `hooks` qui n'est pas la nôtre.
    ///
    /// Ash n'y touche pas : fusionner deux configurations de hooks demanderait de modifier
    /// du texte hors de ses marqueurs, ce que toute cette feature existe pour interdire.
    ForeignHooks { file: PathBuf },

    /// Le fichier n'est pas un objet JSON — pas d'accolade ouvrante où insérer le bloc.
    NotAnObject { file: PathBuf },

    /// Le disque a dit non.
    Io { path: PathBuf, why: String },
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::HandEdited { file, diff } => write!(
                f,
                "le bloc d'Ash dans {} a été modifié à la main ; rien n'a été écrit :\n{diff}",
                file.display()
            ),
            HookError::ForeignHooks { file } => write!(
                f,
                "{} porte déjà des hooks qui ne viennent pas d'Ash ; rien n'a été écrit",
                file.display()
            ),
            HookError::NotAnObject { file } => write!(
                f,
                "{} n'est pas un objet JSON : Ash ne saurait pas où poser son bloc",
                file.display()
            ),
            HookError::Io { path, why } => write!(f, "{} : {why}", path.display()),
        }
    }
}

impl std::error::Error for HookError {}
