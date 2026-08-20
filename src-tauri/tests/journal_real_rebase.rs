//! Le journal d'attribution, sur de vrais dépôts et de vrais rebases.
//!
//! Les tests unitaires prouvent les règles derrière les ports. Celui-ci prouve ce
//! qu'**aucun double ne peut prouver**, et qui est la décision centrale d'
//! [ADR-0014](../../docs/adr/0014-attribution-locale-des-commits.md) : que
//! `(author_date, subject)` est bien ce que **git** préserve quand il réécrit un `sha`.
//!
//! C'est le pari de l'ADR, et il tenait jusqu'ici à une phrase. `git notes` a été écarté
//! parce que les notes ne survivent pas au rebase ; si la clé de repli n'y survivait pas non
//! plus, tout le dispositif tomberait exactement dans le scénario que le produit met en
//! avant — un rebase, l'écran 4d de la spec. On le lance donc pour de vrai.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ash_lib::features::git::{CommitLog, CommitRecord, SystemGit};
use ash_lib::features::journal::{CommitJournal, FileJournalStore, JournalStore, TabAgent, Tabs};
use ash_lib::shared::time::SystemClock;

/// Un dossier temporaire qui se supprime à la fin du test, réussi ou non.
///
/// Il porte **le dépôt et le journal côte à côte, mais séparés** : c'est la première chose
/// que ce fichier vérifie — Ash n'écrit rien dans le dépôt de l'utilisateur.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "ash-journal-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("le bac à sable doit pouvoir être créé");
        Self {
            // Canonique : sur macOS, `$TMPDIR` passe par `/var`, qui est un lien. Le
            // journal compare des chemins de worktree à ceux des onglets.
            root: std::fs::canonicalize(&root).expect("le dossier vient d'être créé"),
        }
    }

    fn repo(&self) -> PathBuf {
        self.root.join("depot")
    }

    fn journal_dir(&self) -> PathBuf {
        self.root.join("ash-journal")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Lance `git` dans un environnement clos : ni configuration globale, ni configuration
/// système. Le test doit donner le même résultat sur toutes les machines.
fn git(cwd: &Path, args: &[&str]) {
    let done = run_git(cwd, args);
    assert!(done, "git {args:?} a échoué dans {cwd:?}");
}

fn run_git(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "user.name=Ash Test",
            "-c",
            "user.email=test@ash.local",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git doit être installé")
        .status
        .success()
}

/// Un dépôt d'un commit, prêt à recevoir une branche de fonctionnalité.
///
/// Son commit est **daté d'avant Ash**, et ce n'est pas un détail de décor : c'est
/// l'histoire que le dépôt avait déjà, et le journal ne doit rien en réclamer. Sans cette
/// date, le test décrirait un dépôt né dans la même seconde que l'application qui l'observe
/// — une situation qui n'existe pas.
fn repository(at: &Path) {
    std::fs::create_dir_all(at).expect("le dossier du dépôt doit pouvoir être créé");
    git(at, &["init", "--quiet"]);
    write(at, "README.md", "ash");
    git(at, &["add", "."]);
    git(
        at,
        &[
            "commit",
            "--quiet",
            "-m",
            "chore: init",
            "--date",
            "2020-01-01T10:00:00 +0000",
        ],
    );
}

fn write(at: &Path, file: &str, content: &str) {
    std::fs::write(at.join(file), content).expect("le dépôt est accessible en écriture");
}

/// Un commit, tel qu'un agent en produirait dans son terminal.
fn commit(at: &Path, file: &str, content: &str, subject: &str) {
    write(at, file, content);
    git(at, &["add", "."]);
    git(at, &["commit", "--quiet", "-m", subject]);
}

/// Ce que les onglets portent, et qu'on change entre deux gestes — c'est le port que le
/// composition root branche sur le registre de PTY.
#[derive(Default)]
struct OpenTabs(Mutex<Vec<TabAgent>>);

impl OpenTabs {
    fn with(worktree_root: &Path, agent: &str, tab_id: &str) -> Arc<Self> {
        let tabs = Arc::new(Self::default());
        tabs.now(worktree_root, agent, tab_id);
        tabs
    }

    fn now(&self, worktree_root: &Path, agent: &str, tab_id: &str) {
        let mut tabs = self.0.lock().expect("le test est seul sur ce verrou");
        *tabs = vec![TabAgent {
            tab_id: tab_id.to_owned(),
            worktree_root: worktree_root.display().to_string(),
            agent: Some(agent.to_owned()),
            since: 1_000,
        }];
    }
}

impl Tabs for OpenTabs {
    fn snapshot(&self) -> Vec<TabAgent> {
        self.0
            .lock()
            .map(|tabs| tabs.clone())
            .unwrap_or_else(|_| Vec::new())
    }
}

/// Le journal tel que le composition root l'assemble : le vrai `git`, un vrai dossier.
fn journal(sandbox: &Sandbox, tabs: Arc<OpenTabs>) -> Arc<CommitJournal> {
    CommitJournal::watching(
        Arc::new(SystemGit::default()),
        Arc::new(FileJournalStore::at(sandbox.journal_dir())) as Arc<dyn JournalStore>,
        tabs,
        &SystemClock,
    )
}

/// Le dossier git commun — l'identité du dépôt, telle que la sidebar la calcule.
fn repo_id(repo: &Path) -> String {
    repo.join(".git").display().to_string()
}

/// Les commits de `HEAD`, lus par le même chemin que le journal.
fn head(repo: &Path) -> Vec<CommitRecord> {
    SystemGit::default().recent(repo)
}

fn subject_of<'a>(commits: &'a [CommitRecord], subject: &str) -> &'a CommitRecord {
    commits
        .iter()
        .find(|commit| commit.subject == subject)
        .unwrap_or_else(|| panic!("le dépôt doit porter le commit « {subject} »"))
}

