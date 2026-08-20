//! La rédaction du prompt de conflit — une **règle pure**, réutilisable.
//!
//! Ash a trois choses que personne d'autre n'a au moment où un rebase s'arrête : les
//! chemins, le commit d'arrêt et la commande de test. Composer ce prompt à la main prend
//! une minute et on oublie toujours l'un des trois
//! ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
//!
//! Une fonction pure : un état entre, un texte sort. Ni PTY, ni onglet, ni disque — c'est
//! ce qui la rend testable sans lancer quoi que ce soit, et **réutilisable** par l'onglet
//! de merge (#30), qui compose le même prompt sur les seuls conflits qu'il n'a pas résolus.
//!
//! # Deux invariants, et ils ne sont pas cosmétiques
//!
//! **Une seule ligne.** Dans un PTY, `\n` *est* la touche `⏎`. Un prompt de trois lignes
//! serait donc trois validations — exactement ce qu'ADR-0015 interdit à Ash. La fonction
//! ne rend jamais de saut de ligne, et [`compose_conflict_prompt`] le garantit sur *toute*
//! entrée, y compris un sujet de commit multiligne.
//!
//! **Aucun caractère de contrôle.** Les chemins viennent du dépôt visité, qu'Ash n'a pas
//! choisi. Un `ESC` glissé dans un nom de fichier repeindrait le terminal, et le texte
//! affiché ne serait plus le texte envoyé — la première condition d'ADR-0015 tomberait.
//! [`super::porcelain`] laisse déjà git les échapper (`core.quotePath=true`) ; ce filtre-ci
//! est la seconde barrière, parce que cette fonction est publique et que #30 l'appellera
//! avec ce qu'il voudra.

use super::metadata::{Operation, OperationKind};
use super::stopped::{StoppedCommit, StoppedOperation};

/// Combien de chemins le prompt nomme au plus.
///
/// Au-delà, il dit combien il en reste plutôt que de les aligner : un prompt de dix mille
/// caractères se relit mal, et ADR-0015 demande que l'utilisateur le **lise** avant de
/// l'envoyer.
const MAX_NAMED_PATHS: usize = 20;

/// Ce sur quoi le prompt porte.
///
/// Volontairement plus pauvre que [`StoppedOperation`] : l'onglet de merge (#30) compose
/// le même prompt sur un **sous-ensemble** des chemins — ceux qu'il n'a pas résolus — et
/// n'aurait rien à faire d'un état complet qu'il devrait d'abord amputer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSubject<'a> {
    pub operation: &'a Operation,
    /// Les chemins à nommer. Peut être un sous-ensemble de ce que le dépôt porte.
    pub paths: &'a [String],
    /// Combien il y en a en tout, quand on le sait et qu'il y en a plus que de nommés.
    pub total: Option<u32>,
    pub stopped_at: Option<&'a StoppedCommit>,
    pub test_command: Option<&'a str>,
}

impl StoppedOperation {
    /// Le sujet que ce rebase arrêté donne au prompt — tous ses conflits.
    pub fn prompt_subject(&self) -> PromptSubject<'_> {
        PromptSubject {
            operation: &self.operation,
            paths: &self.conflicts,
            total: self.conflicted_total,
            stopped_at: self.stopped_at.as_ref(),
            test_command: self.test_command.as_deref(),
        }
    }
}

/// Rédige le prompt. **Une seule ligne**, jamais envoyée.
pub fn compose_conflict_prompt(subject: &PromptSubject<'_>) -> String {
    let mut sentences = vec![opening(subject)];

    if !subject.paths.is_empty() {
        sentences.push(conflicting_files(subject));
    }
    sentences.push(request(subject));
    // La dernière phrase n'est pas une politesse : c'est la règle du produit, écrite là où
    // l'agent la lira. Ash ne valide rien à la place de l'utilisateur, et il ne demande
    // pas non plus à un agent de le faire pour lui.
    sentences.push(format!(
        "Do not run {} yourself — I will.",
        continuation_of(subject.operation.kind)
    ));

    single_line(&sentences.join(" "))
}

fn opening(subject: &PromptSubject<'_>) -> String {
    let mut opening = describe(subject.operation);
    opening.push_str(" stopped");
    if let Some(progress) = subject.operation.progress {
        opening.push_str(&format!(" at step {}/{}", progress.step, progress.total));
    }
    if let Some(stopped) = subject.stopped_at {
        opening.push_str(&format!(" on commit {}", stopped.commit));
        if let Some(subject) = &stopped.subject {
            opening.push_str(&format!(" ({subject})"));
        }
    }
    opening.push('.');
    opening
}

