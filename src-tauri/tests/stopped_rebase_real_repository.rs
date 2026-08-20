//! Ce qu'un **vrai** rebase arrêté laisse derrière lui, et ce qu'Ash en tire (spec §7.4).
//!
//! Les tests unitaires de `features::git::stopped` vérifient la règle derrière le trait
//! `FileSystem` : à partir de tel fichier, telle lecture. Aucun d'eux ne peut prouver ce
//! qui compte le plus ici — que **git écrit vraiment** `stopped-sha`, `message` et
//! `ORIG_HEAD` là où Ash les cherche, et que ce que `git status --porcelain=v2` nomme sur
//! une ligne `u` est bien le chemin en conflit. Une version de git qui déplacerait l'un
//! des quatre ferait afficher un prompt faux, et seuls ces tests-ci rougiraient.
//!
//! Ash ne lance **rien** de son côté ici, hors le `git status` déjà encadré par
//! `git_cli.rs` : aucun verbe git n'a été ajouté pour cette lecture.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use ash_lib::features::git::{
    compose_conflict_prompt, detect_test_command, parse_status, read_metadata, read_stopped,
    resolve_worktree, OperationKind, StatusReader, StoppedOperation, SystemFileSystem, SystemGit,
};

/// Un dossier temporaire qui se supprime à la fin du test, réussi ou non.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "ash-stopped-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("le bac à sable doit pouvoir être créé");
        Self { root }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
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
        .expect("git doit être installé")
        .success()
}

fn git(cwd: &Path, args: &[&str]) {
    assert!(run_git(cwd, args), "git {args:?} a échoué dans {cwd:?}");
}

/// La même chose, pour les commandes dont l'échec **est** le décor : un rebase arrêté sur
/// conflit sort en erreur, et c'est exactement l'état qu'on veut lire.
fn git_may_fail(cwd: &Path, args: &[&str]) {
    let _ = run_git(cwd, args);
}

fn write(path: &Path, content: &str) {
    std::fs::write(path, content).expect("le fichier doit pouvoir être écrit");
}

/// Un dépôt, une branche `feat`, et un rebase arrêté sur **deux** fichiers en conflit.
///
/// Deux, et pas un : c'est la liste des chemins que la spec §7.4 demande, et un seul
/// chemin ne distinguerait pas une liste d'un compte.
fn repository_with_a_stopped_rebase(sandbox: &Sandbox) -> PathBuf {
    let repo = sandbox.path("ash");
    std::fs::create_dir_all(&repo).expect("le dépôt doit pouvoir être créé");
    git(&repo, &["init", "--quiet"]);

    write(&repo.join("probe.rs"), "base\n");
    write(&repo.join("main.ts"), "base\n");
    write(&repo.join("Cargo.toml"), "[package]\nname = \"ash\"\n");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "base"]);

    git(&repo, &["checkout", "--quiet", "-b", "feat"]);
    write(&repo.join("probe.rs"), "feat\n");
    write(&repo.join("main.ts"), "feat\n");
    git(&repo, &["commit", "--quiet", "-am", "add the probe"]);

    git(&repo, &["checkout", "--quiet", "main"]);
    write(&repo.join("probe.rs"), "main\n");
    write(&repo.join("main.ts"), "main\n");
    git(&repo, &["commit", "--quiet", "-am", "main moves"]);

    git(&repo, &["checkout", "--quiet", "feat"]);
    git_may_fail(&repo, &["rebase", "main"]);
    repo
}

/// L'état arrêté, lu exactement comme l'application le lit : la résolution de la feature,
/// le `git status` déjà encadré, puis la lecture des fichiers de contrôle.
fn stopped_at(worktree_root: &Path) -> Option<StoppedOperation> {
    let fs = SystemFileSystem;
    let (git_dir, common_dir) = resolve_worktree(&fs, worktree_root)
        .expect("le worktree doit se résoudre")
        .git_dirs()
        .expect("un dépôt a bien deux dossiers git");

    let mut metadata =
        read_metadata(&fs, &git_dir, &common_dir).expect("les métadonnées se lisent");
    metadata.status = SystemGit::default()
        .read(worktree_root)
        .as_deref()
        .map(parse_status);

    let test_command = detect_test_command(&fs, worktree_root);
    read_stopped(&fs, &git_dir, &metadata, test_command)
}

