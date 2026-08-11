//! Tests d'intégration de la lecture d'un dépôt — sur de vrais dépôts.
//!
//! Les tests unitaires vérifient les règles derrière le trait `FileSystem`. Ceux-ci
//! vérifient ce qu'aucun double ne peut prouver : que ce qu'Ash lit est bien ce que
//! **git écrit** — le `.git` fichier d'un worktree lié, son `gitdir:`, son `commondir`,
//! et les fichiers qu'un rebase laisse derrière lui pendant qu'il est arrêté.
//!
//! Ils lancent `git` pour fabriquer le décor. Ash n'en lance qu'**un** de son côté, et
//! seulement pour ce qu'aucun fichier de contrôle ne porte : l'état de l'arbre de travail
//! et l'avance sur l'amont. C'est aussi ce que ces tests-ci vérifient de bout en bout —
//! l'invocation réelle, sa sortie réelle, et les comptes qu'on en tire.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use ash_lib::features::git::{
    parse_status, read_metadata, resolve_worktree, GitError, Head, OperationKind, Progress, Status,
    StatusReader, SystemFileSystem, SystemGit, WorktreeLocation, WorktreeMetadata,
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

/// L'état de l'arbre, lu comme la surveillance le lit : un vrai `git`, une vraie sortie.
fn status_of(worktree_root: &Path) -> Status {
    let output = SystemGit::default()
        .read(worktree_root)
        .expect("git doit répondre pour un dépôt de test");
    parse_status(&output)
}

#[test]
fn given_a_real_worktree_with_local_changes_when_asking_git_then_the_counts_are_files_not_lines() {
    // Given — un fichier ajouté à l'index, un modifié sur deux lignes, un supprimé, et
    // deux chemins non suivis dont un dossier entier. C'est le `+3 ~1` de la maquette.
    let sandbox = Sandbox::new("status");
    let repo = sandbox.path("solo");
    repository_at(&repo);
    commit(&repo, "base");
    std::fs::write(repo.join("second.txt"), "deux\n").expect("le fichier doit s'écrire");
    git(&repo, &["add", "second.txt"]);
    git(&repo, &["commit", "--quiet", "-m", "second"]);

    std::fs::write(repo.join("ajoute.txt"), "neuf\n").expect("le fichier doit s'écrire");
    git(&repo, &["add", "ajoute.txt"]);
    std::fs::write(repo.join("f.txt"), "une\nautre\nligne\n").expect("le fichier doit s'écrire");
    std::fs::remove_file(repo.join("second.txt")).expect("le fichier doit disparaître");
    std::fs::write(repo.join("perdu.txt"), "x\n").expect("le fichier doit s'écrire");
    std::fs::create_dir_all(repo.join("neuf")).expect("le dossier doit se créer");
    std::fs::write(repo.join("neuf/dedans.txt"), "y\n").expect("le fichier doit s'écrire");

    // When
    let status = status_of(&repo);

    // Then — un fichier modifié sur trois lignes reste **un** fichier ; le dossier
    // entièrement nouveau compte pour une entrée, comme git le rend
    assert_eq!(status.tree.modified, 1);
    assert_eq!(status.tree.deleted, 1);
    assert_eq!(status.tree.added, 3);
    assert_eq!(status.tree.conflicted, 0);
}

#[test]
fn given_a_real_branch_ahead_and_behind_its_upstream_when_asking_git_then_both_counts_are_reported()
{
    // Given — un clone qui a divergé de son origine : deux commits locaux, un distant
    let sandbox = Sandbox::new("upstream");
    let origin = sandbox.path("origin");
    repository_at(&origin);
    commit(&origin, "base");
    git(&sandbox.root, &["clone", "--quiet", "origin", "clone"]);
    let clone = sandbox.path("clone");
    commit(&clone, "local-un");
    commit(&clone, "local-deux");
    commit(&origin, "distant");
    git(&clone, &["fetch", "--quiet", "origin"]);

    // When
    let status = status_of(&clone);

    // Then — `↑2 ↓1`, tiré de l'en-tête du **même** appel que l'état de l'arbre
    let upstream = status.upstream.expect("le clone suit son origine");
    assert_eq!((upstream.ahead, upstream.behind), (2, 1));
}

#[test]
fn given_a_real_repository_without_an_upstream_when_asking_git_then_nothing_is_compared() {
    // Given — une branche locale toute neuve : il n'y a rien à comparer, et inventer
    // `↑0 ↓0` ferait croire à une synchronisation qui n'existe pas
    let sandbox = Sandbox::new("no-upstream");
    let repo = sandbox.path("solo");
    repository_at(&repo);
    commit(&repo, "base");

    // When
    let status = status_of(&repo);

    // Then
    assert_eq!(status.upstream, None);
    assert!(status.tree.is_clean());
}

#[test]
fn given_a_real_merge_conflict_when_asking_git_then_the_conflicted_file_is_counted_apart() {
    // Given — pendant un rebase arrêté, la ligne de statut doit distinguer « en conflit »
    // de « modifié » : ce ne sont pas les mêmes gestes qui suivent
    let sandbox = Sandbox::new("conflict");
    let worktree = repository_with_a_conflicting_rebase(&sandbox);
    git_may_fail(&worktree, &["rebase", "main"]);

    // When
    let status = status_of(&worktree);

    // Then
    assert_eq!(status.tree.conflicted, 1);
    assert_eq!(status.tree.modified, 0);
}