/// `The rebase of feat onto main` — la même formulation que la ligne de statut, sans les
/// abréviations qu'un humain lit mais qu'un agent aurait à deviner.
fn describe(operation: &Operation) -> String {
    let verb = match operation.kind {
        OperationKind::Rebase => "rebase",
        OperationKind::Am => "patch application",
        OperationKind::Merge => "merge",
    };
    let mut described = format!("The {verb}");
    if let Some(branch) = &operation.branch {
        described.push_str(&format!(" of {branch}"));
    }
    if let Some(onto) = &operation.onto {
        // « merging onto feat » inverserait le sens : un merge amène `onto` **dans** la
        // branche courante, un rebase déplace la branche courante **sur** `onto`.
        let preposition = match operation.kind {
            OperationKind::Merge => "of",
            _ => "onto",
        };
        described.push_str(&format!(" {preposition} {onto}"));
    }
    described
}

fn conflicting_files(subject: &PromptSubject<'_>) -> String {
    let named: Vec<&str> = subject
        .paths
        .iter()
        .take(MAX_NAMED_PATHS)
        .map(String::as_str)
        .collect();
    let mut listed = format!("These files are in conflict: {}", named.join(", "));

    let total = subject.total.unwrap_or(subject.paths.len() as u32) as usize;
    if total > named.len() {
        listed.push_str(&format!(" (and {} more)", total - named.len()));
    }
    listed.push('.');
    listed
}

fn request(subject: &PromptSubject<'_>) -> String {
    let mut asked = if subject.paths.is_empty() {
        "Please finish what this step needs".to_owned()
    } else {
        "Please resolve the conflicts and stage the result".to_owned()
    };
    // Rien n'est ajouté quand rien ne nomme la commande : un prompt qui invente
    // `npm test` sur un dépôt qui n'en a pas coûte l'aller-retour qu'il devait éviter
    // (voir [`super::test_command`]).
    if let Some(command) = subject.test_command {
        asked.push_str(&format!(", then run: {command}"));
    }
    asked.push('.');
    asked
}

/// Ce que l'utilisateur tapera lui-même, et qu'on demande à l'agent de ne pas taper.
fn continuation_of(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Rebase => "git rebase --continue",
        OperationKind::Am => "git am --continue",
        OperationKind::Merge => "git commit",
    }
}

