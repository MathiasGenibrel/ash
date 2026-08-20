//! L'onglet de merge sur un **vrai** dépôt (spec §7.4, issue #30).
//!
//! Les tests unitaires de `features::merge` vérifient les règles derrière les trois ports :
//! à partir de tel fichier, tel découpage ; à partir de telle opération, tels noms de côtés.
//! Aucun d'eux ne peut prouver les deux choses qui décident si l'écran dit vrai :
//!
//! 1. que ce que git écrit entre `<<<<<<<` et `=======` est bien le côté que
//!    [`MergeSides::left`] nomme — et qu'il ne s'échange pas entre un rebase et un merge.
//!    C'est **le** critère d'acceptation du ticket, et il ne se vérifie qu'en lisant un
//!    fichier que git a écrit lui-même ;
//! 2. que `git add` puis `git <op> --continue`, lancés par Ash avec son préfixe durci,
//!    terminent réellement l'opération — `core.editor=true` compris, sans quoi
//!    `rebase --continue` resterait pendu sur un éditeur qui n'existe pas.
//!
//! Les deux scénarios utilisent **les deux mêmes branches** dans les deux sens. C'est
//! délibéré : si les côtés s'échangeaient, les deux tests ne pourraient pas passer ensemble.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ash_lib::features::git::{
    detect_test_command, parse_status, read_metadata, read_stopped, resolve_worktree, Head,
    OperationKind, StatusReader, StoppedOperation, SystemFileSystem, SystemGit, TreeWriter,
};
use ash_lib::features::merge::{
    ConflictFiles, MergeOutcome, MergeSurface, MergeView, StoppedWorktree, TreeGit,
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
            std::env::temp_dir().join(format!("ash-merge-{label}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("le bac à sable doit pouvoir être créé");
        Self { root }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Lance `git` dans un environnement clos : ni configuration globale, ni identité de la
/// machine. Le test doit donner le même résultat partout.
fn run_git(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args(["-c", "init.defaultBranch=main"])
        .args(args)
        .status()
        .expect("git doit être installé")
        .success()
}

fn git(cwd: &Path, args: &[&str]) {
    assert!(run_git(cwd, args), "git {args:?} a échoué dans {cwd:?}");
}

fn git_may_fail(cwd: &Path, args: &[&str]) {
    let _ = run_git(cwd, args);
}

fn write(path: &Path, content: &str) {
    std::fs::write(path, content).expect("le fichier doit pouvoir être écrit");
}

/// Un dépôt avec `main` et `feat` qui se contredisent sur une ligne de `probe.rs`.
///
/// L'identité est écrite dans la **configuration du dépôt** et non passée en `-c` : c'est
/// Ash qui lancera `rebase --continue`, et son invocation ne porte aucune identité — comme
/// dans la vraie vie.
fn repository_with_two_diverging_branches(sandbox: &Sandbox, label: &str) -> PathBuf {
    let repo = sandbox.root.join(label);
    std::fs::create_dir_all(&repo).expect("le dépôt doit pouvoir être créé");
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.name", "Ash Test"]);
    git(&repo, &["config", "user.email", "test@ash.local"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);

    write(&repo.join("probe.rs"), "base\n");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "base"]);

    git(&repo, &["checkout", "--quiet", "-b", "feat"]);
    write(&repo.join("probe.rs"), "feat\n");
    git(&repo, &["commit", "--quiet", "-am", "feat moves"]);

    git(&repo, &["checkout", "--quiet", "main"]);
    write(&repo.join("probe.rs"), "main\n");
    git(&repo, &["commit", "--quiet", "-am", "main moves"]);

    repo
}

/// La lecture réelle de l'opération arrêtée — celle de #29, réutilisée telle quelle.
struct RealWorktree;

impl RealWorktree {
    fn read(worktree_root: &Path) -> Option<(StoppedOperation, Head)> {
        let fs = SystemFileSystem;
        let (git_dir, common_dir) = resolve_worktree(&fs, worktree_root)
            .ok()?
            .git_dirs()
            .expect("un dépôt a bien deux dossiers git");
        let mut metadata = read_metadata(&fs, &git_dir, &common_dir).ok()?;
        metadata.status = SystemGit::default()
            .read(worktree_root)
            .as_deref()
            .map(parse_status);
        let head = metadata.head.clone();
        let test_command = detect_test_command(&fs, worktree_root);
        read_stopped(&fs, &git_dir, &metadata, test_command).map(|stopped| (stopped, head))
    }
}

impl StoppedWorktree for RealWorktree {
    fn stopped(&self, worktree_root: &Path) -> Option<StoppedOperation> {
        Self::read(worktree_root).map(|(stopped, _)| stopped)
    }

    fn head(&self, worktree_root: &Path) -> Option<Head> {
        let fs = SystemFileSystem;
        let (git_dir, common_dir) = resolve_worktree(&fs, worktree_root)
            .ok()?
            .git_dirs()
            .expect("un dépôt a bien deux dossiers git");
        read_metadata(&fs, &git_dir, &common_dir)
            .ok()
            .map(|metadata| metadata.head)
    }
}

struct RealFiles;

impl ConflictFiles for RealFiles {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn write(&self, path: &Path, text: &str) -> Result<(), String> {
        std::fs::write(path, text).map_err(|why| why.to_string())
    }
}

/// Les deux verbes qui écrivent, lancés par le **vrai** `SystemGit` — préfixe durci compris.
struct RealGit;

impl TreeGit for RealGit {
    fn stage(&self, worktree_root: &Path, path: &str) -> MergeOutcome {
        let args = vec!["add".to_owned(), "--".to_owned(), path.to_owned()];
        completed(format!("Stage {path}"), worktree_root, &args)
    }

    fn resume(&self, worktree_root: &Path, kind: OperationKind) -> MergeOutcome {
        let verb = match kind {
            OperationKind::Rebase => "rebase",
            OperationKind::Am => "am",
            OperationKind::Merge => "merge",
        };
        let args = vec![verb.to_owned(), "--continue".to_owned()];
        completed(format!("git {verb} --continue"), worktree_root, &args)
    }
}

fn completed(label: String, worktree_root: &Path, args: &[String]) -> MergeOutcome {
    match SystemGit::default().run(worktree_root, args) {
        Some(done) => MergeOutcome {
            label,
            success: done.success,
            output: done.output,
        },
        None => MergeOutcome {
            label,
            success: false,
            output: "git could not be run".to_owned(),
        },
    }
}

fn surface() -> MergeSurface {
    MergeSurface::new(
        Arc::new(RealWorktree),
        Arc::new(RealFiles),
        Arc::new(RealGit),
    )
}

fn shown(view: &MergeView) -> &ash_lib::features::merge::StoppedView {
    view.stopped
        .as_ref()
        .expect("l'opération doit être arrêtée")
}

#[test]
fn given_a_real_rebase_of_feat_onto_main_when_the_merge_tab_opens_then_the_left_side_is_named_main_and_holds_mains_line(
) {
    // Given — pendant un rebase, le `ours` de git est la branche **sur laquelle** on
    // rebase. C'est l'inversion que la spec §7.4 nomme, et seul un vrai `git rebase`
    // peut prouver de quel côté il écrit quoi.
    let sandbox = Sandbox::new("rebase");
    let repo = repository_with_two_diverging_branches(&sandbox, "ash");
    git(&repo, &["checkout", "--quiet", "feat"]);
    git_may_fail(&repo, &["rebase", "main"]);

    let surface = surface();
    let tab = surface
        .open(&repo, "01REBASE".to_owned())
        .expect("le rebase est arrêté");

    // When
    let view = surface.view(&tab).expect("l'onglet existe");
    let stopped = shown(&view);

    // Then — le nom **et** le contenu tombent du même côté
    assert_eq!(stopped.sides.left.name, "main");
    assert_eq!(stopped.sides.right.name, "feat");
    let file = stopped.files.first().expect("un fichier en conflit");
    assert_eq!(file.path, "probe.rs");
    assert_eq!(file.hunks.len(), 1);
    assert_eq!(file.hunks[0].ours, "main\n");
    assert_eq!(file.hunks[0].theirs, "feat\n");
    assert!(!stopped.can_continue);
    assert_eq!(stopped.unresolved, 1);
    assert_eq!(view.title, "rebase feat onto main");
}

#[test]
fn given_a_real_merge_of_feat_into_main_when_the_merge_tab_opens_then_the_left_side_is_still_main_and_still_holds_mains_line(
) {
    // Given — les **mêmes deux branches**, l'autre sens. Le `ours` de git désigne ici la
    // branche courante : si l'onglet nommait les côtés à partir du seul jargon de git, ce
    // test et le précédent ne pourraient pas passer ensemble.
    let sandbox = Sandbox::new("merge");
    let repo = repository_with_two_diverging_branches(&sandbox, "ash");
    git_may_fail(&repo, &["merge", "--no-edit", "feat"]);

    let surface = surface();
    let tab = surface
        .open(&repo, "01MERGE".to_owned())
        .expect("le merge est arrêté");

    // When
    let view = surface.view(&tab).expect("l'onglet existe");
    let stopped = shown(&view);

    // Then
    assert_eq!(stopped.operation.kind, OperationKind::Merge);
    assert_eq!(stopped.sides.left.name, "main");
    assert_eq!(stopped.sides.right.name, "feat");
    let file = stopped.files.first().expect("un fichier en conflit");
    assert_eq!(file.hunks[0].ours, "main\n");
    assert_eq!(file.hunks[0].theirs, "feat\n");
    // Un merge n'a pas de `--skip` : il n'a qu'un pas.
    assert_eq!(stopped.escapes, vec!["git merge --abort".to_owned()]);
    assert_eq!(view.title, "merge feat into main");
}

#[test]
fn given_a_real_stopped_rebase_when_the_last_hunk_is_settled_and_continue_is_pressed_then_git_finishes_it(
) {
    // Given — la boucle entière : trancher, écrire, `git add`, `continue`. C'est aussi le
    // seul test qui prouve que `core.editor=true` fait son travail : sans lui,
    // `rebase --continue` ouvrirait un éditeur et ne rendrait jamais la main.
    let sandbox = Sandbox::new("continue");
    let repo = repository_with_two_diverging_branches(&sandbox, "ash");
    git(&repo, &["checkout", "--quiet", "feat"]);
    git_may_fail(&repo, &["rebase", "main"]);

    let surface = surface();
    let tab = surface
        .open(&repo, "01DONE".to_owned())
        .expect("le rebase est arrêté");

    // When
    let after = surface
        .resolve(&tab, "probe.rs", 0, "main and feat")
        .expect("le hunk existe");
    let outcome = surface.resume(&tab).expect("l'onglet existe");

    // Then
    assert!(shown(&after).can_continue, "le dernier conflit est tranché");
    assert!(outcome.success, "{}", outcome.output);
    assert_eq!(
        std::fs::read_to_string(repo.join("probe.rs")).expect("le fichier est là"),
        "main and feat\n"
    );
    // Plus rien n'est arrêté : l'onglet reste ouvert et le dit, il ne se referme pas seul.
    let view = surface.view(&tab).expect("l'onglet existe encore");
    assert!(view.stopped.is_none());
    assert_eq!(view.title, "nothing to merge");
}

#[test]
fn given_a_merge_tab_closed_in_the_middle_when_it_is_opened_again_then_the_index_still_has_everything(
) {
    // Given — le critère : « fermer l'onglet ne perd rien : l'état vit dans l'index git,
    // pas dans Ash ». La preuve doit passer par un vrai dépôt : c'est git qui garde, pas
    // une structure d'Ash qu'on aurait pu oublier de vider.
    let sandbox = Sandbox::new("reopen");
    let repo = repository_with_two_diverging_branches(&sandbox, "ash");
    git(&repo, &["checkout", "--quiet", "feat"]);
    git_may_fail(&repo, &["rebase", "main"]);

    let surface = surface();
    let tab = surface
        .open(&repo, "01KEEP".to_owned())
        .expect("le rebase est arrêté");
    surface
        .resolve(&tab, "probe.rs", 0, "settled")
        .expect("le hunk existe");

    // When — l'onglet disparaît entièrement, puis un autre s'ouvre sur le même worktree
    surface.close(&tab);
    let reopened = surface
        .open(&repo, "01BACK".to_owned())
        .expect("le rebase est toujours arrêté");

    // Then — le rebase est toujours arrêté (personne n'a pressé `continue`), le fichier
    // porte la décision, et git ne le compte plus comme un conflit : il est dans l'index
    let view = surface.view(&reopened).expect("l'onglet existe");
    let stopped = shown(&view);
    assert!(
        !stopped.files.iter().any(|file| file.path == "probe.rs"),
        "un fichier ajouté à l'index n'est plus en conflit : {:?}",
        stopped.files
    );
    assert_eq!(stopped.unresolved, 0);
    assert!(
        stopped.can_continue,
        "il ne reste rien à trancher — le travail a survécu à la fermeture"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("probe.rs")).expect("le fichier est là"),
        "settled\n"
    );
}
