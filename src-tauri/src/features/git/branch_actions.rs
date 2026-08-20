//! Ce que `⌘⏎` ouvre : les actions de branche, et ce qu'elles refusent (spec §7.1).
//!
//! Trois règles gouvernent ce fichier, et aucune n'est décorative.
//!
//! **Une action nomme ses deux côtés, partout.** « Rebase feat/popup onto main », jamais
//! « Rebase » — *y compris dans les messages d'erreur*, ce que la spec écrit en gras. C'est
//! pour cela que le libellé est composé **ici** et non dans la webview : le message d'échec
//! est fabriqué du côté qui reçoit la sortie de git, et deux compositions séparées
//! finiraient par nommer deux choses différentes pour le même geste.
//!
//! **On refuse plutôt qu'on devine.** Une branche prise par un autre worktree ne se
//! *checkout* pas — git le refuserait, et Ash le dit avant, en nommant le worktree qui la
//! détient. Une branche distante ne se *checkout* pas non plus : `git switch origin/x`
//! créerait une branche locale, et créer une ref n'est pas ce qu'on a demandé. Elle reste
//! une cible parfaitement valable pour un rebase ou un merge, qui ne créent rien.
//!
//! **Le nom passé au processus vient du dépôt, pas du frontend.** C'est le second verrou
//! contre l'injection d'arguments décrit dans [`super::git_cli`] : le nom reçu est cherché
//! dans la liste que `for-each-ref` vient de rendre, et c'est **la chaîne trouvée** qui est
//! passée à `git`. Une branche nommée `--upload-pack=…` n'existe pas dans un dépôt réel ;
//! si elle y était, elle traverserait après un `--`, en opérande.

use std::path::Path;
use std::sync::Arc;

use super::branches::{Branch, BranchKind, BranchOverview};
use super::git_cli::TreeWriter;

/// Les trois verbes livrés. Une énumération fermée, et c'est le point : le frontend envoie
/// un mot de cette liste, jamais une ligne de commande.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum BranchAction {
    /// Passer sur la branche choisie, dans ce worktree.
    Checkout,
    /// Rebaser la branche **courante** sur celle qu'on a choisie.
    Rebase,
    /// Fusionner la branche choisie **dans** la courante.
    Merge,
}

/// Ce qu'une action propose, tel que le sous-menu de `⌘⏎` le montre.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ActionOffer {
    pub action: BranchAction,
    /// Le libellé, qui nomme les deux côtés. Toujours présent, même quand l'action est
    /// refusée : un bouton éteint reste visible **avec sa raison**, il n'est pas masqué.
    pub label: String,
    /// Pourquoi elle est refusée, ou `None` si elle ne l'est pas.
    pub refused: Option<String>,
    /// Elle touche l'arbre de travail, donc elle dérange un agent qui y écrit.
    ///
    /// Les trois verbes livrés le font. Le champ existe quand même : c'est lui que la
    /// confirmation lit, et le jour où une action sans effet sur l'arbre est ajoutée —
    /// renommer une branche, la publier — elle ne doit pas hériter de la question.
    pub touches_tree: bool,
}

/// Ce qu'une action a fait, ou n'a pas fait.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ActionOutcome {
    /// Le libellé de ce qui a été tenté — les deux côtés, encore.
    pub label: String,
    pub success: bool,
    /// Ce que git a dit, tel quel. Vide quand il n'a rien dit.
    pub output: String,
}

/// Composée à partir de ce que la popup montre — donc jamais avec un côté manquant.
///
/// `current` est `None` sur un `HEAD` détaché, et c'est ce qui refuse le rebase et le merge :
/// il n'y a alors pas de branche « depuis » ou « dans » à nommer, et une phrase à un seul
/// côté est exactement ce que la spec interdit.
fn label(action: BranchAction, branch: &Branch, current: Option<&str>, here: &str) -> String {
    match action {
        // Le checkout n'a pas deux branches, il a une branche et un **endroit** : c'est le
        // worktree qui est son second côté, et c'est lui que le geste déplace.
        BranchAction::Checkout => match current {
            Some(leaving) => format!("Check out {} in {here}, leaving {leaving}", branch.name),
            None => format!("Check out {} in {here}", branch.name),
        },
        BranchAction::Rebase => match current {
            Some(from) => format!("Rebase {from} onto {}", branch.name),
            None => format!("Rebase this detached HEAD onto {}", branch.name),
        },
        BranchAction::Merge => match current {
            Some(into) => format!("Merge {} into {into}", branch.name),
            None => format!("Merge {} into this detached HEAD", branch.name),
        },
    }
}

