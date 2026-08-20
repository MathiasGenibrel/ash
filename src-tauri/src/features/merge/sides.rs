//! Qui est de quel côté — **le** point dur de l'onglet de merge (spec §7.4).
//!
//! > « Les côtés portent le nom de leur branche, pas le jargon `ours`/`theirs` de git,
//! > qui s'inverse en rebase. »
//!
//! Ce n'est pas une préférence de vocabulaire, c'est une correction. Pendant un **merge**,
//! `ours` est la branche sur laquelle on est et `theirs` celle qu'on amène. Pendant un
//! **rebase**, git rejoue vos commits *par-dessus* la cible : `ours` désigne alors la
//! cible — celle sur laquelle vous rebasez — et `theirs` vos propres commits. Un onglet
//! qui écrirait `ours` à gauche dans les deux cas mettrait le travail de l'utilisateur du
//! mauvais côté une fois sur deux, au moment précis où il tranche.
//!
//! Les deux noms ne se lisent pas au même endroit non plus, et c'est la seconde moitié du
//! piège : pendant un rebase, `HEAD` est **détaché**, donc la branche courante ne dit
//! rien ; pendant un merge, `git` n'écrit aucun `head-name`, donc l'opération ne porte pas
//! la branche courante — il faut aller la chercher dans `HEAD`. Une seule des deux sources
//! est bonne à chaque fois.

use crate::features::git::{Head, Operation, OperationKind};

/// Les deux côtés d'un conflit, **nommés**.
///
/// Ce qui traverse la frontière ne contient ni `ours` ni `theirs` : les champs s'appellent
/// `left` et `right` parce que c'est ce qu'ils sont — deux colonnes —, et leur contenu est
/// un nom de branche.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MergeSides {
    /// Le côté que git appelle `ours` — à gauche.
    pub left: SideLabel,
    /// Le côté que git appelle `theirs` — à droite.
    pub right: SideLabel,
}

/// Un côté : son nom, et ce qu'il est.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SideLabel {
    /// Le nom de la branche — `main`, `feat/popup` — ou l'identifiant abrégé d'un commit.
    ///
    /// Jamais `ours` ni `theirs`. Quand git ne nomme rien du tout, c'est [`Self::role`] qui
    /// porte le sens, et `name` reprend le seul mot qui reste vrai (`HEAD`).
    pub name: String,
    /// Ce que ce côté est **dans cette opération**, en une phrase courte.
    ///
    /// « the branch you are rebasing onto », « the commits being replayed ». C'est elle qui
    /// empêche l'utilisateur de devoir se souvenir du sens du rebase, et elle change avec
    /// l'opération — pas avec la colonne.
    pub role: String,
}

/// Le nom qu'on donne à un côté que git n'a pas nommé.
const UNNAMED: &str = "HEAD";

/// Nomme les deux côtés d'une opération arrêtée.
///
/// `head` est le `HEAD` du worktree : il ne sert **qu'au merge**, où l'opération ne porte
/// pas la branche courante. Le lui passer pendant un rebase donnerait le commit détaché
/// sur lequel git a posé le worktree, c'est-à-dire un identifiant qui ne veut rien dire
/// pour l'utilisateur.
pub fn sides(operation: &Operation, head: &Head) -> MergeSides {
    match operation.kind {
        OperationKind::Merge => MergeSides {
            left: SideLabel {
                name: head_name(head),
                role: "the branch you are on".to_owned(),
            },
            right: SideLabel {
                name: operation.onto.clone().unwrap_or_else(|| UNNAMED.to_owned()),
                role: "the branch being merged in".to_owned(),
            },
        },
        // Rebase et `am` se lisent pareil : git réapplique des commits par-dessus une base,
        // et `ours` est la **base**. C'est l'inversion que la spec nomme.
        OperationKind::Rebase | OperationKind::Am => MergeSides {
            left: SideLabel {
                name: operation.onto.clone().unwrap_or_else(|| UNNAMED.to_owned()),
                role: match operation.kind {
                    OperationKind::Am => "the branch the patches apply to".to_owned(),
                    _ => "the branch you are rebasing onto".to_owned(),
                },
            },
            right: SideLabel {
                name: operation
                    .branch
                    .clone()
                    .unwrap_or_else(|| UNNAMED.to_owned()),
                role: match operation.kind {
                    OperationKind::Am => "the patch being applied".to_owned(),
                    _ => "your commits, being replayed".to_owned(),
                },
            },
        },
    }
}

/// Comment `continue` s'appelle pour cette opération — **du texte**, affiché sur le bouton.
///
/// Un merge se termine par un `commit`, pas par un `merge --continue`… sauf que git accepte
/// `git merge --continue` depuis 2.12 et en fait exactement un commit. Le nommer ainsi garde
/// une seule forme dans l'écran et une seule dans le code.
pub fn continuation(kind: OperationKind) -> String {
    match kind {
        OperationKind::Rebase => "git rebase --continue".to_owned(),
        OperationKind::Am => "git am --continue".to_owned(),
        OperationKind::Merge => "git merge --continue".to_owned(),
    }
}