#[test]
fn given_a_directory_outside_any_repository_when_asking_git_then_it_answers_nothing_at_all() {
    // Given — `git status` y sort en erreur. Ash doit le lire comme « pas d'état
    // d'arbre », pas comme un arbre propre : afficher `+0 ~0` serait un mensonge.
    let sandbox = Sandbox::new("outside-status");
    let notes = sandbox.path("notes");
    std::fs::create_dir_all(&notes).expect("le dossier doit pouvoir être créé");

    // When
    let output = SystemGit::default().read(&notes);

    // Then
    assert_eq!(output, None);
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
fn given_a_real_flat_repository_when_it_gains_a_linked_worktree_then_the_same_directory_resolves_into_a_group(
) {
    // Given — un dépôt sans worktree lié, affiché à plat (ADR-0012), et un onglet ouvert
    // dedans. C'est la prémisse que le registre d'onglets ne peut pas mémoriser par le seul
    // `cwd` : la réponse dépend de l'état du dépôt, pas du chemin.
    let sandbox = Sandbox::new("gains-worktree");
    let repo = sandbox.path("omelette");
    repository_at(&repo);
    let flat = resolved(&repo);

    // When — depuis un autre terminal, pendant qu'Ash tourne
    git(
        &repo,
        &["worktree", "add", "--quiet", "../omelette-toc", "-b", "toc"],
    );
    let grouped = resolved(&repo);

    // Then — même répertoire, réponse différente. Et l'entrée est apparue là où la
    // surveillance de `.git` regarde : sous le dossier git du dépôt.
    assert_eq!(flat.repo, None);
    assert_eq!(
        grouped.repo.map(|repo| repo.git_dir),
        Some(sandbox.real("omelette/.git"))
    );
    // Git nomme l'entrée d'après le **dossier** du worktree, pas d'après sa branche.
    assert!(sandbox
        .path("omelette/.git/worktrees/omelette-toc")
        .is_dir());
}

#[test]
fn given_a_real_grouped_repository_when_its_last_linked_worktree_is_removed_then_the_same_directory_falls_back_flat(
) {
    // Given — le cas inverse : `git worktree remove` retire aussi l'entrée d'administration,
    // et le dépôt n'a plus personne à grouper.
    let sandbox = Sandbox::new("loses-worktree");
    let repo = sandbox.path("omelette");
    repository_at(&repo);
    git(
        &repo,
        &["worktree", "add", "--quiet", "../omelette-toc", "-b", "toc"],
    );
    let grouped = resolved(&repo);

    // When
    git(&repo, &["worktree", "remove", "../omelette-toc"]);
    let flat = resolved(&repo);

    // Then
    assert!(grouped.repo.is_some());
    assert_eq!(flat.repo, None);
    assert!(!sandbox
        .path("omelette/.git/worktrees/omelette-toc")
        .exists());
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

/// Ash lance `git status` tout seul, sur le simple fait que le shell de l'utilisateur a
/// fait un `cd`. Un dépôt hostile récupéré puis simplement visité ne doit donc rien
/// pouvoir exécuter.
///
/// `core.fsmonitor` est le vecteur : sa valeur est une **commande** que `git status`
/// lance, et elle se pose dans le `.git/config` du dépôt visité. Le `safe.directory` de
/// git ne protège pas de ce cas — il ne se déclenche que pour un dépôt appartenant à un
/// *autre* utilisateur, alors qu'un dépôt téléchargé appartient au nôtre.
///
/// Ce test échoue si quelqu'un retire la surcharge de `HARDENED_STATUS_ARGS` : vérifié en
/// la retirant, le témoin apparaît.
#[test]
fn given_a_visited_repository_that_configures_a_fsmonitor_command_when_ash_reads_its_status_then_the_command_never_runs(
) {
    // Given — un dépôt qui exécute une commande à la moindre lecture de statut
    let sandbox = Sandbox::new("fsmonitor");
    let repo = sandbox.path("hostile");
    std::fs::create_dir_all(&repo).expect("le dépôt doit pouvoir être créé");
    git(&repo, &["init", "--quiet", "--initial-branch=main", "."]);
    git(
        &repo,
        &["commit", "--quiet", "--allow-empty", "-m", "racine"],
    );

    let witness = sandbox.path("witness");
    git(
        &repo,
        &[
            "config",
            "core.fsmonitor",
            &format!("sh -c 'touch {}'", witness.display()),
        ],
    );

    // When — exactement ce qu'Ash fait quand l'onglet arrive dans ce dossier
    let output = SystemGit::default().read(&sandbox.real("hostile"));

    // Then — le statut est lu, et rien n'a été exécuté
    assert!(
        output.is_some(),
        "le dépôt reste lisible : durcir l'appel ne doit pas le casser"
    );
    assert!(
        !witness.exists(),
        "`core.fsmonitor` du dépôt visité a été exécuté — visiter un dossier suffirait \
         à faire tourner du code arbitraire"
    );
}
