//! Ce qu'un worktree dit de lui-même — le modèle, et sa moitié lue dans `.git`.
//!
//! Les métadonnées de la spec §5.3 viennent de **deux** sources, et la différence est
//! structurante :
//!
//! - la branche et l'opération en cours se lisent dans les fichiers de contrôle du dépôt,
//!   ici même, derrière le trait [`FileSystem`]. C'est instantané, et ça ne dépend de rien
//!   d'installé ;
//! - l'état de l'arbre (`+3 ~1`) et l'avance sur l'amont (`↑2 ↓1`) demandent de comparer
//!   l'index à l'arbre de travail et de parcourir le graphe de commits. Aucun fichier de
//!   contrôle ne les porte : ils viennent d'un appel à `git`, déclenché par la surveillance
//!   et jamais par la boucle de sonde (voir [`super::git_cli`] et [`super::porcelain`]).
//!
//! De là le [`Status`] **optionnel** : son absence — `git` introuvable, dépôt trop gros
//! pour le délai — n'empêche pas d'afficher la branche
//! ([ADR-0011](../../../../docs/adr/0011-git-domaine-de-premier-plan.md)).

use std::path::Path;

use super::control::{control_line, optional_line};
use super::error::GitError;
use super::ports::{Entry, FileSystem};

/// Longueur d'un identifiant de commit abrégé, à la mode de git.
const SHORT_COMMIT: usize = 7;

/// Profondeur maximale d'un parcours de `refs/`.
///
/// Une branche peut porter des `/` (`feat/git/watch`), donc des sous-dossiers, mais pas
/// dix niveaux. La borne protège d'une boucle de liens symboliques : le parcours suit les
/// dossiers, et un dossier qui se contient lui-même ferait tourner la lecture sans fin.
const MAX_REF_DEPTH: usize = 8;

/// Ce que la ligne de statut et la sidebar affichent d'un worktree.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct WorktreeMetadata {
    pub head: Head,
    /// L'opération git en cours, quand il y en a une. C'est elle qui l'emporte à
    /// l'affichage : pendant un rebase, `HEAD` est détaché et ne dit plus rien d'utile.
    pub operation: Option<Operation>,
    /// Ce que seul `git status` sait dire, quand il a pu le dire.
    ///
    /// `None` n'est pas une erreur : c'est « on ne sait pas encore », et ça se rend en
    /// n'affichant ni `+3 ~1` ni `↑2 ↓1`. Le reste de la ligne, lui, est toujours là.
    pub status: Option<Status>,
}

/// L'état d'un worktree tel que `git status --porcelain=v2 --branch` le décrit.
///
/// Les deux moitiés viennent du **même** appel : les compter séparément coûterait deux
/// processus pour une seule ligne d'affichage.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub tree: TreeStatus,
    /// La comparaison avec la branche amont. `None` quand la branche n'en suit aucune —
    /// une branche locale toute neuve, ou un `HEAD` détaché.
    pub upstream: Option<Upstream>,
}

/// `+3 ~1` : des **nombres de fichiers**, jamais de lignes.
///
/// La maquette n'en affiche que deux ; les quatre sont là parce qu'ils viennent du même
/// appel et qu'ils disent des choses différentes. Ce qui s'affiche est l'affaire de la
/// ligne de statut, pas du backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TreeStatus {
    /// Fichiers ajoutés à l'index, **et** fichiers non suivis : le `+` de la maquette.
    pub added: u32,
    /// Fichiers modifiés, renommés ou copiés : le `~`.
    pub modified: u32,
    /// Fichiers supprimés. Pas dans la maquette, mais dans le modèle : le savoir sans
    /// l'afficher coûte zéro, le redemander plus tard coûterait un appel de plus.
    pub deleted: u32,
    /// Fichiers en conflit — un merge ou un rebase arrêté attend une décision dessus.
    pub conflicted: u32,
}

impl TreeStatus {
    /// Rien à signaler : l'arbre est propre.
    pub fn is_clean(&self) -> bool {
        *self == Self::default()
    }
}

