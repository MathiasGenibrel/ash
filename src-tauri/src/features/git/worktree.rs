//! Résolution d'un `cwd` vers son worktree et son dépôt commun.
//!
//! La hiérarchie d'[ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md) est
//! à trois niveaux — dépôt → worktree → onglets — mais le niveau du dépôt n'apparaît que
//! quand il groupe quelque chose. C'est toute la subtilité de ce module.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::error::GitError;
use super::ports::{Entry, FileSystem};

/// Le worktree — l'unité de travail à laquelle un onglet se rattache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// La racine de l'arbre de travail : le dossier qui porte le `.git`.
    pub root: PathBuf,
    /// Le nom de son dossier.
    ///
    /// C'est la matière du suffixe `·sidebar` de la sidebar, pas le suffixe lui-même :
    /// composer la chaîne d'affichage est le travail de l'affichage.
    pub name: String,
    /// Le dossier git **propre** à ce worktree — `…/.git` pour un dépôt classique,
    /// `…/.git/worktrees/<nom>` pour un worktree lié.
    ///
    /// C'est là que vivent son `HEAD` et ses fichiers de rebase, pas dans le dépôt commun.
    /// `None` quand le `cwd` est hors de tout dépôt.
    pub git_dir: Option<PathBuf>,
}

/// Le dépôt commun — un **groupe d'affichage**, sans onglets en propre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// Le dossier git commun (le `commondir`). C'est lui qui identifie le dépôt : deux
    /// worktrees du même projet rendent exactement le même chemin.
    pub git_dir: PathBuf,
    /// La racine de travail du dépôt — le parent du `.git` commun, ou le dossier git
    /// lui-même pour un dépôt nu.
    pub root: PathBuf,
    /// Le nom du dépôt, tiré de son dossier.
    pub name: String,
}

/// Où un `cwd` se situe dans la hiérarchie d'ADR-0012 : le worktree qui le porte, et le
/// dépôt sous lequel ce worktree se range — quand il y en a un.
///
/// Le mot `Workspace` est délibérément **évité** ici : ADR-0012 l'a retiré du vocabulaire
/// en renommant en « worktree » le workspace d'ADR-0004. Le réutiliser pour le couple
/// worktree + dépôt lui donnerait un troisième sens, dans le module même qui applique ce
/// renommage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeLocation {
    /// Toujours présent : même hors de tout dépôt, un `cwd` est un worktree.
    pub worktree: Worktree,
    /// Le dépôt commun, **seulement quand il groupe réellement**.
    ///
    /// `None` veut dire « affiche ce worktree à plat » : c'est le cas d'un dépôt sans
    /// worktree lié, et celui d'un dossier hors dépôt. Le frontend n'a donc rien à
    /// deviner — il rend un niveau ou deux selon que ce champ est là.
    ///
    /// Un dépôt qui a hébergé un worktree lié reste groupé — donc affiché sur deux
    /// niveaux, avec un seul enfant — jusqu'au prochain `git worktree prune` : git garde
    /// l'entrée dans `worktrees/` tant qu'on ne l'élague pas. C'est visible, et c'est le
    /// prix de ne rien inventer par-dessus ce que le dépôt déclare.
    pub repo: Option<Repo>,
}

/// Résout un `cwd` vers son worktree et, s'il en forme un groupe, son dépôt commun.
///
/// La marche est en trois temps :
///
/// 1. remonter jusqu'au premier `.git` — il donne la racine du **worktree**, et rien de
///    plus : dans un worktree lié, c'est un fichier ;
/// 2. en tirer le dossier git propre au worktree (`gitdir: …` s'il s'agit d'un fichier) ;
/// 3. remonter au `commondir` pour trouver le **dépôt**.
///
/// Échoue plutôt que de deviner : un `cwd` illisible, un fichier de contrôle sans chemin
/// ou pointant dans le vide donnent une [`GitError`], jamais un worktree sans dépôt.
pub fn resolve_worktree(fs: &dyn FileSystem, cwd: &Path) -> Result<WorktreeLocation, GitError> {
    // Canonicaliser une fois, au départ : tout ce qui suit est comparé et groupé par
    // chemin, et deux chemins qui désignent le même dossier doivent donner le même dépôt.
    let from = fs
        .canonicalize(cwd)
        .ok_or_else(|| GitError::UnreadablePath(cwd.to_owned()))?;

    let Some((root, entry)) = nearest_git_entry(fs, &from) else {
        // Hors de tout dépôt : le dossier est son propre worktree, et il n'a rien
        // au-dessus de lui.
        return Ok(WorktreeLocation {
            worktree: Worktree {
                name: folder_name(&from),
                root: from,
                git_dir: None,
            },
            repo: None,
        });
    };

    let git_dir = git_dir_of(fs, &root, entry)?;
    let common_dir = common_dir_of(fs, &git_dir)?;

    Ok(WorktreeLocation {
        worktree: Worktree {
            name: folder_name(&root),
            root,
            git_dir: Some(git_dir.clone()),
        },
        repo: groups_worktrees(fs, &git_dir, &common_dir).then(|| repo_at(common_dir)),
    })
}

