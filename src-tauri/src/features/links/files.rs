//! La seule question que la feature pose au disque : « est-ce que ça existe ? ».
//!
//! Un trait, pour la raison qui vaut pour tous les effets système du dépôt, et une de plus
//! qui lui est propre : **la vérification est ce qui décide qu'un mot devient cliquable**,
//! donc c'est elle qu'il faut pouvoir mettre en scène — un chemin qui existe, un qui
//! n'existe pas, un lien cassé — sans rien poser dans le `$HOME` de qui lance `cargo test`.
//!
//! Le port est volontairement **minuscule** : une méthode, un booléen. `features/git` a son
//! propre [`FileSystem`](crate::features::git::FileSystem), plus large, parce qu'il lit des
//! fichiers de contrôle ; le partager ici donnerait à cette feature le droit de lire et de
//! parcourir des dossiers, là où elle n'a besoin que de savoir si un chemin est là.

use std::path::Path;

/// Ce que `links` demande au système de fichiers, et rien de plus.
pub trait Files: Send + Sync {
    /// Vrai si **quelque chose** est à ce chemin — fichier, dossier, ou lien symbolique.
    fn exists(&self, path: &Path) -> bool;
}

/// Le vrai disque.
pub struct SystemFiles;

impl Files for SystemFiles {
    /// `symlink_metadata` et non `Path::exists` : un lien symbolique **cassé** existe pour
    /// le Finder, qui sait le montrer et dire qu'il est cassé. Suivre le lien ferait
    /// disparaître le texte cliquable exactement là où on cherche pourquoi quelque chose
    /// ne marche pas — un `ls -l` qui montre une cible manquante est le cas d'usage même.
    fn exists(&self, path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }
}

#[cfg(test)]
pub struct FakeFiles {
    present: Vec<std::path::PathBuf>,
    asked: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl FakeFiles {
    pub fn with<'a>(present: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            present: present.into_iter().map(std::path::PathBuf::from).collect(),
            asked: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Combien de fois le disque a été interrogé — ce qui permet de vérifier qu'un candidat
    /// aberrant est refusé **avant** d'atteindre le système de fichiers.
    pub fn asked(&self) -> usize {
        self.asked.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl Files for FakeFiles {
    fn exists(&self, path: &Path) -> bool {
        self.asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.present.iter().any(|present| present == path)
    }
}
