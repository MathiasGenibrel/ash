/// Le contenu de `~/.ash/state.json` : les épingles, les lignes repliées, et rien d'autre.
///
/// C'est **la** forme que la spec §9.2 énumère, et le seul endroit du dépôt où quelque chose
/// d'une session survit à la suivante en dehors des préférences d'apparence. Chaque champ
/// ajouté ici serait un fait de session de plus qui survit ; il n'y en a que deux, et le test
/// `given_a_pinned_and_collapsed_state_when_it_is_written_then_the_file_holds_nothing_else`
/// de [`super::store`] est ce qui l'empêche d'y en avoir trois.
///
/// Les deux listes sont des **ensembles ordonnés** : pas de doublon, et l'ordre d'ajout est
/// conservé — c'est celui dans lequel les lignes épinglées apparaîtront sous leur dépôt, et
/// il ne doit pas sauter d'un démarrage à l'autre.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Persisted {
    /// Les racines des worktrees épinglés — un chemin absolu, jamais une fiche.
    #[serde(default)]
    pub pinned: Vec<String>,
    /// Les lignes repliées, par la clé que la sidebar leur donne : la racine d'un worktree,
    /// ou la clé d'un groupe de dépôt (`repo:…`, `flat:…`).
    ///
    /// Une seule liste pour les deux niveaux : les deux familles de clés ne peuvent pas se
    /// confondre — un groupe est préfixé, un worktree est un chemin absolu — et les séparer
    /// donnerait deux champs qui disent la même chose.
    #[serde(default)]
    pub collapsed: Vec<String>,
}

impl Persisted {
    /// Ajoute ou retire une valeur d'une des deux listes. Rend `true` si quelque chose a
    /// changé.
    pub(super) fn toggle(list: &mut Vec<String>, value: String, present: bool) -> bool {
        let known = list.iter().position(|held| held == &value);
        match (known, present) {
            (None, true) => {
                list.push(value);
                true
            }
            (Some(index), false) => {
                list.remove(index);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_worktree_already_pinned_when_it_is_pinned_again_then_the_list_does_not_gain_a_twin()
    {
        // Given — deux clics sur la même épingle, ou une épingle posée par deux fenêtres
        let mut state = Persisted {
            pinned: vec!["/wt/ash-sidebar".to_owned()],
            collapsed: Vec::new(),
        };

        // When
        let changed = Persisted::toggle(&mut state.pinned, "/wt/ash-sidebar".to_owned(), true);

        // Then — rien n'a changé, donc rien n'est à réécrire ni à annoncer
        assert!(!changed);
        assert_eq!(state.pinned, vec!["/wt/ash-sidebar".to_owned()]);
    }

    #[test]
    fn given_three_pinned_worktrees_when_the_middle_one_is_unpinned_then_the_others_keep_their_order(
    ) {
        // Given — l'ordre est celui des lignes sous leur dépôt : il ne doit pas sauter
        let mut state = Persisted {
            pinned: vec!["/wt/a".to_owned(), "/wt/b".to_owned(), "/wt/c".to_owned()],
            collapsed: Vec::new(),
        };

        // When
        let changed = Persisted::toggle(&mut state.pinned, "/wt/b".to_owned(), false);

        // Then
        assert!(changed);
        assert_eq!(state.pinned, vec!["/wt/a".to_owned(), "/wt/c".to_owned()]);
    }
}