/// `↑2 ↓1` : l'avance et le retard sur la branche amont.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Upstream {
    pub ahead: u32,
    pub behind: u32,
}

/// Où pointe `HEAD`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Head {
    /// Sur une branche — le cas ordinaire. `name` est le nom court (`feat/watch`).
    ///
    /// C'est aussi ce que rend une branche **non née** : `HEAD` désigne
    /// `refs/heads/main` avant le premier commit, et afficher `main` y est la vérité.
    Branch { name: String },
    /// Détaché : `HEAD` porte un identifiant de commit, abrégé ici.
    Detached { commit: String },
}

/// L'opération git en cours dans ce worktree.
///
/// Elle est **propre au worktree** : deux worktrees du même dépôt peuvent avoir un rebase
/// dans l'un et rien dans l'autre ([ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md)).
/// C'est pourquoi elle se lit dans le dossier git du worktree, pas dans le dépôt commun.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub kind: OperationKind,
    /// La branche que l'opération déplace (`refs/heads/feat` → `feat`), quand git la nomme.
    pub branch: Option<String>,
    /// Le point d'arrivée : un nom de branche quand un ref le désigne, l'identifiant
    /// abrégé sinon. C'est le `main` de `rebasing onto main`.
    pub onto: Option<String>,
    /// Où en est l'opération. Absente pour un merge, qui n'a pas d'étapes.
    pub progress: Option<Progress>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum OperationKind {
    /// `git rebase`, quel que soit son moteur — `rebase-merge` ou `rebase-apply`.
    Rebase,
    /// `git am` : des patchs appliqués un à un, hors de tout rebase.
    Am,
    /// Un merge arrêté sur conflit — `MERGE_HEAD` traîne jusqu'au commit ou à l'abandon.
    Merge,
}

/// `2/5` : l'étape en cours et le total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub step: u32,
    pub total: u32,
}

/// Lit les métadonnées d'un worktree.
///
/// `git_dir` est le dossier git **propre** au worktree (`…/.git/worktrees/<nom>` pour un
/// worktree lié) : c'est là que vivent `HEAD`, `MERGE_HEAD` et les dossiers de rebase.
/// `common_dir` est le dossier git **partagé**, où vivent les refs — c'est lui qui permet
/// de mettre un nom sur l'identifiant de commit vers lequel on rebase.
pub fn read_metadata(
    fs: &dyn FileSystem,
    git_dir: &Path,
    common_dir: &Path,
) -> Result<WorktreeMetadata, GitError> {
    Ok(WorktreeMetadata {
        head: read_head(fs, git_dir)?,
        operation: read_operation(fs, git_dir, common_dir),
        // Rempli par qui sait lancer `git` — la surveillance. Ce module ne lance rien.
        status: None,
    })
}

fn read_head(fs: &dyn FileSystem, git_dir: &Path) -> Result<Head, GitError> {
    let line = control_line(fs, &git_dir.join("HEAD"))?;
    Ok(match line.strip_prefix("ref:").map(str::trim) {
        Some(reference) => Head::Branch {
            name: short_ref(reference),
        },
        None => Head::Detached {
            commit: short_commit(&line),
        },
    })
}

