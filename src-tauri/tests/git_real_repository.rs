//! Tests d'intégration de la lecture d'un dépôt — sur de vrais dépôts.
//!
//! Les tests unitaires vérifient les règles derrière le trait `FileSystem`. Ceux-ci
//! vérifient ce qu'aucun double ne peut prouver : que ce qu'Ash lit est bien ce que
//! **git écrit** — le `.git` fichier d'un worktree lié, son `gitdir:`, son `commondir`,
//! et les fichiers qu'un rebase laisse derrière lui pendant qu'il est arrêté.
//!
//! Ils lancent `git`, jamais pour résoudre ni pour lire quoi que ce soit : uniquement
//! pour fabriquer le décor. Ash, lui, ne lit que des fichiers.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use ash_lib::features::git::{
    read_metadata, resolve_worktree, GitError, Head, OperationKind, Progress, SystemFileSystem,
    WorktreeLocation, WorktreeMetadata,
};

/// Un dossier temporaire qui se supprime à la fin du test, réussi ou non.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("ash-git-{label}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("le bac à sable doit pouvoir être créé");
        Self { root }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// Le chemin réel — sur macOS, `$TMPDIR` passe par `/var`, qui est un lien
    /// symbolique. Comparer sans canonicaliser ferait échouer tous les tests de ce
    /// fichier, pour une raison qui n'a rien à voir avec git.
    fn real(&self, relative: &str) -> PathBuf {
        std::fs::canonicalize(self.path(relative)).expect("le chemin attendu doit exister")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Lance `git` dans un environnement clos : ni configuration globale, ni configuration
/// système, ni identité de la machine. Le test doit donner le même résultat partout.
fn git(cwd: &Path, args: &[&str]) {
    assert!(run_git(cwd, args), "git {args:?} a échoué dans {cwd:?}");
}

/// La même chose, pour les commandes dont l'échec **est** le décor : un rebase arrêté sur
/// conflit sort en erreur, et c'est exactement l'état qu'on veut lire.
fn git_may_fail(cwd: &Path, args: &[&str]) {
    let _ = run_git(cwd, args);
}

fn run_git(cwd: &Path, args: &[&str]) -> bool {
    let status = Command::new("git")
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
        .status()
        .expect("git doit être installé");
    status.success()
}

/// Un dépôt avec un commit — `git worktree add` refuse une `HEAD` non née.
fn repository_at(path: &Path) {
    std::fs::create_dir_all(path).expect("le dossier du dépôt doit pouvoir être créé");
    git(path, &["init", "--quiet"]);
    git(path, &["commit", "--allow-empty", "--quiet", "-m", "init"]);
}

fn resolve(cwd: &Path) -> Result<WorktreeLocation, GitError> {
    resolve_worktree(&SystemFileSystem, cwd)
}

fn resolved(cwd: &Path) -> WorktreeLocation {
    resolve(cwd).expect("la résolution doit aboutir")
}

/// Les métadonnées d'un worktree, lues comme la surveillance les lit : on résout d'abord,
/// on lit ensuite dans les deux dossiers que la résolution a rendus.
fn metadata_of(worktree_root: &Path) -> WorktreeMetadata {
    let (git_dir, common_dir) = resolved(worktree_root)
        .git_dirs()
        .expect("un worktree dans un dépôt a un dossier git");
    read_metadata(&SystemFileSystem, &git_dir, &common_dir).expect("le dépôt doit être lisible")
}

/// Écrit un fichier et le commite — de quoi fabriquer une divergence, donc un conflit.
fn commit(worktree: &Path, content: &str) {
    std::fs::write(worktree.join("f.txt"), content).expect("le fichier doit pouvoir être écrit");
    git(worktree, &["add", "f.txt"]);
    git(worktree, &["commit", "--quiet", "-m", content]);
}

/// Un dépôt, un worktree lié sur `feat`, et un rebase arrêté sur conflit à la première
/// des deux étapes.
fn repository_with_a_conflicting_rebase(sandbox: &Sandbox) -> PathBuf {
    let main = sandbox.path("ash");
    repository_at(&main);
    commit(&main, "base");
    git(
        &main,
        &["worktree", "add", "--quiet", "../ash-feat", "-b", "feat"],
    );

    let worktree = sandbox.path("ash-feat");
    commit(&worktree, "feat-un");
    commit(&worktree, "feat-deux");
    commit(&main, "main-bouge");
    worktree
}

#[test]
fn given_a_real_rebase_stopped_on_a_conflict_when_reading_the_metadata_then_it_reports_the_branch_the_target_and_the_step(
) {
    // Given — l'état que la sidebar doit rendre `rebasing onto main · 1/2`. Aucun double
    // ne prouve que git écrit bien `msgnum`, `end`, `head-name` et un `onto` qui est un
    // identifiant de commit et non un nom.
    let sandbox = Sandbox::new("rebase");
    let worktree = repository_with_a_conflicting_rebase(&sandbox);
    git_may_fail(&worktree, &["rebase", "main"]);

    // When
    let metadata = metadata_of(&worktree);

    // Then
    let operation = metadata
        .operation
        .expect("un rebase arrêté sur conflit est une opération en cours");
    assert_eq!(operation.kind, OperationKind::Rebase);
    assert_eq!(operation.branch.as_deref(), Some("feat"));
    assert_eq!(operation.onto.as_deref(), Some("main"));
    assert_eq!(operation.progress, Some(Progress { step: 1, total: 2 }));
}

#[test]
fn given_a_rebase_in_one_real_worktree_when_reading_its_sibling_then_the_sibling_reports_nothing() {
    // Given — deux worktrees du même dépôt ont des états différents, « parfois un rebase
    // en cours dans l'un et rien dans l'autre » (ADR-0012). Lire l'opération dans le
    // dépôt commun les confondrait.
    let sandbox = Sandbox::new("sibling");
    let worktree = repository_with_a_conflicting_rebase(&sandbox);
    git_may_fail(&worktree, &["rebase", "main"]);

    // When
    let sibling = metadata_of(&sandbox.path("ash"));

    // Then
    assert_eq!(sibling.operation, None);
    assert_eq!(
        sibling.head,
        Head::Branch {
            name: "main".to_owned()
        }
    );
}

#[test]
fn given_a_real_repository_whose_refs_have_been_packed_when_a_rebase_runs_then_the_target_is_still_named(
) {
    // Given — après un `git gc`, `refs/heads/main` n'existe plus comme fichier : il est
    // dans `packed-refs`. Ne surveiller et ne lire que `refs/` ferait afficher
    // `rebasing onto 80eca44` au lieu de `rebasing onto main`.
    let sandbox = Sandbox::new("packed");
    let worktree = repository_with_a_conflicting_rebase(&sandbox);
    git(&sandbox.path("ash"), &["pack-refs", "--all"]);
    assert!(
        !sandbox.path("ash/.git/refs/heads/main").exists(),
        "le décor du test suppose des refs empaquetées"
    );
    git_may_fail(&worktree, &["rebase", "main"]);

    // When
    let metadata = metadata_of(&worktree);

    // Then
    assert_eq!(
        metadata.operation.and_then(|operation| operation.onto),
        Some("main".to_owned())
    );
}

#[test]
fn given_a_real_detached_head_when_reading_the_metadata_then_it_reports_the_abbreviated_commit() {
    // Given
    let sandbox = Sandbox::new("detached");
    let repo = sandbox.path("solo");
    repository_at(&repo);
    commit(&repo, "base");
    git(&repo, &["checkout", "--quiet", "--detach"]);

    // When
    let metadata = metadata_of(&repo);

    // Then — sept caractères, comme git les abrège lui-même
    let Head::Detached { commit } = metadata.head else {
        panic!("attendu un HEAD détaché, obtenu {:?}", metadata.head);
    };
    assert_eq!(commit.len(), 7);
    assert!(commit.chars().all(|glyph| glyph.is_ascii_hexdigit()));
}

#[test]
fn given_a_real_linked_worktree_when_resolving_from_a_deep_subdirectory_then_it_finds_the_worktree_and_its_common_repository(
) {
    // Given — c'est ici que git écrit un `.git` **fichier**, et c'est ce cas que toute
    // implémentation naïve rate.
    let sandbox = Sandbox::new("linked");
    let main = sandbox.path("ash");
    repository_at(&main);
    git(
        &main,
        &[
            "worktree",
            "add",
            "--quiet",
            "../ash-sidebar",
            "-b",
            "sidebar",
        ],
    );
    let deep = sandbox.path("ash-sidebar/src/features");
    std::fs::create_dir_all(&deep).expect("le sous-dossier doit pouvoir être créé");
    assert!(
        sandbox.path("ash-sidebar/.git").is_file(),
        "le décor du test suppose un `.git` fichier"
    );

    // When
    let location = resolved(&deep);

    // Then
    assert_eq!(location.worktree.root, sandbox.real("ash-sidebar"));
    assert_eq!(location.worktree.name, "ash-sidebar");
    assert_eq!(
        location.worktree.git_dir,
        Some(sandbox.real("ash/.git/worktrees/ash-sidebar"))
    );
    let repo = location
        .repo
        .expect("un worktree lié appartient toujours à un dépôt");
    assert_eq!(repo.git_dir, sandbox.real("ash/.git"));
    assert_eq!(repo.root, sandbox.real("ash"));
    assert_eq!(repo.name, "ash");
}

#[test]
fn given_a_real_repository_without_linked_worktrees_when_resolving_then_it_stays_flat() {
    // Given
    let sandbox = Sandbox::new("flat");
    let repo = sandbox.path("solo");
    repository_at(&repo);
    let deep = sandbox.path("solo/src/deep");
    std::fs::create_dir_all(&deep).expect("le sous-dossier doit pouvoir être créé");

    // When
    let location = resolved(&deep);

    // Then — un seul niveau : rien à grouper au-dessus.
    assert_eq!(location.repo, None);
    assert_eq!(location.worktree.root, sandbox.real("solo"));
    assert_eq!(location.worktree.git_dir, Some(sandbox.real("solo/.git")));
}

#[test]
fn given_the_main_worktree_of_a_real_repository_that_hosts_a_linked_worktree_when_resolving_then_it_is_grouped_too(
) {
    // Given — son `.git` est un dossier comme celui d'un dépôt à plat ; ce qui change,
    // c'est qu'il a un frère.
    let sandbox = Sandbox::new("main");
    let main = sandbox.path("ash");
    repository_at(&main);
    git(
        &main,
        &["worktree", "add", "--quiet", "../ash-toc", "-b", "toc"],
    );

    // When
    let location = resolved(&main);
    let sibling = resolved(&sandbox.path("ash-toc"));

    // Then — les deux se rangent sous le même dépôt, et c'est tout l'objet d'ADR-0012.
    assert_eq!(
        location.repo.as_ref().map(|repo| &repo.git_dir),
        Some(&sandbox.real("ash/.git"))
    );
    assert_eq!(location.repo, sibling.repo);
    assert_ne!(location.worktree, sibling.worktree);
}

#[test]
fn given_a_real_directory_outside_any_repository_when_resolving_then_it_is_a_worktree_without_repository(
) {
    // Given
    let sandbox = Sandbox::new("outside");
    let notes = sandbox.path("notes/drafts");
    std::fs::create_dir_all(&notes).expect("le dossier doit pouvoir être créé");

    // When
    let location = resolved(&notes);

    // Then
    assert_eq!(location.repo, None);
    assert_eq!(location.worktree.git_dir, None);
    assert_eq!(location.worktree.root, sandbox.real("notes/drafts"));
    assert_eq!(location.worktree.name, "drafts");
}

#[test]
fn given_a_symlink_to_a_real_linked_worktree_when_resolving_through_it_then_it_reports_the_same_worktree(
) {
    // Given — un `cwd` peut très bien arriver par un lien : deux chemins pour le même
    // worktree ne doivent pas produire deux lignes dans la sidebar.
    let sandbox = Sandbox::new("symlink");
    let main = sandbox.path("ash");
    repository_at(&main);
    git(
        &main,
        &[
            "worktree",
            "add",
            "--quiet",
            "../ash-sidebar",
            "-b",
            "sidebar",
        ],
    );
    let link = sandbox.path("shortcut");
    std::os::unix::fs::symlink(sandbox.path("ash-sidebar"), &link)
        .expect("le lien symbolique doit pouvoir être créé");

    // When
    let through_link = resolved(&link);

    // Then
    assert_eq!(through_link, resolved(&sandbox.path("ash-sidebar")));
    assert_eq!(through_link.worktree.root, sandbox.real("ash-sidebar"));
}

#[test]
fn given_a_real_worktree_whose_repository_has_been_removed_when_resolving_then_it_reports_a_dangling_worktree(
) {
    // Given — le dossier du worktree survit à la disparition de son dépôt, et son `.git`
    // continue de désigner un `gitdir:` qui n'existe plus.
    let sandbox = Sandbox::new("dangling");
    let main = sandbox.path("ash");
    repository_at(&main);
    git(
        &main,
        &[
            "worktree",
            "add",
            "--quiet",
            "../ash-sidebar",
            "-b",
            "sidebar",
        ],
    );
    let orphan = sandbox.real("ash-sidebar");
    std::fs::remove_dir_all(&main).expect("le dépôt doit pouvoir être supprimé");

    // When
    let resolved = resolve(&orphan);

    // Then — mieux vaut le dire que faire passer un worktree orphelin pour un dossier
    // ordinaire.
    assert!(
        matches!(resolved, Err(GitError::Dangling { .. })),
        "attendu un worktree orphelin, obtenu {resolved:?}"
    );
}