/// Le premier ancêtre — `from` compris — qui porte un `.git`.
///
/// Le premier, donc : un dépôt imbriqué (une dépendance vendorée, un thème) l'emporte sur
/// celui qui le contient, comme le fait git lui-même.
fn nearest_git_entry(fs: &dyn FileSystem, from: &Path) -> Option<(PathBuf, Entry)> {
    from.ancestors().find_map(|dir| {
        fs.entry(&dir.join(".git"))
            .map(|entry| (dir.to_owned(), entry))
    })
}

/// Le dossier git propre au worktree, à partir de son `.git`.
fn git_dir_of(fs: &dyn FileSystem, root: &Path, entry: Entry) -> Result<PathBuf, GitError> {
    let git_path = root.join(".git");
    match entry {
        // Dépôt classique : le `.git` *est* le dossier git.
        Entry::Directory => Ok(git_path),
        // Worktree lié : le `.git` est un fichier qui pointe ailleurs.
        Entry::File => {
            let line = control_line(fs, &git_path)?;
            let target = line
                .strip_prefix("gitdir:")
                .map(str::trim)
                .filter(|target| !target.is_empty())
                .ok_or_else(|| GitError::Malformed(git_path.clone()))?;
            // Un `gitdir:` relatif s'entend depuis le dossier qui porte le `.git`. Git en
            // écrit dans les dépôts déclarés portables (`git worktree add --relative-paths`).
            resolve_against(fs, root, target, &git_path)
        }
    }
}

/// Le dossier git **commun**, à partir du dossier git d'un worktree.
///
/// Le fichier `commondir` n'existe que dans le dossier git d'un worktree lié ; sans lui,
/// le dossier git est déjà le dossier commun.
fn common_dir_of(fs: &dyn FileSystem, git_dir: &Path) -> Result<PathBuf, GitError> {
    let marker = git_dir.join("commondir");
    if fs.entry(&marker).is_none() {
        return Ok(git_dir.to_owned());
    }

    let target = control_line(fs, &marker)?;
    // Git y écrit un chemin relatif au dossier git du worktree — le plus souvent `../..`.
    resolve_against(fs, git_dir, &target, &marker)
}

/// Ce dépôt commun forme-t-il un groupe, ou s'affiche-t-il à plat ?
///
/// Deux façons d'en former un : être vu depuis un worktree lié (le dossier git du
/// worktree diffère alors du dossier commun), ou héberger soi-même des worktrees liés —
/// c'est le cas du worktree **principal**, dont le `.git` est un dossier mais qui doit
/// tout de même se ranger sous son dépôt, aux côtés de ses frères.
///
/// Le test porte sur `worktrees/`, et pas sur un décompte : git y laisse l'entrée d'un
/// worktree supprimé jusqu'au prochain `prune`. Un dépôt qui a servi restera donc groupé
/// jusque-là — c'est visible, sans conséquence, et la détection du `stale` est une autre
/// affaire.
fn groups_worktrees(fs: &dyn FileSystem, git_dir: &Path, common_dir: &Path) -> bool {
    git_dir != common_dir || fs.has_entries(&common_dir.join("worktrees"))
}

fn repo_at(common_dir: PathBuf) -> Repo {
    // Un dépôt nu n'a pas de `.git` : son dossier git *est* sa racine, et son nom porte
    // le suffixe `.git` qu'on ne veut pas afficher.
    let root = if common_dir.file_name() == Some(OsStr::new(".git")) {
        common_dir.parent().map(Path::to_owned)
    } else {
        None
    };
    let root = root.unwrap_or_else(|| common_dir.clone());
    let name = folder_name(&root);
    let name = name.strip_suffix(".git").unwrap_or(&name).to_owned();

    Repo {
        git_dir: common_dir,
        root,
        name,
    }
}

fn resolve_against(
    fs: &dyn FileSystem,
    base: &Path,
    target: &str,
    named_by: &Path,
) -> Result<PathBuf, GitError> {
    let target = Path::new(target);
    let joined = if target.is_absolute() {
        target.to_owned()
    } else {
        base.join(target)
    };
    fs.canonicalize(&joined).ok_or(GitError::Dangling {
        at: named_by.to_owned(),
        target: joined,
    })
}