/// Une ligne, et rien qu'une ligne — voir les invariants en tête de module.
///
/// Les caractères de contrôle sont **remplacés par une espace** plutôt que supprimés : un
/// `a\u{7}b` recollé en `ab` fabriquerait un chemin qui n'existe pas, alors qu'un `a b`
/// se voit.
fn single_line(text: &str) -> String {
    let flattened: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    // Les espaces multiples viennent d'un champ vide ou d'un contrôle remplacé : les
    // recoller garde le prompt lisible, ce qu'ADR-0015 exige puisqu'il est fait pour être
    // relu avant d'être envoyé.
    flattened.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::metadata::Progress;

    /// Test Data Builder : un rebase arrêté, tel que le panneau des conflits le voit.
    ///
    /// Défauts valides et déterministes — `feat` sur `main`, deux commits sur cinq, deux
    /// chemins, un commit d'arrêt, `cargo test`.
    struct SubjectBuilder {
        operation: Operation,
        paths: Vec<String>,
        total: Option<u32>,
        stopped_at: Option<StoppedCommit>,
        test_command: Option<String>,
    }

    impl SubjectBuilder {
        fn new() -> Self {
            Self {
                operation: Operation {
                    kind: OperationKind::Rebase,
                    branch: Some("feat".to_owned()),
                    onto: Some("main".to_owned()),
                    progress: Some(Progress { step: 2, total: 5 }),
                },
                paths: vec!["src/probe.rs".to_owned(), "src/app/main.ts".to_owned()],
                total: Some(2),
                stopped_at: Some(StoppedCommit {
                    commit: "1a2b3c4".to_owned(),
                    subject: Some("add the probe".to_owned()),
                }),
                test_command: Some("cargo test".to_owned()),
            }
        }

        fn paths(mut self, paths: &[&str]) -> Self {
            self.paths = paths.iter().map(|path| (*path).to_owned()).collect();
            self.total = Some(self.paths.len() as u32);
            self
        }

        fn total(mut self, total: u32) -> Self {
            self.total = Some(total);
            self
        }

        fn stopped_at(mut self, stopped: Option<StoppedCommit>) -> Self {
            self.stopped_at = stopped;
            self
        }

        fn merging(mut self) -> Self {
            self.operation = Operation {
                kind: OperationKind::Merge,
                branch: None,
                onto: Some("feat".to_owned()),
                progress: None,
            };
            self.stopped_at = None;
            self
        }

        fn without_test_command(mut self) -> Self {
            self.test_command = None;
            self
        }

        fn compose(&self) -> String {
            compose_conflict_prompt(&PromptSubject {
                operation: &self.operation,
                paths: &self.paths,
                total: self.total,
                stopped_at: self.stopped_at.as_ref(),
                test_command: self.test_command.as_deref(),
            })
        }
    }

    #[test]
    fn given_a_rebase_stopped_on_two_files_when_composing_the_prompt_then_it_carries_the_paths_the_stopped_commit_and_the_test_command(
    ) {
        // Given — les trois choses qu'on oublie, et qui coûtent un aller-retour chacune
        // (ADR-0015)
        let subject = SubjectBuilder::new();

        // When
        let prompt = subject.compose();

        // Then
        assert_eq!(
            prompt,
            "The rebase of feat onto main stopped at step 2/5 on commit 1a2b3c4 \
             (add the probe). These files are in conflict: src/probe.rs, src/app/main.ts. \
             Please resolve the conflicts and stage the result, then run: cargo test. \
             Do not run git rebase --continue yourself — I will."
        );
    }

    #[test]
    fn given_any_stopped_operation_when_composing_the_prompt_then_it_never_contains_a_newline() {
        // Given — dans un PTY, `\n` **est** la touche `⏎` : un prompt de trois lignes
        // serait trois validations, et Ash n'en presse aucune (ADR-0015)
        let subject = SubjectBuilder::new()
            .paths(&["a.rs", "b.rs"])
            .stopped_at(Some(StoppedCommit {
                commit: "1a2b3c4".to_owned(),
                // Un sujet de commit qu'un `optional_line` distrait laisserait passer entier
                subject: Some("first line\nsecond line\r\nthird".to_owned()),
            }));

        // When
        let prompt = subject.compose();

        // Then
        assert!(!prompt.contains('\n'), "{prompt}");
        assert!(!prompt.contains('\r'), "{prompt}");
        assert!(prompt.contains("first line second line third"), "{prompt}");
    }

    #[test]
    fn given_a_path_carrying_an_escape_sequence_when_composing_the_prompt_then_no_control_character_survives(
    ) {
        // Given — les chemins viennent d'un dépôt qu'Ash n'a pas choisi. Un `ESC` qui
        // atteint le terminal repeint l'écran : le texte affiché ne serait plus le texte
        // envoyé, et la première condition d'ADR-0015 tomberait.
        let subject = SubjectBuilder::new().paths(&["src/\u{1b}[2Jgotcha.rs"]);

        // When
        let prompt = subject.compose();

        // Then
        assert!(
            !prompt.chars().any(char::is_control),
            "un caractère de contrôle a traversé : {prompt:?}"
        );
    }

    #[test]
    fn given_a_worktree_that_names_no_test_command_when_composing_the_prompt_then_it_stays_silent_about_tests(
    ) {
        // Given — nommer `npm test` sur un dépôt qui n'en a pas coûte l'aller-retour que
        // le prompt devait éviter (voir `test_command`)
        let subject = SubjectBuilder::new().without_test_command();

        // When
        let prompt = subject.compose();

        // Then
        assert!(!prompt.contains("run:"), "{prompt}");
        assert!(prompt.contains("Please resolve the conflicts"), "{prompt}");
    }

    #[test]
    fn given_a_conflict_on_a_whole_vendored_tree_when_composing_the_prompt_then_it_counts_the_rest_instead_of_listing_it(
    ) {
        // Given — ADR-0015 demande que l'utilisateur **lise** le prompt avant de l'envoyer
        let paths: Vec<String> = (0..40).map(|index| format!("vendor/f{index}.rs")).collect();
        let borrowed: Vec<&str> = paths.iter().map(String::as_str).collect();
        let subject = SubjectBuilder::new().paths(&borrowed).total(3_000);

        // When
        let prompt = subject.compose();

        // Then — le compte reste celui du dépôt, pas celui de la liste tronquée
        assert!(prompt.contains("vendor/f19.rs"), "{prompt}");
        assert!(!prompt.contains("vendor/f20.rs"), "{prompt}");
        assert!(prompt.contains("(and 2980 more)"), "{prompt}");
    }

    #[test]
    fn given_a_stopped_merge_when_composing_the_prompt_then_it_says_merge_of_and_names_the_right_continuation(
    ) {
        // Given — « merging onto feat » inverserait le sens de l'opération, et
        // `git merge --continue` n'est pas ce qui termine un merge résolu
        let subject = SubjectBuilder::new().merging();

        // When
        let prompt = subject.compose();

        // Then
        assert!(prompt.starts_with("The merge of feat stopped."), "{prompt}");
        assert!(
            prompt.ends_with("Do not run git commit yourself — I will."),
            "{prompt}"
        );
    }

    #[test]
    fn given_an_interactive_rebase_stopped_with_nothing_in_conflict_when_composing_then_it_does_not_claim_there_are_conflicts(
    ) {
        // Given — un `edit` ou un `break` arrête le rebase sans le moindre conflit
        let subject = SubjectBuilder::new().paths(&[]);

        // When
        let prompt = subject.compose();

        // Then
        assert!(!prompt.contains("in conflict"), "{prompt}");
        assert!(
            prompt.contains("Please finish what this step needs"),
            "{prompt}"
        );
    }
}