/// L'opération en cours, dans l'ordre où git lui-même les distingue.
///
/// Les trois dossiers ne coexistent pas : git refuse de démarrer un rebase pendant un
/// merge, et réciproquement. L'ordre n'arbitre donc rien, il évite seulement trois
/// lectures quand la première a répondu.
fn read_operation(fs: &dyn FileSystem, git_dir: &Path, common_dir: &Path) -> Option<Operation> {
    let rebase_merge = git_dir.join("rebase-merge");
    if is_dir(fs, &rebase_merge) {
        // Le moteur « merge », défaut de `git rebase` depuis git 2.26 — y compris sans
        // `-i` : le fichier `interactive` y est présent dans les deux cas, il ne dit donc
        // pas ce qu'on croirait, et n'est pas lu.
        return Some(Operation {
            kind: OperationKind::Rebase,
            branch: optional_line(fs, &rebase_merge.join("head-name")).map(|r| short_ref(&r)),
            onto: named_commit(fs, common_dir, &rebase_merge.join("onto")),
            progress: read_progress(fs, &rebase_merge.join("msgnum"), &rebase_merge.join("end")),
        });
    }

    let rebase_apply = git_dir.join("rebase-apply");
    if is_dir(fs, &rebase_apply) {
        // Le moteur historique, encore utilisé par `git rebase --apply` et par `git am`.
        // Seul `git am` y pose le drapeau `applying` — c'est ce qui les sépare.
        let applying = fs.entry(&rebase_apply.join("applying")).is_some();
        return Some(Operation {
            kind: if applying {
                OperationKind::Am
            } else {
                OperationKind::Rebase
            },
            branch: optional_line(fs, &rebase_apply.join("head-name")).map(|r| short_ref(&r)),
            onto: named_commit(fs, common_dir, &rebase_apply.join("onto")),
            progress: read_progress(fs, &rebase_apply.join("next"), &rebase_apply.join("last")),
        });
    }

    let merge_head = git_dir.join("MERGE_HEAD");
    if fs.entry(&merge_head).is_some() {
        return Some(Operation {
            kind: OperationKind::Merge,
            branch: None,
            // `MERGE_HEAD` porte lui-même le commit fusionné : le nom se retrouve dans
            // les refs, comme pour un rebase.
            onto: named_commit(fs, common_dir, &merge_head),
            progress: None,
        });
    }

    None
}

/// La progression, seulement si les deux bornes sont lisibles et cohérentes.
///
/// Un `2/0` ou un `2/` affiché serait plus inquiétant que pas de progression du tout :
/// l'utilisateur lit cette ligne pour savoir où il en est d'un rebase.
fn read_progress(fs: &dyn FileSystem, step: &Path, total: &Path) -> Option<Progress> {
    let step = optional_line(fs, step)?.parse().ok()?;
    let total = optional_line(fs, total)?.parse().ok()?;
    (total > 0 && step <= total).then_some(Progress { step, total })
}

/// Le nom du commit désigné par un fichier de contrôle, ou son identifiant abrégé.
fn named_commit(fs: &dyn FileSystem, common_dir: &Path, path: &Path) -> Option<String> {
    let commit = optional_line(fs, path)?;
    Some(ref_named(fs, common_dir, &commit).unwrap_or_else(|| short_commit(&commit)))
}

/// Le nom de ref qui désigne ce commit, s'il en existe un.
///
/// Les branches locales d'abord — `rebasing onto main` est plus parlant que
/// `rebasing onto origin/main` quand les deux pointent au même endroit.
///
/// Les refs se lisent de **deux** façons, et les deux comptent : un fichier par ref, et le
/// `packed-refs` où `git gc` les empaquette. Un dépôt fraîchement compacté n'a plus une
/// seule ref en fichier ; ne lire que `refs/` y perdrait tous les noms.
fn ref_named(fs: &dyn FileSystem, common_dir: &Path, commit: &str) -> Option<String> {
    let packed = packed_refs(fs, common_dir);
    loose_ref(fs, &common_dir.join("refs/heads"), commit, 0)
        .or_else(|| named_in(&packed, "refs/heads/", commit))
        .or_else(|| loose_ref(fs, &common_dir.join("refs/remotes"), commit, 0))
        .or_else(|| named_in(&packed, "refs/remotes/", commit))
}

/// Le premier fichier de ref, sous `dir`, dont le contenu est ce commit.
fn loose_ref(fs: &dyn FileSystem, dir: &Path, commit: &str, depth: usize) -> Option<String> {
    if depth > MAX_REF_DEPTH {
        return None;
    }
    let mut entries = fs.list_dir(dir);
    // `list_dir` ne promet pas d'ordre, et deux branches peuvent pointer sur le même
    // commit : sans tri, le nom affiché changerait d'un rafraîchissement à l'autre.
    entries.sort();
    entries.iter().find_map(|entry| {
        let name = entry.strip_prefix(dir).ok()?.to_string_lossy().into_owned();
        match fs.entry(entry)? {
            Entry::Directory => {
                loose_ref(fs, entry, commit, depth + 1).map(|nested| format!("{name}/{nested}"))
            }
            Entry::File => (optional_line(fs, entry)? == commit).then_some(name),
        }
    })
}

