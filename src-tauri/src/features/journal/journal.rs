//! Le journal lui-même : ce qu'il écrit quand un `HEAD` bouge, et ce qu'il relit.

use std::path::Path;
use std::sync::Arc;

use crate::shared::time::{Clock, UnixMillis};

use super::commits::{CommitLog, CommitRecord};
use super::entry::{file_name, Entry};
use super::error::JournalError;
use super::resolve::{already_known, attribution_of};
use super::store::JournalStore;
use super::tabs::{author_of, Tabs};

/// Ce que le journal pèse — de quoi proposer sa purge en sachant ce qu'elle emporte.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JournalSummary {
    pub entries: usize,
    pub repos: usize,
}

/// L'attribution locale des commits (ADR-0014).
///
/// Il écrit **au moment où un commit naît**, parce que c'est le seul moment où l'on sait qui
/// l'a écrit : la sonde dit quel outil tient l'avant-plan de quel onglet, et cette
/// information n'existe plus une minute plus tard.
pub struct CommitJournal {
    commits: Arc<dyn CommitLog>,
    store: Arc<dyn JournalStore>,
    tabs: Arc<dyn Tabs>,
    /// Depuis quand Ash regarde.
    ///
    /// **La borne qui empêche le journal de mentir.** Un mouvement de `HEAD` n'est pas
    /// toujours un commit — un `checkout`, un `reset`, un `pull` en produisent aussi — et la
    /// lecture qui suit rend les cinquante derniers commits de la branche, dont l'écrasante
    /// majorité est plus vieille qu'Ash. Les attribuer à l'agent présent serait inventer une
    /// observation ; ils sont donc écartés sur leur date d'auteur, qu'Ash n'a pas pu voir
    /// naître.
    ///
    /// C'est aussi ce qui rend inoffensif le premier `git log` sur un dépôt vieux de dix ans.
    watching_since: UnixMillis,
}

impl CommitJournal {
    /// Le journal, à partir de maintenant.
    ///
    /// L'horloge est lue **une fois**, ici : c'est le moment où Ash commence à regarder, et
    /// il ne se rejoue pas. Le port reste injecté pour que la borne soit descriptible dans un
    /// test au lieu d'y être subie.
    pub fn watching(
        commits: Arc<dyn CommitLog>,
        store: Arc<dyn JournalStore>,
        tabs: Arc<dyn Tabs>,
        clock: &dyn Clock,
    ) -> Arc<Self> {
        Arc::new(Self {
            commits,
            store,
            tabs,
            watching_since: clock.wall(),
        })
    }

    /// Le `HEAD` d'un worktree a bougé : un commit a pu y naître.
    ///
    /// Le seul chemin d'écriture du journal. Ce qu'il ne fait **jamais** :
    ///
    /// - écrire quoi que ce soit dans le dépôt de l'utilisateur — il lit `git log`, et pose
    ///   ses lignes sous `~/.ash/` ;
    /// - attribuer un commit qu'aucun agent reconnu n'a pu écrire ;
    /// - réécrire un commit déjà connu, fût-ce sous un `sha` neuf : c'est ce qui laisse un
    ///   rebase garder l'attribution d'origine.
    pub fn on_head_moved(&self, worktree_root: &Path, common_dir: &Path) {
        let repo = common_dir.to_string_lossy().into_owned();
        let file = file_name(&repo);
        let mut known = Entry::read_all(&self.store.read(&file));

        // Du plus ancien au plus récent : le journal se lit dans l'ordre où les commits sont
        // nés, et c'est ce qui donne son sens au « la plus récente gagne » de la résolution.
        let born = self.commits.recent(worktree_root);
        // **Une observation par mouvement de `HEAD`**, et non une par commit. Ce que le
        // journal enregistre est ce qu'Ash a vu à l'instant où `HEAD` a bougé ; relire les
        // onglets entre deux lignes d'une même rafale ferait dépendre l'attribution de la
        // durée d'un `git log`, et un `rebase` de dix commits pourrait en attribuer trois à
        // l'agent qui les a écrits et sept à celui qui a pris l'avant-plan entre-temps.
        let tabs = self.tabs.snapshot();
        for commit in born.iter().rev() {
            if !self.observed(commit) || already_known(&known, commit) {
                continue;
            }
            let Some(author) = author_of(worktree_root, &tabs) else {
                // Aucun agent reconnu : git a déjà un nom d'auteur pour ce commit, et Ash
                // n'a rien à ajouter (ADR-0014).
                continue;
            };
            let entry = Entry {
                repo: repo.clone(),
                sha: commit.sha.clone(),
                author_date: commit.author_date.clone(),
                subject: commit.subject.clone(),
                agent: author.agent.clone().unwrap_or_default(),
                tab_id: author.tab_id.clone(),
                // Les deux champs sans source — voir `mod.rs`.
                session_started: None,
                prompt: None,
            };
            // Échouer à écrire ne fait rien échouer d'autre : le journal est un confort
            // d'affichage, et un `~/.ash` en lecture seule ne doit coûter ni le terminal, ni
            // git, ni un message dans une boucle de surveillance.
            if self.store.append(&file, &entry.line()).is_ok() {
                // Retenue en mémoire aussi : une même rafale peut porter deux commits que le
                // fichier ne connaît pas encore, et le second doit voir le premier.
                known.push(entry);
            }
        }
    }