#[test]
fn given_a_real_rebase_stopped_on_two_conflicts_when_reading_it_then_the_paths_the_stopped_commit_and_orig_head_are_the_ones_git_wrote(
) {
    // Given — les trois choses de la spec §7.4, sur un vrai dépôt. Aucun double ne prouve
    // que git écrit `stopped-sha` et `ORIG_HEAD` là où Ash les cherche.
    let sandbox = Sandbox::new("read");
    let repo = repository_with_a_stopped_rebase(&sandbox);

    // When
    let stopped = stopped_at(&repo).expect("un rebase arrêté sur conflit est une opération");

    // Then
    assert_eq!(stopped.operation.kind, OperationKind::Rebase);
    assert_eq!(stopped.operation.branch.as_deref(), Some("feat"));
    assert_eq!(stopped.operation.onto.as_deref(), Some("main"));

    let mut conflicts = stopped.conflicts.clone();
    conflicts.sort();
    assert_eq!(
        conflicts,
        vec!["main.ts".to_owned(), "probe.rs".to_owned()],
        "les chemins viennent des lignes `u` du `git status` déjà lancé"
    );
    assert_eq!(stopped.conflicted_total, Some(2));

    let commit = stopped
        .stopped_at
        .as_ref()
        .expect("git écrit `stopped-sha` quand un pick s'arrête");
    assert!(
        commit.commit.chars().all(|c| c.is_ascii_hexdigit()) && !commit.commit.is_empty(),
        "un identifiant de commit, et rien d'autre : {commit:?}"
    );
    assert_eq!(
        commit.subject.as_deref(),
        Some("add the probe"),
        "le sujet vient de `rebase-merge/message`, et il est **du commit en cours**"
    );

    assert!(
        stopped.orig_head.is_some(),
        "`ORIG_HEAD` est le filet de secours de la spec §7.4"
    );
    // Le dépôt de test porte un `Cargo.toml` à sa racine, et rien d'autre : c'est la seule
    // preuve disponible, donc la seule commande qu'Ash a le droit de nommer.
    assert_eq!(stopped.test_command.as_deref(), Some("cargo test"));
}

#[test]
fn given_a_real_stopped_rebase_when_composing_the_prompt_then_it_is_one_line_and_carries_the_three_things(
) {
    // Given — le prompt part dans un PTY : un `\n` y **est** la touche `⏎`, et Ash n'en
    // presse jamais ([ADR-0015](../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md))
    let sandbox = Sandbox::new("prompt");
    let repo = repository_with_a_stopped_rebase(&sandbox);
    let stopped = stopped_at(&repo).expect("un rebase arrêté sur conflit est une opération");

    // When
    let prompt = compose_conflict_prompt(&stopped.prompt_subject());

    // Then
    assert!(!prompt.contains('\n'), "{prompt}");
    assert!(!prompt.contains('\r'), "{prompt}");
    assert!(prompt.contains("probe.rs"), "{prompt}");
    assert!(prompt.contains("main.ts"), "{prompt}");
    assert!(prompt.contains("cargo test"), "{prompt}");
    let commit = stopped.stopped_at.expect("il y a un commit d'arrêt");
    assert!(prompt.contains(&commit.commit), "{prompt}");
}

#[test]
fn given_a_real_repository_with_nothing_in_progress_when_reading_a_stopped_operation_then_there_is_none(
) {
    // Given — le cas courant, et de loin : rien ne doit s'afficher
    let sandbox = Sandbox::new("clean");
    let repo = sandbox.path("calme");
    std::fs::create_dir_all(&repo).expect("le dépôt doit pouvoir être créé");
    git(&repo, &["init", "--quiet"]);
    write(&repo.join("f.txt"), "base\n");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "base"]);

    // When / Then
    assert!(stopped_at(&repo).is_none());
}

#[test]
fn given_a_real_stopped_rebase_when_ash_has_read_it_then_ash_has_not_touched_the_repository() {
    // Given — spec §7.4 : « Ash **ne touche à rien** de lui-même ». Le vérifier demande
    // de photographier le dossier git avant et après : une lecture qui écrirait un
    // `index.lock` ou rafraîchirait l'index se verrait ici, et nulle part ailleurs.
    let sandbox = Sandbox::new("readonly");
    let repo = repository_with_a_stopped_rebase(&sandbox);
    let before = fingerprint(&repo.join(".git"));

    // When — la lecture complète, `git status` compris
    let stopped = stopped_at(&repo).expect("un rebase arrêté sur conflit est une opération");
    let _ = compose_conflict_prompt(&stopped.prompt_subject());

    // Then
    assert_eq!(
        fingerprint(&repo.join(".git")),
        before,
        "lire un rebase arrêté ne doit rien changer dans `.git` — `--no-optional-locks` \
         est là pour ça (voir `git_cli.rs`)"
    );
    // Et les deux sorties de secours sont **du texte à montrer**, jamais des actions :
    // le rebase est toujours en cours après la lecture.
    assert_eq!(
        stopped.escapes,
        vec![
            "git rebase --abort".to_owned(),
            "git rebase --skip".to_owned()
        ]
    );
}

/// Le contenu d'un dossier git, chemin par chemin, avec la taille de chaque fichier.
fn fingerprint(git_dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut seen = Vec::new();
    let mut stack = vec![git_dir.to_owned()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.metadata() {
                Ok(metadata) if metadata.is_dir() => stack.push(path),
                Ok(metadata) => seen.push((path, metadata.len())),
                Err(_) => {}
            }
        }
    }
    seen.sort();
    seen
}