/// Pourquoi cette action est refusée sur cette branche, ou `None`.
fn refusal(action: BranchAction, branch: &Branch, current: Option<&str>) -> Option<String> {
    if current.is_none() && action != BranchAction::Checkout {
        return Some(
            "this worktree is on a detached HEAD: there is no branch to rebase or merge into"
                .to_owned(),
        );
    }

    match action {
        BranchAction::Checkout => {
            if let Some(held) = &branch.worktree {
                // Le fait d'ADR-0012, dit avant que git ne le refuse : deux worktrees ne
                // peuvent pas être sur la même branche.
                return Some(format!(
                    "{} is checked out in {} — a branch lives in one worktree at a time",
                    branch.name, held.name
                ));
            }
            if branch.kind == BranchKind::Remote {
                return Some(format!(
                    "{} is a remote branch: checking it out would create a local branch, and Ash \
                     does not create refs",
                    branch.name
                ));
            }
            if current == Some(branch.name.as_str()) {
                return Some(format!("{} is already checked out here", branch.name));
            }
            None
        }
        BranchAction::Rebase | BranchAction::Merge => {
            (current == Some(branch.name.as_str())).then(|| {
                format!(
                    "{} is the current branch: it cannot be both sides of the same action",
                    branch.name
                )
            })
        }
    }
}

/// Les trois actions offertes pour une branche, refus compris.
///
/// **Toutes les trois, toujours** : une action refusée reste visible avec sa raison. La
/// masquer ferait croire qu'elle n'existe pas — la même règle que le socle de composants
/// impose côté TypeScript, appliquée du côté qui la décide.
pub fn offers(overview: &BranchOverview, branch: &Branch) -> Vec<ActionOffer> {
    let here = Path::new(&overview.worktree_root)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| overview.worktree_root.clone());
    let current = overview.current.as_deref();

    [
        BranchAction::Checkout,
        BranchAction::Rebase,
        BranchAction::Merge,
    ]
    .into_iter()
    .map(|action| ActionOffer {
        action,
        label: label(action, branch, current, &here),
        refused: refusal(action, branch, current),
        touches_tree: true,
    })
    .collect()
}

/// La ligne de commande d'une action — une liste **fermée**, composée ici et nulle part ailleurs.
///
/// Le `--` sépare les options des opérandes partout où git l'accepte. `switch` le prend ;
/// `rebase` et `merge` ne le prennent pas pour une révision — c'est pourquoi le nom vérifié
/// contre le dépôt est le verrou qui compte pour eux, et non ce séparateur.
fn command(action: BranchAction, branch: &str) -> Vec<String> {
    match action {
        BranchAction::Checkout => vec!["switch".to_owned(), "--".to_owned(), branch.to_owned()],
        BranchAction::Rebase => vec!["rebase".to_owned(), branch.to_owned()],
        BranchAction::Merge => vec![
            "merge".to_owned(),
            // Sans lui, git ouvre `$EDITOR` : un processus sans terminal ni fenêtre, qui ne
            // rendrait jamais la main et qu'il faudrait tuer à la main.
            "--no-edit".to_owned(),
            branch.to_owned(),
        ],
    }
}

