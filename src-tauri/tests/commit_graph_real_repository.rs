//! Le graphe de commits, lu sur un **vrai** dépôt (#27, spec §7.2).
//!
//! Les tests unitaires prouvent l'algorithme des couloirs derrière un double, et la colonne
//! `by` derrière un autre. Ce qu'aucun double ne peut prouver est ici : que le jeu
//! d'arguments durci de `git_cli.rs` rend bien ce que le dessin attend — les parents de
//! chaque commit, les refs qui nomment les branches, et un ordre **topologique** dans lequel
//! deux branches ne s'entrelacent pas.
//!
//! C'est la vérification qui compte le plus de ce fichier : une option mal orthographiée
//! ferait rendre un vecteur vide à `SystemGit::window` — donc un panneau vide, sans la
//! moindre erreur nulle part.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use ash_lib::features::git::{lay_out, GraphLog, SystemGit};

/// Un dossier temporaire qui se supprime à la fin du test, réussi ou non.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("ash-graph-{label}-{}-{unique}", std::process::id()));
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

/// Lance `git` dans un environnement clos : ni configuration globale, ni configuration
/// système, ni identité de la machine. Le test doit donner le même résultat partout.
fn git(cwd: &Path, args: &[&str]) {
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
    assert!(status.success(), "git {args:?} a échoué dans {cwd:?}");
}

fn commit(root: &Path, message: &str) {
    git(root, &["commit", "--allow-empty", "--quiet", "-m", message]);
}

/// Un dépôt en Y : `main`, une branche `side` qui en part, et une fusion des deux.
fn forked_repository(root: &Path) {
    git(root, &["init", "--quiet"]);
    commit(root, "chore: initial import");
    commit(root, "feat: sur main");
    git(root, &["checkout", "--quiet", "-b", "side", "HEAD~1"]);
    commit(root, "feat: sur side");
    git(root, &["checkout", "--quiet", "main"]);
    git(
        root,
        &["merge", "--quiet", "--no-ff", "-m", "merge: side", "side"],
    );
}

#[test]
fn given_a_forked_repository_when_the_graph_reads_it_then_it_gets_the_parents_the_drawing_needs() {
    // Given — un dépôt en Y, la forme la plus simple qui ait quelque chose à dessiner. Sans
    // les parents, le graphe n'a que des points sans traits ; sans les refs, il ne peut pas
    // nommer une branche repliée.
    let sandbox = Sandbox::new("fork");
    forked_repository(&sandbox.root);

    // When — l'invocation réelle, avec le jeu d'arguments durci de la production
    let commits = SystemGit::default().window(&sandbox.root, 50);

    // Then
    assert_eq!(commits.len(), 4, "{commits:#?}");
    let merge = &commits[0];
    assert_eq!(merge.subject, "merge: side");
    assert_eq!(
        merge.parents.len(),
        2,
        "une fusion a deux parents, et c'est le second qui ouvre un couloir"
    );
    assert!(
        commits
            .iter()
            .any(|commit| commit.refs.iter().any(|name| name.contains("main"))),
        "les refs nomment les branches : {commits:#?}"
    );
    assert!(
        commits.iter().all(|commit| !commit.author.is_empty()),
        "le nom d'auteur git est ce que la colonne `by` affiche faute d'attribution"
    );
}

#[test]
fn given_a_forked_repository_when_it_is_laid_out_then_the_side_branch_gets_its_own_lane() {
    // Given — c'est la chaîne complète : le processus `git`, la lecture de sa sortie, et
    // l'affectation des couloirs. Une seule d'entre elles cassée rend un dessin plat.
    let sandbox = Sandbox::new("lanes");
    forked_repository(&sandbox.root);
    let commits = SystemGit::default().window(&sandbox.root, 50);

    // When — l'heure est passée, jamais lue : la règle des 30 jours ne doit pas dépendre du
    // jour où ce test tourne.
    let layout = lay_out(&commits, 1_786_000_000_000);

    // Then
    assert_eq!(layout.lanes, 2, "{layout:#?}");
    assert!(
        layout.folded.is_empty(),
        "deux couloirs sont sous le seuil : rien ne se replie"
    );
    assert_eq!(layout.rows.len(), commits.len());
}

#[test]
fn given_a_repository_that_is_not_one_when_the_graph_reads_it_then_it_simply_has_nothing_to_show() {
    // Given — un onglet dans `/tmp` est un cas nominal, pas une panne : `git log` sort en 128
    // et n'écrit rien. Le panneau doit dire « rien à montrer », pas rester à charger.
    let sandbox = Sandbox::new("outside");

    // When
    let commits = SystemGit::default().window(&sandbox.root, 50);

    // Then
    assert!(commits.is_empty());
}