fn head_name(head: &Head) -> String {
    match head {
        Head::Branch { name } => name.clone(),
        Head::Detached { commit } => commit.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::Progress;

    /// Test Data Builder : une opération arrêtée, dont on ne surcharge que ce qu'on regarde.
    struct StoppedBuilder {
        kind: OperationKind,
        branch: Option<String>,
        onto: Option<String>,
        head: Head,
    }

    impl StoppedBuilder {
        /// Un rebase de `feat` sur `main`, arrêté — le décor du critère d'acceptation.
        fn rebase() -> Self {
            Self {
                kind: OperationKind::Rebase,
                branch: Some("feat".to_owned()),
                onto: Some("main".to_owned()),
                // Pendant un rebase, git détache `HEAD` : c'est pour ça qu'il ne peut pas
                // servir à nommer un côté.
                head: Head::Detached {
                    commit: "1a2b3c4".to_owned(),
                },
            }
        }

        /// Un merge de `feat` **dans** `main` — les deux mêmes branches, l'autre sens.
        fn merge() -> Self {
            Self {
                kind: OperationKind::Merge,
                // git n'écrit aucun `head-name` pour un merge : l'opération ne porte pas la
                // branche courante, et c'est `HEAD` qui la donne.
                branch: None,
                onto: Some("feat".to_owned()),
                head: Head::Branch {
                    name: "main".to_owned(),
                },
            }
        }

        fn without_names(mut self) -> Self {
            self.branch = None;
            self.onto = None;
            self
        }

        fn kind(mut self, kind: OperationKind) -> Self {
            self.kind = kind;
            self
        }

        fn read(&self) -> MergeSides {
            sides(
                &Operation {
                    kind: self.kind,
                    branch: self.branch.clone(),
                    onto: self.onto.clone(),
                    progress: Some(Progress { step: 2, total: 5 }),
                },
                &self.head,
            )
        }
    }

    #[test]
    fn given_a_rebase_of_feat_onto_main_when_naming_the_sides_then_main_is_the_base_and_feat_is_yours(
    ) {
        // Given — pendant un rebase, git rejoue *vos* commits par-dessus la cible : son
        // `ours` est donc `main`, et son `theirs` est `feat`
        let stopped = StoppedBuilder::rebase();

        // When
        let sides = stopped.read();

        // Then
        assert_eq!(sides.left.name, "main");
        assert_eq!(sides.right.name, "feat");
        assert_eq!(sides.right.role, "your commits, being replayed");
    }

    #[test]
    fn given_a_merge_of_feat_into_main_when_naming_the_sides_then_main_is_on_the_left_and_feat_on_the_right(
    ) {
        // Given — les **mêmes deux branches** que le rebase ci-dessus, dans l'autre sens.
        // Si les côtés s'échangeaient, ce test et le précédent ne pourraient pas passer
        // ensemble : c'est exactement ce qu'ils sont là pour interdire.
        let stopped = StoppedBuilder::merge();

        // When
        let sides = stopped.read();

        // Then
        assert_eq!(sides.left.name, "main");
        assert_eq!(sides.right.name, "feat");
        assert_eq!(sides.left.role, "the branch you are on");
    }

    #[test]
    fn given_a_rebase_and_a_merge_of_the_same_two_branches_when_naming_their_sides_then_the_roles_are_not_the_same(
    ) {
        // Given — le même couple de branches, les deux opérations
        let rebase = StoppedBuilder::rebase().read();
        let merge = StoppedBuilder::merge().read();

        // When — les noms tombent du même côté…
        let same_names = rebase.left.name == merge.left.name;

        // Then — …mais ce que chaque côté *est* diffère, et c'est ce qui doit s'afficher :
        // le `main` d'un rebase est une base, le `main` d'un merge est là où l'on travaille
        assert!(same_names);
        assert_ne!(rebase.left.role, merge.left.role);
        assert_ne!(rebase.right.role, merge.right.role);
    }

    #[test]
    fn given_an_operation_git_did_not_name_when_naming_the_sides_then_no_side_is_called_ours_or_theirs(
    ) {
        // Given — un rebase sur un commit détaché que rien ne nomme
        let stopped = StoppedBuilder::rebase().without_names();

        // When
        let sides = stopped.read();

        // Then — le repli reste un mot de git *sur les refs*, jamais le jargon des côtés
        assert_eq!(sides.left.name, "HEAD");
        assert_eq!(sides.right.name, "HEAD");
        for label in [&sides.left, &sides.right] {
            assert!(!label.name.contains("ours"));
            assert!(!label.name.contains("theirs"));
        }
    }

    #[test]
    fn given_a_stopped_patch_application_when_naming_the_continue_button_then_it_says_git_am() {
        // Given — `git am` n'est ni un rebase ni un merge, et son `continue` ne s'écrit pas
        // comme les deux autres
        let kind = OperationKind::Am;

        // When
        let button = continuation(kind);

        // Then
        assert_eq!(button, "git am --continue");
        assert_eq!(
            StoppedBuilder::rebase().kind(kind).read().right.role,
            "the patch being applied"
        );
    }
}