/// Lance une action de branche, après l'avoir refusée si elle devait l'être.
///
/// L'ordre compte : la liste est relue **avant** d'agir, et c'est elle qui décide. Ce que le
/// frontend envoie est un nom, pas une cible — le nom sert à retrouver la branche dans ce que
/// le dépôt contient à cet instant, et c'est cette branche-là qui traverse. Une branche
/// effacée entre l'ouverture de la popup et le geste est donc refusée, pas devinée.
pub fn run(
    writer: &Arc<dyn TreeWriter>,
    overview: &BranchOverview,
    action: BranchAction,
    branch_name: &str,
) -> ActionOutcome {
    let Some(branch) = overview
        .sections
        .iter()
        .flat_map(|section| &section.branches)
        .find(|candidate| candidate.name == branch_name)
    else {
        return ActionOutcome {
            label: format!("{action:?} {branch_name}"),
            success: false,
            output: format!("{branch_name} is no longer a branch of this repository"),
        };
    };

    let offer = offers(overview, branch)
        .into_iter()
        .find(|offer| offer.action == action);
    let Some(offer) = offer else {
        return ActionOutcome {
            label: format!("{action:?} {branch_name}"),
            success: false,
            output: "this action is not offered for this branch".to_owned(),
        };
    };

    if let Some(why) = offer.refused {
        return ActionOutcome {
            label: offer.label,
            success: false,
            output: why,
        };
    }

    // `branch.name` et non `branch_name` : c'est la chaîne que le dépôt a rendue qui part
    // vers le processus, jamais celle que la webview a envoyée.
    let completed = writer.run(
        Path::new(&overview.worktree_root),
        &command(action, &branch.name),
    );

    match completed {
        Some(completed) => ActionOutcome {
            label: offer.label,
            success: completed.success,
            output: completed.output,
        },
        None => ActionOutcome {
            label: offer.label,
            success: false,
            output: "git could not be started".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::branches::{overview, BranchWorktree};
    use std::sync::Mutex;

    /// Un `git` qui n'existe pas : il note ce qu'on lui a demandé, et répond ce qu'on a dit.
    #[derive(Default)]
    struct FakeGit {
        ran: Mutex<Vec<Vec<String>>>,
        answer: Mutex<Option<super::super::git_cli::Completed>>,
    }

    impl FakeGit {
        fn answering(success: bool, output: &str) -> Self {
            Self {
                ran: Mutex::new(Vec::new()),
                answer: Mutex::new(Some(super::super::git_cli::Completed {
                    success,
                    output: output.to_owned(),
                })),
            }
        }

        fn ran(&self) -> Vec<Vec<String>> {
            self.ran.lock().map(|seen| seen.clone()).unwrap_or_default()
        }
    }

    impl TreeWriter for FakeGit {
        fn run(&self, _root: &Path, args: &[String]) -> Option<super::super::git_cli::Completed> {
            if let Ok(mut ran) = self.ran.lock() {
                ran.push(args.to_vec());
            }
            self.answer.lock().ok().and_then(|answer| answer.clone())
        }
    }

    /// Un dépôt sur `main`, avec `feat/popup` libre — le décor de tous les scénarios.
    fn on_main() -> BranchOverview {
        overview(
            Path::new("/dev/ash"),
            "a1b2c3d\t200\t*\trefs/heads/main\na1b2c3d\t100\t \trefs/heads/feat/popup\n\
             a1b2c3d\t100\t \trefs/remotes/origin/main",
            "worktree /dev/ash\nHEAD a1b2c3d\nbranch refs/heads/main\n",
            Vec::new(),
        )
    }

    fn branch(shown: &BranchOverview, name: &str) -> Branch {
        shown
            .sections
            .iter()
            .flat_map(|section| &section.branches)
            .find(|candidate| candidate.name == name)
            .cloned()
            .expect("la branche du scénario")
    }

    fn offer(shown: &BranchOverview, name: &str, action: BranchAction) -> ActionOffer {
        offers(shown, &branch(shown, name))
            .into_iter()
            .find(|offer| offer.action == action)
            .expect("l'action du scénario")
    }

    #[test]
    fn given_a_branch_and_the_current_one_when_the_actions_are_offered_then_each_names_both_sides()
    {
        // Given
        let shown = on_main();

        // When
        let labels: Vec<String> = offers(&shown, &branch(&shown, "feat/popup"))
            .into_iter()
            .map(|offer| offer.label)
            .collect();

        // Then — « Rebase », jamais tout seul (spec §7.1)
        assert_eq!(
            labels,
            vec![
                "Check out feat/popup in ash, leaving main".to_owned(),
                "Rebase main onto feat/popup".to_owned(),
                "Merge feat/popup into main".to_owned(),
            ]
        );
    }

    #[test]
    fn given_a_branch_held_by_another_worktree_when_a_checkout_is_offered_then_it_is_refused_and_names_that_worktree(
    ) {
        // Given
        let shown = overview(
            Path::new("/dev/ash"),
            "a1b2c3d\t200\t*\trefs/heads/main\na1b2c3d\t100\t \trefs/heads/feat/sidebar",
            "worktree /dev/ash\nHEAD a1b2c3d\nbranch refs/heads/main\n\n\
             worktree /wt/ash-sidebar\nHEAD a1b2c3d\nbranch refs/heads/feat/sidebar\n",
            Vec::new(),
        );

        // When
        let checkout = offer(&shown, "feat/sidebar", BranchAction::Checkout);

        // Then — dit avant que git ne le refuse, et en nommant qui la détient (ADR-0012)
        assert_eq!(
            checkout.refused.as_deref(),
            Some(
                "feat/sidebar is checked out in ash-sidebar — a branch lives in one worktree at \
                 a time"
            )
        );
        // And — refusée, mais toujours nommée : elle n'est pas masquée
        assert_eq!(
            checkout.label,
            "Check out feat/sidebar in ash, leaving main"
        );
    }

    #[test]
    fn given_a_remote_branch_when_the_actions_are_offered_then_only_the_checkout_is_refused() {
        // Given
        let shown = on_main();

        // When
        let checkout = offer(&shown, "origin/main", BranchAction::Checkout);
        let rebase = offer(&shown, "origin/main", BranchAction::Rebase);

        // Then — checkout créerait une ref locale ; rebase n'en crée aucune
        assert!(checkout.refused.is_some());
        assert_eq!(rebase.refused, None);
        assert_eq!(rebase.label, "Rebase main onto origin/main");
    }

    #[test]
    fn given_the_current_branch_when_a_rebase_onto_itself_is_offered_then_it_is_refused() {
        // Given
        let shown = on_main();

        // When
        let rebase = offer(&shown, "main", BranchAction::Rebase);

        // Then
        assert_eq!(
            rebase.refused.as_deref(),
            Some("main is the current branch: it cannot be both sides of the same action")
        );
    }

    #[test]
    fn given_a_detached_head_when_the_actions_are_offered_then_rebase_and_merge_are_refused() {
        // Given — le worktree est en plein rebase
        let shown = overview(
            Path::new("/dev/ash"),
            "a1b2c3d\t200\t \trefs/heads/main",
            "worktree /dev/ash\nHEAD a1b2c3d\ndetached\n",
            Vec::new(),
        );

        // When
        let rebase = offer(&shown, "main", BranchAction::Rebase);
        let checkout = offer(&shown, "main", BranchAction::Checkout);

        // Then — une phrase à un seul côté est exactement ce que la spec interdit
        assert!(rebase.refused.is_some());
        assert_eq!(checkout.refused, None);
        assert_eq!(checkout.label, "Check out main in ash");
    }

    #[test]
    fn given_a_refused_action_when_it_is_run_anyway_then_git_is_never_started() {
        // Given — le frontend peut envoyer n'importe quoi ; la liste décide
        let shown = on_main();
        let git = Arc::new(FakeGit::default()) as Arc<dyn TreeWriter>;

        // When
        let done = run(&git, &shown, BranchAction::Rebase, "main");

        // Then
        assert!(!done.success);
        assert_eq!(done.label, "Rebase main onto main");
    }

    #[test]
    fn given_a_branch_name_the_repository_no_longer_holds_when_an_action_is_run_then_it_refuses_rather_than_guessing(
    ) {
        // Given
        let shown = on_main();
        let git = Arc::new(FakeGit::default());
        let writer = Arc::clone(&git) as Arc<dyn TreeWriter>;

        // When
        let done = run(
            &writer,
            &shown,
            BranchAction::Checkout,
            "--upload-pack=evil",
        );

        // Then — rien n'est passé à `git` qui ne vienne du dépôt
        assert!(!done.success);
        assert!(git.ran().is_empty());
    }

    #[test]
    fn given_an_allowed_checkout_when_it_is_run_then_the_branch_name_travels_as_an_operand() {
        // Given
        let shown = on_main();
        let git = Arc::new(FakeGit::answering(true, ""));
        let writer = Arc::clone(&git) as Arc<dyn TreeWriter>;

        // When
        let done = run(&writer, &shown, BranchAction::Checkout, "feat/popup");

        // Then — le `--` sépare les options des opérandes
        assert!(done.success);
        assert_eq!(
            git.ran(),
            vec![vec![
                "switch".to_owned(),
                "--".to_owned(),
                "feat/popup".to_owned()
            ]]
        );
    }

    #[test]
    fn given_a_rebase_that_git_refuses_when_it_fails_then_the_message_still_names_both_sides() {
        // Given
        let shown = on_main();
        let git = Arc::new(FakeGit::answering(
            false,
            "error: cannot rebase: You have unstaged changes.",
        ));
        let writer = Arc::clone(&git) as Arc<dyn TreeWriter>;

        // When
        let done = run(&writer, &shown, BranchAction::Rebase, "feat/popup");

        // Then — « y compris dans les messages d'erreur » (spec §7.1)
        assert!(!done.success);
        assert_eq!(done.label, "Rebase main onto feat/popup");
        assert!(done.output.contains("unstaged changes"));
    }

    #[test]
    fn given_a_merge_when_it_is_run_then_git_is_never_left_waiting_on_an_editor() {
        // Given
        let shown = on_main();
        let git = Arc::new(FakeGit::answering(true, "Fast-forward"));
        let writer = Arc::clone(&git) as Arc<dyn TreeWriter>;

        // When
        run(&writer, &shown, BranchAction::Merge, "feat/popup");

        // Then — sans `--no-edit`, git ouvre `$EDITOR` : un processus sans terminal ni
        // fenêtre, qui ne rend jamais la main
        assert_eq!(
            git.ran(),
            vec![vec![
                "merge".to_owned(),
                "--no-edit".to_owned(),
                "feat/popup".to_owned()
            ]]
        );
    }

    #[test]
    fn given_a_branch_free_of_any_worktree_when_it_is_offered_then_nothing_is_named_on_the_right() {
        // Given
        let shown = on_main();

        // When
        let free = branch(&shown, "feat/popup");

        // Then
        assert_eq!(free.worktree, None::<BranchWorktree>);
    }
}
