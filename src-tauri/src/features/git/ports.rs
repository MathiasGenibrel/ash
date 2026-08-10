use std::path::{Path, PathBuf};

/// Ce qu'on trouve à un chemin.
///
/// Seule la distinction fichier / dossier compte ici, et elle porte toute la tâche : un
/// `.git` **dossier** est une racine de dépôt, un `.git` **fichier** est un worktree lié
/// qui ne dit rien de son dépôt ([ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    File,
    Directory,
}

/// Le système de fichiers, derrière un trait que la feature possède.
///
/// C'est ce qui rend la résolution testable sans toucher au disque, et c'est la seule
/// dépendance de la résolution : **le binaire `git` n'est jamais invoqué**. Tout ce qu'il
/// faut est dans les fichiers de contrôle (`.git`, `commondir`), leur lecture est plus
/// rapide qu'un `fork`, et elle ne dépend pas de la configuration de la machine.
///
/// Les liens symboliques sont **suivis**, et les chemins rendus par [`Self::canonicalize`]
/// sont réels : sans cela, `/tmp/x` et `/private/tmp/x` désigneraient deux dépôts
/// différents sur macOS, et la sidebar afficherait deux fois le même projet.
pub trait FileSystem: Send + Sync {
    /// Ce qui existe à ce chemin, `None` si rien n'y existe.
    fn entry(&self, path: &Path) -> Option<Entry>;

    /// Le contenu d'un fichier de contrôle — quelques dizaines d'octets.
    fn read_to_string(&self, path: &Path) -> Result<String, String>;

    /// Vrai si le dossier existe **et** contient au moins une entrée.
    fn has_entries(&self, path: &Path) -> bool;

    /// Le chemin réel : `..` résolus, liens suivis. `None` si le chemin n'existe pas.
    fn canonicalize(&self, path: &Path) -> Option<PathBuf>;
}
