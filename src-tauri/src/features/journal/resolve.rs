//! La résolution en deux temps d'[ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md).
//!
//! C'est le cœur de la décision, et la raison pour laquelle le journal n'est pas indexé par
//! `sha` seul :
//!
//! 1. correspondance par **`sha`** ;
//! 2. à défaut — donc après un rebase, un amend ou un cherry-pick — correspondance par
//!    **(`author_date`, `subject`)**, que git préserve dans ces trois opérations.
//!
//! Le second temps est ce qui fait tenir tout le jalon J5 : le rebase est l'opération
//! centrale du produit, et c'est exactement celle qui réécrit les `sha`.

use super::commits::CommitRecord;

use super::entry::Entry;

/// Ce qu'Ash sait de ce commit, ou rien.
///
/// « Rien » n'est pas un échec : c'est un commit qu'Ash n'a pas vu naître, et la colonne `by`
/// y montre alors le nom d'auteur git, comme n'importe quel client.
pub fn attribution_of<'a>(entries: &'a [Entry], commit: &CommitRecord) -> Option<&'a Entry> {
    by_sha(entries, &commit.sha).or_else(|| by_identity(entries, commit))
}

/// Premier temps : le `sha`, qui identifie sans ambiguïté.
fn by_sha<'a>(entries: &'a [Entry], sha: &str) -> Option<&'a Entry> {
    entries.iter().rev().find(|entry| entry.sha == sha)
}

/// Second temps : ce que git préserve quand il réécrit.
///
/// **La plus récente gagne**, et c'est ce que `rev()` dit. Le cas ambigu — deux commits de
/// même sujet à la même seconde — est reconnu par l'ADR comme indiscernable ; il fallait
/// néanmoins trancher, et la dernière observation est la moins fausse : c'est celle qui
/// décrit l'état le plus récent du dépôt.
fn by_identity<'a>(entries: &'a [Entry], commit: &CommitRecord) -> Option<&'a Entry> {
    entries
        .iter()
        .rev()
        .find(|entry| entry.author_date == commit.author_date && entry.subject == commit.subject)
}

/// Ce commit est-il déjà connu du journal ?
///
/// La question que se pose l'écriture, et c'est la **même** que celle de l'affichage, à
/// dessein : un rebase produit des commits dont le `sha` est neuf mais dont l'identité est
/// déjà journalisée. Les réécrire les attribuerait à l'agent qui a lancé le rebase — donc
/// perdrait précisément ce que la correspondance de repli existe pour sauver.
pub fn already_known(entries: &[Entry], commit: &CommitRecord) -> bool {
    attribution_of(entries, commit).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::journal::fakes::EntryBuilder;

    /// Test Data Builder : un commit tel que `git log` le rend.
    fn commit(sha: &str, author_date: &str, subject: &str) -> CommitRecord {
        CommitRecord {
            sha: sha.to_owned(),
            author_date: author_date.to_owned(),
            authored_at: 1_755_000_000,
            subject: subject.to_owned(),
        }
    }

    #[test]
    fn given_a_journalled_commit_when_it_is_looked_up_by_its_sha_then_its_agent_is_named() {
        // Given
        let entries = vec![EntryBuilder::new().sha("8f3a1c2").agent("claude").build()];

        // When
        let found = attribution_of(
            &entries,
            &commit(
                "8f3a1c2",
                "2026-08-12T14:03:21+02:00",
                "feat(sidebar): group tabs by worktree",
            ),
        );

        // Then
        assert_eq!(found.map(|entry| entry.agent.as_str()), Some("claude"));
    }

    #[test]
    fn given_a_commit_whose_sha_was_rewritten_when_it_is_looked_up_then_its_date_and_subject_find_it(
    ) {
        // Given — l'état d'après un rebase : le journal parle de l'ancien `sha`, le dépôt
        // n'a plus que le nouveau. C'est le scénario qui a écarté `git notes` de l'ADR.
        let entries = vec![EntryBuilder::new().sha("8f3a1c2").agent("claude").build()];

        // When
        let found = attribution_of(
            &entries,
            &commit(
                "beefcafe",
                "2026-08-12T14:03:21+02:00",
                "feat(sidebar): group tabs by worktree",
            ),
        );

        // Then — l'attribution survit à la réécriture, ce que le `sha` seul ne permet pas
        assert_eq!(found.map(|entry| entry.agent.as_str()), Some("claude"));
    }

    #[test]
    fn given_a_commit_ash_never_saw_when_it_is_looked_up_then_nothing_is_invented() {
        // Given — un commit d'un collègue, arrivé par `git pull`
        let entries = vec![EntryBuilder::new().build()];

        // When
        let found = attribution_of(
            &entries,
            &commit("0d0d0d0", "2020-01-01T00:00:00+00:00", "chore: bump"),
        );

        // Then — pas d'orphelin, pas de devinette : la colonne montrera le nom d'auteur git
        assert!(found.is_none());
    }

    #[test]
    fn given_two_commits_with_the_same_subject_at_the_same_second_when_one_is_looked_up_then_the_latest_answers(
    ) {
        // Given — le cas que l'ADR déclare indiscernable. Ce qu'on garantit ici n'est pas
        // d'avoir raison, c'est de répondre : un nom possiblement faux dans une colonne
        // d'affichage, jamais une panne.
        let entries = vec![
            EntryBuilder::new().sha("aaa").agent("codex").build(),
            EntryBuilder::new().sha("bbb").agent("claude").build(),
        ];

        // When
        let found = attribution_of(
            &entries,
            &commit(
                "ccc",
                "2026-08-12T14:03:21+02:00",
                "feat(sidebar): group tabs by worktree",
            ),
        );

        // Then
        assert_eq!(found.map(|entry| entry.agent.as_str()), Some("claude"));
    }

    #[test]
    fn given_a_journalled_commit_when_a_rebase_replays_it_then_it_is_not_recorded_a_second_time() {
        // Given — un rebase réécrit un commit déjà journalisé. Le réécrire sous le nom de
        // l'agent qui a lancé le rebase perdrait l'attribution d'origine, c'est-à-dire
        // exactement ce que la correspondance de repli existe pour sauver.
        let entries = vec![EntryBuilder::new().sha("8f3a1c2").agent("claude").build()];
        let replayed = commit(
            "beefcafe",
            "2026-08-12T14:03:21+02:00",
            "feat(sidebar): group tabs by worktree",
        );

        // When / Then
        assert!(already_known(&entries, &replayed));
        assert!(!already_known(
            &entries,
            &commit("dead", "2026-08-13T09:00:00+02:00", "feat: autre chose")
        ));
    }

    #[test]
    fn given_a_commit_amended_into_a_new_subject_when_it_is_looked_up_then_the_old_entry_does_not_claim_it(
    ) {
        // Given — `git commit --amend -m` garde la date d'auteur et change le sujet. Les
        // deux moitiés de la clé sont exigées ensemble : la date seule attribuerait à
        // l'agent tout ce qui est écrit dans la même seconde.
        let entries = vec![EntryBuilder::new().build()];

        // When
        let found = attribution_of(
            &entries,
            &commit(
                "beefcafe",
                "2026-08-12T14:03:21+02:00",
                "feat(sidebar): un autre sujet",
            ),
        );

        // Then
        assert!(found.is_none());
    }
}
