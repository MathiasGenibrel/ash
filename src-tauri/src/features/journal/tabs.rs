//! Qui a écrit ce commit ? Le port par lequel le journal le demande, et la règle qui tranche.
//!
//! `journal` ne connaît ni les PTY, ni la sonde, ni la table des outils reconnus. Il pose
//! une question — *quel agent vit dans ce worktree, en ce moment ?* — et c'est le
//! composition root qui la relie au registre d'onglets, exactement comme il relie déjà `pty`
//! à la résolution de `git` et au superviseur d'états.
//!
//! **C'est ici que se tient la dépendance à la sonde d'ADR-0014** : l'attribution ne demande
//! aucun hook, donc elle marche pour tous les outils, `generic` compris
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)).

use std::path::Path;

use crate::shared::time::UnixMillis;

/// Ce qu'un onglet apporte au journal, et rien de plus.
///
/// Quatre champs sur la douzaine que porte un `TabInfo` : le journal n'a que faire du `cwd`,
/// du programme affiché, des sous-agents ou de l'état. Prendre le type du registre ferait
/// dépendre l'attribution de la forme d'une fiche d'onglet, qui change avec l'affichage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabAgent {
    pub tab_id: String,
    /// La racine du worktree où l'onglet vit, telle que `git` l'a résolue.
    pub worktree_root: String,
    /// L'outil reconnu dans l'avant-plan, ou `None` — un shell à son invite, un `vim`.
    pub agent: Option<String>,
    /// Depuis quand cet onglet est dans l'état qu'il montre.
    ///
    /// Sert à départager, et **seulement** à ça : deux agents dans le même worktree sont un
    /// cas réel — deux terminaux ouverts sur le même dossier — et il faut bien en nommer un.
    pub since: UnixMillis,
}

/// À qui le journal demande ce que les onglets portent en ce moment.
pub trait Tabs: Send + Sync {
    fn snapshot(&self) -> Vec<TabAgent>;
}

/// L'agent auquel un commit né dans ce worktree revient, ou aucun.
///
/// La règle, et ce qu'elle assume :
///
/// - **un worktree, pas un dépôt** : un commit naît dans un worktree, et deux worktrees d'un
///   même projet peuvent porter deux agents différents
///   ([ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md)) ;
/// - **un outil reconnu, ou rien** : un `git commit` tapé à la main dans un shell n'est pas
///   attribué. C'est la lettre d'ADR-0014 — « la colonne `by` ne montre un nom d'agent que
///   quand Ash l'a réellement observé » — et c'est ce qui garde le journal petit ;
/// - **le plus récemment entré dans son état** quand plusieurs agents cohabitent. C'est une
///   heuristique, assumée comme celle de la correspondance de repli : celui qui vient de
///   changer d'état est celui qui vient de faire quelque chose. Le prix d'une erreur est un
///   nom faux dans une colonne d'affichage.
pub fn author_of<'a>(worktree_root: &Path, tabs: &'a [TabAgent]) -> Option<&'a TabAgent> {
    let root = worktree_root.to_string_lossy();
    tabs.iter()
        .filter(|tab| tab.worktree_root == root && tab.agent.is_some())
        .max_by_key(|tab| tab.since)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : un onglet situé, avec ou sans agent.
    fn tab(tab_id: &str, worktree_root: &str, agent: Option<&str>, since: UnixMillis) -> TabAgent {
        TabAgent {
            tab_id: tab_id.to_owned(),
            worktree_root: worktree_root.to_owned(),
            agent: agent.map(str::to_owned),
            since,
        }
    }

    #[test]
    fn given_an_agent_working_in_a_worktree_when_a_commit_is_born_there_then_it_is_attributed_to_it(
    ) {
        // Given — un agent dans le worktree qui commite, un autre ailleurs
        let tabs = vec![
            tab("01J0TAB", "/wt/ash-sidebar", Some("claude"), 1_000),
            tab("01J0OTHER", "/dev/ash", Some("codex"), 2_000),
        ];

        // When
        let author = author_of(Path::new("/wt/ash-sidebar"), &tabs);

        // Then — un commit naît dans un worktree, pas dans un dépôt : l'agent du voisin,
        // même plus récent, n'a rien écrit ici
        assert_eq!(author.map(|tab| tab.tab_id.as_str()), Some("01J0TAB"));
    }

    #[test]
    fn given_only_a_shell_at_its_prompt_when_a_commit_is_born_then_nothing_is_attributed() {
        // Given — l'utilisateur tape `git commit` lui-même. ADR-0014 : la colonne `by` ne
        // montre un nom d'agent que quand Ash l'a réellement observé, et git a déjà un nom
        // d'auteur pour ce commit-là.
        let tabs = vec![tab("01J0TAB", "/dev/ash", None, 1_000)];

        // When
        let author = author_of(Path::new("/dev/ash"), &tabs);

        // Then — rien à écrire, donc rien dans le journal
        assert!(author.is_none());
    }

    #[test]
    fn given_two_agents_in_the_same_worktree_when_a_commit_is_born_then_the_latest_to_move_is_named(
    ) {
        // Given — deux terminaux ouverts sur le même worktree, deux outils. Il faut en
        // nommer un ; l'heuristique est celle qui vient d'agir.
        let tabs = vec![
            tab("01J0OLD", "/dev/ash", Some("codex"), 1_000),
            tab("01J0NEW", "/dev/ash", Some("claude"), 5_000),
        ];

        // When
        let author = author_of(Path::new("/dev/ash"), &tabs);

        // Then
        assert_eq!(author.map(|tab| tab.agent.as_deref()), Some(Some("claude")));
    }
}
