use std::path::Path;

use super::document::CardDocument;

/// Les fichiers de la fiche, derrière un trait que la feature possède.
///
/// Le jumeau de `hooks::ConfigFiles`, et il n'a pas été mutualisé avec lui pour une raison
/// qui se lit dans la signature : `write` n'accepte pas le même document. Là-bas c'est un
/// [`hooks::Document`](crate::features::hooks::Document), qui ne se compose que d'entrées
/// portant le marqueur `# ash:hook v` ; ici c'est un [`CardDocument`], qui ne se compose que
/// d'un bloc remplacé, d'un bloc ajouté ou d'un fichier neuf. Un port commun aurait un
/// `write(&self, path, &str)`, et les deux garanties qui vivent aujourd'hui **dans les
/// types** redeviendraient de la prudence.
///
/// Ce qui, lui, est partagé pour de bon : [`crate::shared::text_diff`].
///
/// La surface est étroite — lire, écrire, copier — et elle ne sait rien du markdown : le
/// fichier est du texte, et il est remplacé d'un coup.
pub trait CardFiles: Send + Sync {
    /// Le contenu du fichier, ou `None` s'il n'existe pas.
    ///
    /// L'absence n'est pas une erreur : une branche sans fiche est le cas de départ de
    /// toutes les branches.
    fn read(&self, path: &Path) -> Result<Option<String>, String>;

    fn exists(&self, path: &Path) -> bool;

    /// Remplace le fichier par ce texte, **sans état intermédiaire visible**, en créant les
    /// dossiers manquants — `.ash/` n'existe pas avant la première fiche.
    ///
    /// L'implémentation système écrit à côté puis renomme : une coupure au milieu d'une
    /// écriture ne doit pas laisser la fiche de l'utilisateur tronquée. C'est une exigence
    /// du port, pas un détail de son adaptateur.
    fn write(&self, path: &Path, content: &CardDocument) -> Result<(), String>;

    fn copy(&self, from: &Path, to: &Path) -> Result<(), String>;
}

/// Ce que les agents ont écrit dans ce worktree — la matière des trois colonnes.
///
/// **La feature ne connaît pas le journal**, et c'est voulu : elle pose une question, le
/// composition root la relie à `features::journal` et à `features::git`, comme il relie déjà
/// `journal` au registre de PTY. C'est la même forme que `pty` avec `AgentStates`, et elle
/// vaut ici pour une raison de plus — la réponse demande **deux** features (les commits que
/// git rend, l'attribution que le journal garde), et aucune des deux n'a à connaître la
/// fiche.
///
/// Ce que cette source ne sait **pas**, et que la fiche ne prétendra donc pas savoir :
///
/// - les commits nés **avant le démarrage d'Ash** ne sont pas attribués (ADR-0014) : une
///   fiche ouverte au premier lancement peut être vide alors que la branche a dix commits ;
/// - les commits écrits **à la main** n'ont pas d'agent, et n'apparaissent pas ;
/// - le journal est **local** : la fiche voyage avec la branche, l'attribution non. Chez le
///   collègue, la table dira ce que *sa* machine a observé.
pub trait AgentWork: Send + Sync {
    fn in_worktree(&self, worktree_root: &Path) -> Vec<WorkRecord>;
}

/// Un commit observé : qui, et quand. C'est tout ce dont la table a besoin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkRecord {
    /// La commande reconnue — `claude`, `codex` (ADR-0006, ADR-0014).
    pub agent: String,
    /// La date d'auteur, en **secondes** Unix, comme `git::CommitRecord::authored_at`.
    pub authored_at: u64,
}
