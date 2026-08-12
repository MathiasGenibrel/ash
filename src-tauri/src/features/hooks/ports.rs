use std::path::Path;

use super::document::Document;

/// Les fichiers de configuration **de l'utilisateur**, derrière un trait que la feature
/// possède.
///
/// C'est le port le plus important du dépôt à ce jalon : sans lui, la seule façon de
/// prouver qu'Ash ne touche à rien hors de ses marqueurs serait d'écrire dans le
/// `~/.claude/settings.json` de qui lance les tests. Les scénarios qui comptent — un `.bak`
/// déjà présent, un bloc édité à la main, un fichier qu'Ash a créé et doit reprendre — se
/// jouent tous en mémoire.
///
/// La surface est délibérément étroite : lire, écrire, copier, effacer. Aucune méthode ne
/// prend de contenu partiel, parce que la feature ne modifie jamais un fichier en place —
/// elle en compose le texte complet et le remplace d'un coup.
///
/// Et ce texte n'est pas une chaîne quelconque : c'est un [`Document`], le seul type qui
/// sache se composer, et qui ne se compose que d'une des deux façons qu'Ash s'autorise. La
/// règle « rien n'est modifié hors marqueurs » vit donc **dans la signature du port**, pas
/// seulement dans les tests de ses appelants.
pub trait ConfigFiles: Send + Sync {
    /// Le contenu du fichier, ou `None` s'il n'existe pas.
    ///
    /// L'absence n'est pas une erreur : un dossier de configuration tout neuf n'a pas encore
    /// de `settings.json`, et c'est le cas nominal d'une première installation.
    fn read(&self, path: &Path) -> Result<Option<String>, String>;

    fn exists(&self, path: &Path) -> bool;

    /// Remplace le fichier par ce texte, **sans état intermédiaire visible**.
    ///
    /// L'implémentation système écrit à côté puis renomme : une coupure de courant au
    /// milieu d'une installation ne doit pas laisser le `settings.json` de l'utilisateur
    /// tronqué. C'est une exigence du port, pas un détail de son adaptateur — un double qui
    /// ne la respecterait pas ne doublerait pas la même chose.
    fn write(&self, path: &Path, content: &Document) -> Result<(), String>;

    fn copy(&self, from: &Path, to: &Path) -> Result<(), String>;

    fn remove(&self, path: &Path) -> Result<(), String>;
}