    /// Ce qu'Ash sait de ce commit — la lecture qu'attend la colonne `by` du graphe (#27).
    ///
    /// Elle prend le dépôt et un commit tel que git le décrit, et rend l'entrée du journal.
    /// Les deux temps de la résolution sont dans [`attribution_of`] ; ici il n'y a que la
    /// lecture du fichier.
    pub fn attribution(&self, repo: &str, commit: &CommitRecord) -> Option<Entry> {
        let entries = Entry::read_all(&self.store.read(&file_name(repo)));
        attribution_of(&entries, commit).cloned()
    }

    /// Ce que le journal pèse aujourd'hui.
    pub fn summary(&self) -> JournalSummary {
        let files = self.store.files();
        JournalSummary {
            entries: files
                .iter()
                .map(|file| Entry::read_all(&self.store.read(file)).len())
                .sum(),
            repos: files.len(),
        }
    }

    /// Efface tout (spec §10), et rend ce qu'il en reste.
    ///
    /// Le compte rendu est **relu après coup** plutôt que déduit de ce qui a été supprimé :
    /// c'est la seule façon de dire ce qui reste vraiment, y compris quand un fichier a
    /// résisté.
    pub fn purge(&self) -> Result<JournalSummary, JournalError> {
        self.store.purge()?;
        Ok(self.summary())
    }

