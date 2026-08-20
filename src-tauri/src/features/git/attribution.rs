//! Qui a écrit ces commits ? Le port par lequel le graphe le demande.
//!
//! **La jointure commits × attribution se fait ici, du côté de `git`, et c'est un choix.**
//! Les deux features ont chacune une moitié : `git` détient les commits — c'est elle qui
//! porte le seul appel au binaire —, `journal` détient l'attribution
//! ([ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md)). Trois endroits
//! étaient possibles, et deux sont mauvais :
//!
//! - **dans `journal`** : il faudrait qu'il sache lire un graphe, donc qu'il connaisse les
//!   parents, les couloirs et les refs. Or `journal` a déjà un port `CommitLog` par lequel il
//!   *demande* des commits à `git` : lui faire aussi rendre un graphe inverserait la
//!   dépendance qu'il a lui-même posée ;
//! - **dans le composition root** : `lib.rs` assemblerait alors une page de graphe, c'est-à-dire
//!   qu'il porterait une règle du produit — et il n'a pas de test unitaire.
//!
//! Reste **ici**, et c'est la forme que le dépôt emploie déjà partout : la feature qui pose
//! la question possède le port, l'autre y répond, `lib.rs` relie. `pty` possède
//! `AgentStates` et `agents` répond ; `journal` possède `CommitLog` et `git` répond ; `git`
//! possède [`Attributions`] et `journal` répond. Aucune des deux ne connaît l'autre.
//!
//! # Pourquoi la demande est **groupée**
//!
//! `CommitJournal::attribution` relit le fichier du dépôt à chaque appel : une page de deux
//! cents lignes ferait deux cents lectures du même fichier. La réponse n'est **pas** un
//! cache — un cache demanderait d'inventer ce qui l'invalide, alors que le journal est
//! append-only et grossit sous les pieds de qui le lit. C'est une question posée pour toute
//! la page d'un coup : une lecture, une résolution par commit.

use super::CommitRecord;

/// Ce qu'Ash a observé de l'écriture d'un commit.
///
/// `prompt` est **facultatif et souvent vide**, et ce n'est pas un manque à combler : sa
/// source existe — l'entrée standard du hook `UserPromptSubmit` — mais l'atteindre engage une
/// décision de confidentialité que personne n'a prise. Le panneau de détail est écrit pour
/// l'afficher quand il existe et pour dire qu'il n'y en a pas quand il est vide ; il n'en
/// fabrique aucun.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribution {
    /// L'outil — `claude`, `codex` —, et non l'adaptateur qui le traduit.
    pub agent: String,
    /// L'onglet où il tournait. Ce qui permettra de retrouver la conversation.
    pub tab_id: String,
    pub prompt: Option<String>,
}

/// À qui le graphe demande ce qu'Ash a vu.
///
/// `repo` est le dossier git **commun**, la même clé que celle par laquelle la sidebar groupe
/// les worktrees d'un même projet et par laquelle le journal nomme son fichier.
///
/// La réponse est **alignée** sur la demande : une entrée par commit, `None` quand Ash ne l'a
/// pas vu naître. « Rien » n'est pas un échec — c'est un commit tapé à la main, ou né avant
/// qu'Ash regarde, et la colonne `by` y montre alors le nom d'auteur git.
pub trait Attributions: Send + Sync {
    fn of(&self, repo: &str, commits: &[CommitRecord]) -> Vec<Option<Attribution>>;
}