/// La ligne utile d'un fichier de contrôle git — `.git` d'un worktree lié, `commondir`.
///
/// Ces fichiers portent tous la même forme : **une** ligne, un chemin, parfois un
/// préfixe. La règle est ici une fois pour toutes — première ligne, espaces retirés,
/// jamais vide — plutôt qu'une fois par fichier lu : c'est ce qui garantit que le
/// prochain (`worktrees/<nom>/gitdir`, `rebase-merge/head-name`) sera lu à l'identique,
/// et échouera de la même manière.
fn control_line(fs: &dyn FileSystem, path: &Path) -> Result<String, GitError> {
    let content = fs.read_to_string(path).map_err(|why| GitError::Io {
        path: path.to_owned(),
        why,
    })?;
    let line = content.lines().next().unwrap_or_default().trim();
    if line.is_empty() {
        return Err(GitError::Malformed(path.to_owned()));
    }
    Ok(line.to_owned())
}

/// Le nom du dossier, avec le chemin entier pour seul repli — un dossier sans nom est la
/// racine du système, et l'afficher vide serait pire que verbeux.
fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::fake_fs::FakeFs;

    fn resolve(fs: &FakeFs, cwd: &str) -> Result<WorktreeLocation, GitError> {
        resolve_worktree(fs, Path::new(cwd))
    }

    #[test]
    fn given_a_worktree_git_file_when_resolving_then_it_finds_the_worktree_and_the_common_repo() {
        // Given — un worktree lié, dont le `.git` est un fichier : remonter jusqu'à lui
        // donne la racine du worktree, et ne dit rien du dépôt.
        let tree = FakeFs::new()
            .repo_hosting("/dev/ash", &["sidebar"])
            .linked_worktree(
                "/wt/ash-sidebar",
                "gitdir: /dev/ash/.git/worktrees/sidebar\n",
            )
            .dir("/wt/ash-sidebar/src");

        // When
        let location = resolve(&tree, "/wt/ash-sidebar/src").unwrap();

        // Then
        assert_eq!(location.worktree.root, Path::new("/wt/ash-sidebar"));
        assert_eq!(location.worktree.name, "ash-sidebar");
        assert_eq!(
            location.worktree.git_dir.as_deref(),
            Some(Path::new("/dev/ash/.git/worktrees/sidebar"))
        );
        let repo = location
            .repo
            .expect("un worktree lié appartient à un dépôt");
        assert_eq!(repo.git_dir, Path::new("/dev/ash/.git"));
        assert_eq!(repo.root, Path::new("/dev/ash"));
        assert_eq!(repo.name, "ash");
    }

    #[test]
    fn given_a_repository_without_any_linked_worktree_when_resolving_then_it_stays_flat() {
        // Given
        let tree = FakeFs::new()
            .plain_repo("/dev/solo")
            .dir("/dev/solo/src/deep");

        // When
        let location = resolve(&tree, "/dev/solo/src/deep").unwrap();

        // Then — un seul niveau visible : pas de groupe à afficher au-dessus.
        assert_eq!(location.repo, None);
        assert_eq!(location.worktree.root, Path::new("/dev/solo"));
        assert_eq!(location.worktree.name, "solo");
        assert_eq!(
            location.worktree.git_dir.as_deref(),
            Some(Path::new("/dev/solo/.git"))
        );
    }

    #[test]
    fn given_the_main_worktree_of_a_repository_that_hosts_linked_worktrees_when_resolving_then_it_is_grouped_too(
    ) {
        // Given — son `.git` est un dossier, comme un dépôt à plat, mais il a des frères :
        // le rendre à plat les séparerait dans la sidebar.
        let tree = FakeFs::new().repo_hosting("/dev/ash", &["sidebar", "toc"]);

        // When
        let location = resolve(&tree, "/dev/ash").unwrap();

        // Then
        let repo = location.repo.expect("le worktree principal a des frères");
        assert_eq!(repo.git_dir, Path::new("/dev/ash/.git"));
        assert_eq!(location.worktree.root, Path::new("/dev/ash"));
    }

    #[test]
    fn given_two_worktrees_of_the_same_project_when_resolving_both_then_they_report_the_same_repository(
    ) {
        // Given — c'est ce qui fait que la sidebar les range ensemble.
        let tree = FakeFs::new()
            .repo_hosting("/dev/ash", &["sidebar", "toc"])
            .linked_worktree("/wt/ash-sidebar", "gitdir: /dev/ash/.git/worktrees/sidebar")
            .linked_worktree("/wt/ash-toc", "gitdir: /dev/ash/.git/worktrees/toc");

        // When
        let sidebar = resolve(&tree, "/wt/ash-sidebar").unwrap();
        let toc = resolve(&tree, "/wt/ash-toc").unwrap();

        // Then
        assert_eq!(sidebar.repo, toc.repo);
        assert_ne!(sidebar.worktree, toc.worktree);
    }

    #[test]
    fn given_a_cwd_outside_any_repository_when_resolving_then_it_is_a_worktree_without_repository()
    {
        // Given
        let tree = FakeFs::new().dir("/dev/notes/drafts");

        // When
        let location = resolve(&tree, "/dev/notes/drafts").unwrap();

        // Then — le dossier est son propre worktree, et il n'a rien au-dessus.
        assert_eq!(location.repo, None);
        assert_eq!(location.worktree.root, Path::new("/dev/notes/drafts"));
        assert_eq!(location.worktree.name, "drafts");
        assert_eq!(location.worktree.git_dir, None);
    }

    #[test]
    fn given_a_relative_gitdir_when_resolving_then_it_is_read_from_the_worktree_root() {
        // Given — `git worktree add --relative-paths` en écrit, et un `gitdir:` relatif
        // pris depuis le mauvais dossier mène silencieusement ailleurs.
        let tree = FakeFs::new()
            .repo_hosting("/dev/ash", &["sidebar"])
            .linked_worktree(
                "/dev/wt/ash-sidebar",
                "gitdir: ../../ash/.git/worktrees/sidebar",
            );

        // When
        let location = resolve(&tree, "/dev/wt/ash-sidebar").unwrap();

        // Then
        assert_eq!(
            location.worktree.git_dir.as_deref(),
            Some(Path::new("/dev/ash/.git/worktrees/sidebar"))
        );
        assert_eq!(
            location.repo.map(|repo| repo.git_dir),
            Some(PathBuf::from("/dev/ash/.git"))
        );
    }

    #[test]
    fn given_a_git_file_without_a_gitdir_line_when_resolving_then_it_reports_a_malformed_file() {
        // Given
        let tree = FakeFs::new().linked_worktree("/wt/broken", "ceci n'est pas un gitdir\n");

        // When
        let resolved = resolve(&tree, "/wt/broken");

        // Then — se taire ici ferait passer un worktree cassé pour un dossier ordinaire.
        assert_eq!(
            resolved,
            Err(GitError::Malformed(PathBuf::from("/wt/broken/.git")))
        );
    }

    #[test]
    fn given_a_git_file_pointing_to_a_missing_gitdir_when_resolving_then_it_reports_a_dangling_worktree(
    ) {
        // Given — le dépôt a été déplacé ou supprimé, le dossier du worktree est resté.
        let tree =
            FakeFs::new().linked_worktree("/wt/orphan", "gitdir: /dev/gone/.git/worktrees/x");

        // When
        let resolved = resolve(&tree, "/wt/orphan");

        // Then
        assert_eq!(
            resolved,
            Err(GitError::Dangling {
                at: PathBuf::from("/wt/orphan/.git"),
                target: PathBuf::from("/dev/gone/.git/worktrees/x"),
            })
        );
    }

    #[test]
    fn given_a_repository_nested_inside_another_when_resolving_then_the_innermost_one_wins() {
        // Given — une dépendance vendorée avec son propre dépôt.
        let tree = FakeFs::new()
            .plain_repo("/dev/ash")
            .plain_repo("/dev/ash/vendor/lib")
            .dir("/dev/ash/vendor/lib/src");

        // When
        let location = resolve(&tree, "/dev/ash/vendor/lib/src").unwrap();

        // Then
        assert_eq!(location.worktree.root, Path::new("/dev/ash/vendor/lib"));
    }

    #[test]
    fn given_a_cwd_that_does_not_exist_when_resolving_then_it_says_so_instead_of_guessing() {
        // Given
        let tree = FakeFs::new().plain_repo("/dev/ash");

        // When
        let resolved = resolve(&tree, "/dev/ash/gone");

        // Then
        assert_eq!(
            resolved,
            Err(GitError::UnreadablePath(PathBuf::from("/dev/ash/gone")))
        );
    }

    #[test]
    fn given_a_bare_repository_hosting_worktrees_when_resolving_one_then_the_repo_name_drops_the_git_suffix(
    ) {
        // Given — le motif « dépôt nu + un worktree par branche », courant avec les agents.
        let tree = FakeFs::new()
            .dir("/dev/ash.git/refs")
            .dir("/dev/ash.git/worktrees/sidebar")
            .file("/dev/ash.git/worktrees/sidebar/commondir", "../..\n")
            .linked_worktree("/dev/wt/sidebar", "gitdir: /dev/ash.git/worktrees/sidebar");

        // When
        let location = resolve(&tree, "/dev/wt/sidebar").unwrap();

        // Then
        let repo = location
            .repo
            .expect("un worktree lié appartient à un dépôt");
        assert_eq!(repo.git_dir, Path::new("/dev/ash.git"));
        assert_eq!(repo.root, Path::new("/dev/ash.git"));
        assert_eq!(repo.name, "ash");
    }
}