#[test]
fn given_commits_written_by_an_agent_when_a_real_rebase_rewrites_them_then_the_attribution_follows()
{
    // Given — `claude` écrit deux commits sur une branche, pendant que `main` avance de son
    // côté. C'est le décor du critère de sortie de J5 : un historique qui dit quel agent a
    // écrit quoi, et qui doit continuer à le dire après un rebase.
    let sandbox = Sandbox::new("rebase");
    let repo = sandbox.repo();
    repository(&repo);
    let tabs = OpenTabs::with(&repo, "claude", "01J0CLAUDE");
    let journal = journal(&sandbox, Arc::clone(&tabs));

    git(&repo, &["checkout", "--quiet", "-b", "feat/tabs"]);
    commit(&repo, "tabs.rs", "un", "feat(pty): open a tab");
    journal.on_head_moved(&repo, &repo.join(".git"));
    commit(&repo, "tabs.rs", "deux", "feat(pty): close a tab");
    journal.on_head_moved(&repo, &repo.join(".git"));

    let before: Vec<String> = head(&repo)
        .iter()
        .take(2)
        .map(|commit| commit.sha.clone())
        .collect();

    git(&repo, &["checkout", "--quiet", "main"]);
    commit(&repo, "README.md", "ash, entouré", "docs: reword");
    git(&repo, &["checkout", "--quiet", "feat/tabs"]);

    // When — le rebase, lancé par **un autre agent** dans un autre onglet. C'est le piège :
    // les deux commits réécrits ne doivent pas lui revenir.
    tabs.now(&repo, "codex", "01J0CODEX");
    git(&repo, &["rebase", "--quiet", "main"]);
    journal.on_head_moved(&repo, &repo.join(".git"));

    // Then — les `sha` ont bien changé, et l'attribution a suivi
    let after = head(&repo);
    let opened = subject_of(&after, "feat(pty): open a tab");
    let closed = subject_of(&after, "feat(pty): close a tab");
    assert!(
        !before.contains(&opened.sha) && !before.contains(&closed.sha),
        "le rebase doit avoir réécrit les deux commits, sinon le test ne prouve rien"
    );

    let repo_id = repo_id(&repo);
    assert_eq!(
        journal
            .attribution(&repo_id, opened)
            .map(|entry| entry.agent),
        Some("claude".to_owned())
    );
    assert_eq!(
        journal
            .attribution(&repo_id, closed)
            .map(|entry| entry.agent),
        Some("claude".to_owned())
    );
    // Et le commit de `main`, écrit par personne d'observé, reste sans agent : la colonne
    // `by` y montrera le nom d'auteur git.
    assert!(journal
        .attribution(&repo_id, subject_of(&after, "chore: init"))
        .is_none());
}