fn packed_refs(fs: &dyn FileSystem, common_dir: &Path) -> Vec<(String, String)> {
    let Ok(content) = fs.read_to_string(&common_dir.join("packed-refs")) else {
        return Vec::new();
    };
    content
        .lines()
        // `#` en tête est l'en-tête du fichier, `^` une ligne de commit pelé (un tag
        // annoté) : ni l'un ni l'autre ne nomme un commit.
        .filter(|line| !line.starts_with('#') && !line.starts_with('^'))
        .filter_map(|line| {
            let (commit, reference) = line.split_once(' ')?;
            Some((commit.trim().to_owned(), reference.trim().to_owned()))
        })
        .collect()
}

fn named_in(packed: &[(String, String)], prefix: &str, commit: &str) -> Option<String> {
    packed
        .iter()
        .filter(|(_, reference)| reference.starts_with(prefix))
        .find(|(packed_commit, _)| packed_commit == commit)
        .and_then(|(_, reference)| reference.strip_prefix(prefix))
        .map(str::to_owned)
}

fn short_ref(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_owned()
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(SHORT_COMMIT).collect()
}

fn is_dir(fs: &dyn FileSystem, path: &Path) -> bool {
    matches!(fs.entry(path), Some(Entry::Directory))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::fake_fs::FakeFs;

    /// Test Data Builder : un dossier git tel que git l'écrit.
    ///
    /// Défauts valides et déterministes — une branche `main`, aucune opération en cours,
    /// deux refs en fichiers. Un scénario n'ajoute que ce qu'il regarde.
    struct RepoBuilder {
        root: String,
        fs: FakeFs,
    }

    impl RepoBuilder {
        fn new() -> Self {
            let root = "/dev/ash".to_owned();
            let fs = FakeFs::new()
                .plain_repo(&root)
                .file("/dev/ash/.git/HEAD", "ref: refs/heads/main\n")
                .file("/dev/ash/.git/refs/heads/main", &format!("{MAIN}\n"))
                .file("/dev/ash/.git/refs/heads/feat", &format!("{FEAT}\n"));
            Self { root, fs }
        }

        fn head(mut self, content: &str) -> Self {
            self.fs.write("/dev/ash/.git/HEAD", content);
            self
        }

        fn file(mut self, path: &str, content: &str) -> Self {
            self.fs.write(&format!("/dev/ash/.git/{path}"), content);
            self
        }

        /// `git gc` : plus une seule ref en fichier, tout est dans `packed-refs`.
        fn packed(mut self) -> Self {
            self.fs.remove("/dev/ash/.git/refs/heads");
            self.fs.write(
                "/dev/ash/.git/packed-refs",
                &format!(
                    "# pack-refs with: peeled fully-peeled sorted \n\
                     {MAIN} refs/heads/main\n{FEAT} refs/heads/feat\n"
                ),
            );
            self
        }

        fn read(&self) -> WorktreeMetadata {
            let git_dir = std::path::PathBuf::from(format!("{}/.git", self.root));
            read_metadata(&self.fs, &git_dir, &git_dir).unwrap()
        }
    }

    const MAIN: &str = "80eca445d2118da8da44686ec2f7047789b5da1f";
    const FEAT: &str = "367a1b2178a37787b1b334e9d88a858c21885e3d";

    #[test]
    fn given_a_head_on_a_branch_when_reading_the_metadata_then_it_reports_the_short_branch_name() {
        // Given
        let repo = RepoBuilder::new().head("ref: refs/heads/feat/git/watch\n");

        // When
        let metadata = repo.read();

        // Then — le `refs/heads/` n'a rien à faire dans une sidebar de 240 px
        assert_eq!(
            metadata.head,
            Head::Branch {
                name: "feat/git/watch".to_owned()
            }
        );
        assert_eq!(metadata.operation, None);
    }

    #[test]
    fn given_a_detached_head_when_reading_the_metadata_then_it_reports_the_abbreviated_commit() {
        // Given — un `git checkout <sha>`, ou un rebase abandonné en chemin
        let repo = RepoBuilder::new().head(&format!("{FEAT}\n"));

        // When
        let metadata = repo.read();

        // Then
        assert_eq!(
            metadata.head,
            Head::Detached {
                commit: "367a1b2".to_owned()
            }
        );
    }

    #[test]
    fn given_a_rebase_stopped_on_a_conflict_when_reading_the_metadata_then_it_reports_the_target_branch_and_the_step(
    ) {
        // Given — ce que git écrit vraiment : `HEAD` détaché sur le `onto`, la branche
        // rebasée dans `head-name`, et un `onto` qui est un identifiant, pas un nom
        let repo = RepoBuilder::new()
            .head(&format!("{MAIN}\n"))
            .file("rebase-merge/head-name", "refs/heads/feat\n")
            .file("rebase-merge/onto", &format!("{MAIN}\n"))
            .file("rebase-merge/msgnum", "2\n")
            .file("rebase-merge/end", "5\n");

        // When
        let metadata = repo.read();

        // Then — de quoi écrire `rebasing onto main · 2/5` sans rien deviner
        assert_eq!(
            metadata.operation,
            Some(Operation {
                kind: OperationKind::Rebase,
                branch: Some("feat".to_owned()),
                onto: Some("main".to_owned()),
                progress: Some(Progress { step: 2, total: 5 }),
            })
        );
    }

    #[test]
    fn given_a_repository_whose_refs_have_been_packed_when_a_rebase_is_running_then_the_target_is_still_named(
    ) {
        // Given — après un `git gc`, `refs/heads/main` n'existe plus comme fichier
        let repo = RepoBuilder::new()
            .packed()
            .head(&format!("{MAIN}\n"))
            .file("rebase-merge/onto", &format!("{MAIN}\n"))
            .file("rebase-merge/msgnum", "1\n")
            .file("rebase-merge/end", "3\n");

        // When
        let metadata = repo.read();

        // Then — sans lire `packed-refs`, Ash afficherait `rebasing onto 80eca44`
        assert_eq!(
            metadata.operation.and_then(|operation| operation.onto),
            Some("main".to_owned())
        );
    }

    #[test]
    fn given_a_rebase_onto_a_commit_that_no_ref_designates_when_reading_then_it_falls_back_to_the_abbreviated_commit(
    ) {
        // Given — rebase sur un commit intermédiaire (`--onto HEAD~3`)
        let repo = RepoBuilder::new()
            .file(
                "rebase-merge/onto",
                "deadbeefcafe1234567890abcdef1234567890ab\n",
            )
            .file("rebase-merge/msgnum", "1\n")
            .file("rebase-merge/end", "2\n");

        // When
        let metadata = repo.read();

        // Then
        assert_eq!(
            metadata.operation.and_then(|operation| operation.onto),
            Some("deadbee".to_owned())
        );
    }

    #[test]
    fn given_a_rebase_run_by_the_apply_engine_when_reading_the_metadata_then_the_step_is_read_from_its_own_files(
    ) {
        // Given — `git rebase --apply` nomme ses bornes `next` et `last`, pas `msgnum`
        // et `end` : les lire au mauvais endroit ferait disparaître la progression
        let repo = RepoBuilder::new()
            .file("rebase-apply/head-name", "refs/heads/feat\n")
            .file("rebase-apply/next", "1\n")
            .file("rebase-apply/last", "3\n");

        // When
        let metadata = repo.read();

        // Then
        assert_eq!(
            metadata.operation,
            Some(Operation {
                kind: OperationKind::Rebase,
                branch: Some("feat".to_owned()),
                onto: None,
                progress: Some(Progress { step: 1, total: 3 }),
            })
        );
    }

    #[test]
    fn given_a_git_am_in_progress_when_reading_the_metadata_then_it_is_not_called_a_rebase() {
        // Given — même dossier que `rebase --apply`, avec le drapeau `applying` en plus
        let repo = RepoBuilder::new()
            .file("rebase-apply/applying", "")
            .file("rebase-apply/next", "2\n")
            .file("rebase-apply/last", "4\n");

        // When
        let metadata = repo.read();

        // Then
        assert_eq!(
            metadata.operation.map(|operation| operation.kind),
            Some(OperationKind::Am)
        );
    }

    #[test]
    fn given_a_merge_stopped_on_a_conflict_when_reading_the_metadata_then_it_names_the_merged_branch(
    ) {
        // Given
        let repo = RepoBuilder::new().file("MERGE_HEAD", &format!("{FEAT}\n"));

        // When
        let metadata = repo.read();

        // Then — un merge n'a pas d'étapes, et sa branche courante reste celle de `HEAD`
        assert_eq!(
            metadata.operation,
            Some(Operation {
                kind: OperationKind::Merge,
                branch: None,
                onto: Some("feat".to_owned()),
                progress: None,
            })
        );
        assert_eq!(
            metadata.head,
            Head::Branch {
                name: "main".to_owned()
            }
        );
    }

    #[test]
    fn given_a_rebase_whose_step_files_are_unreadable_when_reading_then_no_progress_is_invented() {
        // Given — git écrit `msgnum` et `end` l'un après l'autre : les deux instants où
        // l'un manque sont réels, et un `2/0` affiché serait pire qu'un silence
        let repo = RepoBuilder::new().file("rebase-merge/msgnum", "2\n");

        // When
        let metadata = repo.read();

        // Then
        assert_eq!(
            metadata.operation.map(|operation| operation.progress),
            Some(None)
        );
    }

    #[test]
    fn given_a_worktree_whose_head_cannot_be_read_when_reading_then_it_fails_instead_of_guessing() {
        // Given — un dossier git tronqué ; afficher une branche vide serait un mensonge
        let fs = FakeFs::new().dir("/dev/ash/.git");

        // When
        let read = read_metadata(&fs, Path::new("/dev/ash/.git"), Path::new("/dev/ash/.git"));

        // Then
        assert_eq!(
            read,
            Err(GitError::Io {
                path: std::path::PathBuf::from("/dev/ash/.git/HEAD"),
                why: "aucun fichier".to_owned(),
            })
        );
    }

    #[test]
    fn given_a_linked_worktree_when_reading_then_the_operation_comes_from_its_own_git_dir_and_the_names_from_the_common_one(
    ) {
        // Given — deux worktrees du même dépôt : un rebase dans l'un, rien dans l'autre
        // (ADR-0012). Lire l'opération dans le dépôt commun les confondrait.
        let fs = FakeFs::new()
            .repo_hosting("/dev/ash", &["sidebar"])
            .file("/dev/ash/.git/refs/heads/main", &format!("{MAIN}\n"))
            .file("/dev/ash/.git/HEAD", "ref: refs/heads/main\n")
            .file("/dev/ash/.git/worktrees/sidebar/HEAD", &format!("{MAIN}\n"))
            .file(
                "/dev/ash/.git/worktrees/sidebar/rebase-merge/head-name",
                "refs/heads/feat\n",
            )
            .file(
                "/dev/ash/.git/worktrees/sidebar/rebase-merge/onto",
                &format!("{MAIN}\n"),
            );

        // When
        let linked = read_metadata(
            &fs,
            Path::new("/dev/ash/.git/worktrees/sidebar"),
            Path::new("/dev/ash/.git"),
        )
        .unwrap();
        let main =
            read_metadata(&fs, Path::new("/dev/ash/.git"), Path::new("/dev/ash/.git")).unwrap();

        // Then
        assert_eq!(
            linked.operation.and_then(|operation| operation.onto),
            Some("main".to_owned())
        );
        assert_eq!(main.operation, None);
    }
}