    /// Ce commit est-il né sous les yeux d'Ash ?
    fn observed(&self, commit: &CommitRecord) -> bool {
        commit.authored_at >= self.watching_since / 1_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::journal::fakes::{FakeCommits, FakeTabs, MemoryJournal};
    use crate::shared::time::UnixMillis;

    const REPO: &str = "/dev/ash/.git";
    const WORKTREE: &str = "/dev/ash";
    /// Ash démarre : tout ce qui est plus vieux appartient à l'histoire d'avant.
    const STARTED: UnixMillis = 1_755_000_000_000;

    /// Test Data Builder : un journal branché sur des doubles, avec un agent au travail.
    struct JournalBuilder {
        commits: Arc<FakeCommits>,
        store: Arc<MemoryJournal>,
        tabs: Arc<FakeTabs>,
    }

    impl JournalBuilder {
        fn new() -> Self {
            Self {
                commits: FakeCommits::new(),
                store: MemoryJournal::new(),
                tabs: FakeTabs::with_agent(WORKTREE, "claude", "01J0TAB"),
            }
        }

        fn build(&self) -> Arc<CommitJournal> {
            CommitJournal::watching(
                Arc::clone(&self.commits) as Arc<dyn CommitLog>,
                Arc::clone(&self.store) as Arc<dyn JournalStore>,
                Arc::clone(&self.tabs) as Arc<dyn Tabs>,
                &crate::features::journal::fakes::FrozenClock(STARTED),
            )
        }

        fn head_moved(&self, journal: &Arc<CommitJournal>) {
            journal.on_head_moved(Path::new(WORKTREE), Path::new(REPO));
        }

        fn journalled(&self) -> Vec<Entry> {
            Entry::read_all(&self.store.read(&file_name(REPO)))
        }
    }

    /// Un commit né après le démarrage d'Ash.
    fn fresh(sha: &str, subject: &str) -> CommitRecord {
        CommitRecord {
            sha: sha.to_owned(),
            author_date: "2026-08-12T14:03:21+02:00".to_owned(),
            authored_at: STARTED / 1_000 + 60,
            subject: subject.to_owned(),
        }
    }

    #[test]
    fn given_an_agent_at_work_when_it_commits_then_the_commit_is_journalled_with_its_name() {
        // Given
        let world = JournalBuilder::new();
        let journal = world.build();
        world.commits.set(vec![fresh("8f3a1c2", "feat: onglets")]);

        // When — la surveillance de `.git/logs/HEAD` a parlé
        world.head_moved(&journal);

        // Then — c'est la ligne dont la colonne `by` du graphe se servira
        let entries = world.journalled();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].sha, "8f3a1c2");
        assert_eq!(entries[0].agent, "claude");
        assert_eq!(entries[0].tab_id, "01J0TAB");
        assert_eq!(entries[0].repo, REPO);
        assert_eq!(entries[0].subject, "feat: onglets");
    }

    #[test]
    fn given_a_repository_older_than_ash_when_its_head_moves_then_its_history_is_not_claimed() {
        // Given — un `git checkout` déplace `HEAD` sans rien créer, et la lecture qui suit
        // rend les cinquante derniers commits de la branche. Les attribuer à l'agent présent
        // inventerait une observation qui n'a jamais eu lieu — et c'est ce qui arriverait dès
        // le premier changement de branche dans un dépôt de dix ans.
        let world = JournalBuilder::new();
        let journal = world.build();
        world.commits.set(vec![CommitRecord {
            sha: "01d".to_owned(),
            author_date: "2019-01-01T10:00:00+01:00".to_owned(),
            authored_at: 1_546_333_200,
            subject: "chore: initial import".to_owned(),
        }]);

        // When
        world.head_moved(&journal);

        // Then
        assert!(world.journalled().is_empty());
    }

    #[test]
    fn given_a_commit_typed_by_hand_in_a_shell_when_it_is_born_then_nothing_is_written() {
        // Given — aucun outil reconnu dans l'onglet. ADR-0014 : la colonne `by` ne montre un
        // nom d'agent que quand Ash l'a réellement observé.
        let world = JournalBuilder::new();
        world.tabs.set_shell(WORKTREE);
        let journal = world.build();
        world.commits.set(vec![fresh("8f3a1c2", "fix: à la main")]);

        // When
        world.head_moved(&journal);

        // Then
        assert!(world.journalled().is_empty());
    }

    #[test]
    fn given_a_journalled_commit_when_the_head_moves_again_then_it_is_not_recorded_twice() {
        // Given — le reflog bouge à chaque `checkout`, `pull` ou `reset`, et la lecture rend
        // toujours les mêmes commits. Sans cette garde, le journal grossirait à chaque
        // changement de branche.
        let world = JournalBuilder::new();
        let journal = world.build();
        world.commits.set(vec![fresh("8f3a1c2", "feat: onglets")]);
        world.head_moved(&journal);

        // When
        world.head_moved(&journal);
        world.head_moved(&journal);

        // Then
        assert_eq!(world.journalled().len(), 1);
    }

    #[test]
    fn given_a_rebase_that_rewrites_a_journalled_commit_when_it_lands_then_the_first_agent_keeps_it(
    ) {
        // Given — `claude` a écrit le commit ; c'est `codex` qui rebase, dans un autre
        // onglet. Le `sha` change, la date d'auteur et le sujet non.
        let world = JournalBuilder::new();
        let journal = world.build();
        world.commits.set(vec![fresh("8f3a1c2", "feat: onglets")]);
        world.head_moved(&journal);

        // When — le rebase, et l'agent qui l'a lancé
        world.tabs.set_agent(WORKTREE, "codex", "01J0OTHER");
        world.commits.set(vec![fresh("beefcafe", "feat: onglets")]);
        world.head_moved(&journal);

        // Then — une seule ligne, et c'est toujours `claude` qui a écrit ce commit
        let entries = world.journalled();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].agent, "claude");
        // Et la lecture le retrouve sous son nouveau `sha`, qui n'a jamais été écrit
        let found = journal.attribution(REPO, &fresh("beefcafe", "feat: onglets"));
        assert_eq!(found.map(|entry| entry.agent), Some("claude".to_owned()));
    }

    #[test]
    fn given_several_commits_born_at_once_when_they_are_read_then_all_of_them_are_journalled() {
        // Given — un agent qui commite trois fois pendant qu'Ash était occupé ailleurs. La
        // lecture rend le plus récent en premier ; le journal doit se lire dans l'ordre où
        // les commits sont nés.
        let world = JournalBuilder::new();
        let journal = world.build();
        world.commits.set(vec![
            fresh("ccc", "trois"),
            fresh("bbb", "deux"),
            fresh("aaa", "un"),
        ]);

        // When
        world.head_moved(&journal);

        // Then
        let subjects: Vec<String> = world
            .journalled()
            .into_iter()
            .map(|entry| entry.subject)
            .collect();
        assert_eq!(subjects, vec!["un", "deux", "trois"]);
    }

    #[test]
    fn given_a_journal_with_entries_when_it_is_purged_then_nothing_is_left_to_read() {
        // Given — le journal contient des prompts (spec §10). Ce qui est promis n'est pas
        // qu'on puisse l'effacer, c'est qu'après l'avoir effacé il n'en reste rien.
        let world = JournalBuilder::new();
        let journal = world.build();
        world.commits.set(vec![fresh("8f3a1c2", "feat: onglets")]);
        world.head_moved(&journal);
        assert_eq!(journal.summary().entries, 1);

        // When
        let after = journal.purge().expect("le dossier appartient à Ash");

        // Then
        assert_eq!(after, JournalSummary::default());
        assert!(journal
            .attribution(REPO, &fresh("8f3a1c2", "feat: onglets"))
            .is_none());
    }
}