#[test]
fn given_a_journalled_commit_when_it_is_amended_or_cherry_picked_then_its_agent_is_still_named() {
    // Given — les deux autres opérations qu'ADR-0014 nomme à côté du rebase. Elles changent
    // le `sha` et préservent la date d'auteur ; c'est la même clé de repli qui répond.
    let sandbox = Sandbox::new("amend");
    let repo = sandbox.repo();
    repository(&repo);
    let tabs = OpenTabs::with(&repo, "claude", "01J0CLAUDE");
    let journal = journal(&sandbox, Arc::clone(&tabs));
    let repo_id = repo_id(&repo);

    commit(&repo, "probe.rs", "un", "feat(probe): follow the cwd");
    journal.on_head_moved(&repo, &repo.join(".git"));

    // When — un amend qui ne touche qu'au contenu, puis un cherry-pick sur une autre branche
    write(&repo, "probe.rs", "un, corrigé");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "--amend", "--no-edit"]);
    journal.on_head_moved(&repo, &repo.join(".git"));
    let amended = head(&repo)[0].clone();

    // Une branche qui diverge vraiment : un cherry-pick sur le **même** parent, avec le
    // même arbre et les mêmes dates, rendrait le même `sha` — et ne prouverait rien.
    git(&repo, &["checkout", "--quiet", "-b", "autre", "HEAD~1"]);
    commit(&repo, "ailleurs.rs", "un", "chore: diverge");
    journal.on_head_moved(&repo, &repo.join(".git"));
    tabs.now(&repo, "codex", "01J0CODEX");
    git(&repo, &["cherry-pick", &amended.sha]);
    journal.on_head_moved(&repo, &repo.join(".git"));
    let picked = head(&repo)[0].clone();

    // Then — trois `sha` pour un seul travail, et un seul agent nommé
    assert_ne!(amended.sha, picked.sha);
    assert_eq!(
        journal
            .attribution(&repo_id, &amended)
            .map(|entry| entry.agent),
        Some("claude".to_owned())
    );
    assert_eq!(
        journal
            .attribution(&repo_id, &picked)
            .map(|entry| entry.agent),
        Some("claude".to_owned())
    );
}

#[test]
fn given_two_commits_with_the_same_subject_in_the_same_second_when_they_are_looked_up_then_ash_still_answers(
) {
    // Given — le cas qu'ADR-0014 déclare indiscernable, monté pour de vrai : deux commits de
    // même sujet, dont on force la date d'auteur à la même seconde. Ce qu'on vérifie n'est
    // pas d'avoir raison — l'ADR dit qu'on ne peut pas — mais que la conséquence reste
    // bénigne : un nom d'agent, jamais un plantage ni une disparition.
    let sandbox = Sandbox::new("ambigu");
    let repo = sandbox.repo();
    repository(&repo);
    let tabs = OpenTabs::with(&repo, "claude", "01J0CLAUDE");
    let journal = journal(&sandbox, Arc::clone(&tabs));
    let repo_id = repo_id(&repo);
    let same_second = format!("{} +0000", now_in_seconds());

    for content in ["un", "deux"] {
        write(&repo, "notes.md", content);
        git(&repo, &["add", "."]);
        git(
            &repo,
            &[
                "commit",
                "--quiet",
                "-m",
                "chore: touch",
                "--date",
                &same_second,
            ],
        );
        journal.on_head_moved(&repo, &repo.join(".git"));
    }

    // When — on demande l'attribution des deux
    let commits = head(&repo);
    let attributed: Vec<Option<String>> = commits
        .iter()
        .filter(|commit| commit.subject == "chore: touch")
        .map(|commit| journal.attribution(&repo_id, commit).map(|e| e.agent))
        .collect();

    // Then — les deux sont attribués, et à l'agent qui était là
    assert_eq!(attributed.len(), 2);
    assert!(attributed
        .iter()
        .all(|agent| agent.as_deref() == Some("claude")));
}

#[test]
fn given_a_repository_whose_commits_are_journalled_when_ash_has_written_then_the_repository_is_untouched(
) {
    // Given — la promesse d'ADR-0014, celle qui a écarté `git notes`, le trailer et le hook
    // `prepare-commit-msg` : **rien n'est écrit dans le dépôt de l'utilisateur**. Ce test est
    // une garantie de non-écriture, et il tombera le jour où quelqu'un décidera qu'une petite
    // note dans `.git` serait plus commode.
    let sandbox = Sandbox::new("empreinte");
    let repo = sandbox.repo();
    repository(&repo);
    let tabs = OpenTabs::with(&repo, "claude", "01J0CLAUDE");
    let journal = journal(&sandbox, tabs);

    commit(&repo, "menu.rs", "un", "feat(menu): native menu");
    let before = tracked_files(&repo);

    // When
    journal.on_head_moved(&repo, &repo.join(".git"));

    // Then — l'arbre de travail est propre, aucun fichier n'est apparu, et ce qu'Ash a écrit
    // est ailleurs
    assert!(
        status(&repo).is_empty(),
        "l'arbre de travail doit rester propre : {}",
        status(&repo)
    );
    assert_eq!(tracked_files(&repo), before);
    assert!(!repo.join(".git/refs/notes").exists());
    assert!(!repo.join(".git/hooks/prepare-commit-msg").exists());
    assert_eq!(journal.summary().entries, 1);
    assert!(
        sandbox.journal_dir().exists(),
        "le journal vit sous ~/.ash/"
    );
}

/// Ce que `git status --porcelain` répond — vide veut dire « rien à signaler ».
fn status(repo: &Path) -> String {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["status", "--porcelain"])
        .output()
        .expect("git doit être installé");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn tracked_files(repo: &Path) -> String {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["ls-files"])
        .output()
        .expect("git doit être installé");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn now_in_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}
