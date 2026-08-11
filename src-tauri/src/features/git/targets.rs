//! Ce qu'on surveille dans un dépôt, et ce qu'on en ignore.
//!
//! La règle de la spec §5.3 est « surveillance de fichiers, pas de sondage ». Elle se
//! traduit par deux questions, toutes les deux tranchées ici et testables sans disque :
//! **quels dossiers** confier à l'observateur, et **quel changement** mérite une relecture.

use std::path::{Path, PathBuf};

/// Un dossier confié à l'observateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRoot {
    pub path: PathBuf,
    /// Sous-dossiers compris.
    pub recursive: bool,
}

/// Les dossiers à surveiller pour un worktree, et le filtre qui va avec.
///
/// Dans un worktree lié, tout n'est pas au même endroit : `HEAD` et les dossiers de rebase
/// vivent dans le dossier git **propre** au worktree, tandis que `refs/` et `packed-refs`
/// vivent dans le dossier **commun**, partagé avec ses frères
/// ([ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md)). Surveiller un seul
/// des deux laisserait la moitié des changements passer inaperçus.
#[derive(Debug, Clone)]
pub struct WatchTargets {
    git_dir: PathBuf,
    common_dir: PathBuf,
    roots: Vec<WatchRoot>,
}

impl WatchTargets {
    pub fn for_worktree(git_dir: &Path, common_dir: &Path) -> Self {
        // Le dossier git du worktree est pris **récursivement** : la progression d'un
        // rebase s'écrit dans `rebase-merge/msgnum`, un cran plus bas, et ce dossier
        // n'existe pas encore au moment où l'on s'abonne — on ne peut donc pas l'ajouter
        // comme racine à part. Le prix est de recevoir aussi les écritures d'objets d'un
        // dépôt classique ; [`Self::concerns`] les jette pour quelques comparaisons de
        // chemin, sans jamais toucher au disque.
        let mut roots = vec![WatchRoot {
            path: git_dir.to_owned(),
            recursive: true,
        }];

        if common_dir != git_dir {
            // Worktree lié : les refs sont ailleurs. `packed-refs` est un fichier à la
            // racine du dossier commun, d'où la racine non récursive qui l'accompagne.
            roots.push(WatchRoot {
                path: common_dir.to_owned(),
                recursive: false,
            });
            roots.push(WatchRoot {
                path: common_dir.join("refs"),
                recursive: true,
            });
        }

        Self {
            git_dir: git_dir.to_owned(),
            common_dir: common_dir.to_owned(),
            roots,
        }
    }

    pub fn roots(&self) -> &[WatchRoot] {
        &self.roots
    }

    /// Ce changement peut-il modifier ce qu'Ash affiche ?
    ///
    /// La liste est celle de la spec §5.3, plus `rebase-apply` — son équivalent pour
    /// `git rebase --apply` et `git am`, que la spec ne nomme pas mais qui produit
    /// exactement le même état affiché.
    pub fn concerns(&self, changed: &Path) -> bool {
        // Un `.lock` est l'écriture *en cours* : git renomme ensuite le fichier définitif,
        // et c'est ce renommage qui compte. Le laisser passer doublerait chaque
        // changement — sans conséquence grâce à la limitation de débit, mais en lisant un
        // état à moitié écrit.
        if changed.extension().is_some_and(|suffix| suffix == "lock") {
            return false;
        }

        // FSEvents remonte parfois le **dossier** plutôt que le fichier modifié : un
        // dossier surveillé qui bouge est donc pris au sérieux.
        if self.roots.iter().any(|root| root.path == changed) {
            return true;
        }

        relative(&self.git_dir, changed).is_some_and(inside_git_dir)
            || relative(&self.common_dir, changed).is_some_and(inside_common_dir)
    }
}

/// Les fichiers propres au worktree : sa branche, son merge, son rebase.
fn inside_git_dir(relative: &Path) -> bool {
    relative == Path::new("HEAD")
        || relative == Path::new("MERGE_HEAD")
        || relative.starts_with("rebase-merge")
        || relative.starts_with("rebase-apply")
}

/// Les refs, partagées par tous les worktrees du dépôt — et leur forme empaquetée.
fn inside_common_dir(relative: &Path) -> bool {
    relative.starts_with("refs") || relative == Path::new("packed-refs")
}

fn relative<'a>(base: &Path, path: &'a Path) -> Option<&'a Path> {
    path.strip_prefix(base).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : les deux formes de dépôt, et rien d'autre à choisir.
    struct TargetsBuilder;

    impl TargetsBuilder {
        /// Un dépôt sans worktree lié : dossier git et dossier commun confondus.
        fn plain() -> WatchTargets {
            WatchTargets::for_worktree(Path::new("/dev/ash/.git"), Path::new("/dev/ash/.git"))
        }

        /// Un worktree lié : son dossier git est sous celui du dépôt.
        fn linked() -> WatchTargets {
            WatchTargets::for_worktree(
                Path::new("/dev/ash/.git/worktrees/sidebar"),
                Path::new("/dev/ash/.git"),
            )
        }
    }

    #[test]
    fn given_a_linked_worktree_when_choosing_what_to_watch_then_it_covers_its_own_git_dir_and_the_shared_refs(
    ) {
        // Given / When
        let targets = TargetsBuilder::linked();

        // Then — les refs d'un worktree lié ne sont pas sous son dossier git : les
        // oublier ferait rater tout changement de branche fait depuis un frère
        let roots: Vec<_> = targets
            .roots()
            .iter()
            .map(|root| (root.path.display().to_string(), root.recursive))
            .collect();
        assert_eq!(
            roots,
            vec![
                ("/dev/ash/.git/worktrees/sidebar".to_owned(), true),
                ("/dev/ash/.git".to_owned(), false),
                ("/dev/ash/.git/refs".to_owned(), true),
            ]
        );
    }

    #[test]
    fn given_a_plain_repository_when_choosing_what_to_watch_then_one_root_is_enough() {
        // Given / When — dossier git et dossier commun sont le même : deux racines
        // imbriquées feraient arriver chaque changement deux fois
        let targets = TargetsBuilder::plain();

        // Then
        assert_eq!(targets.roots().len(), 1);
        assert!(targets.roots()[0].recursive);
    }

    #[test]
    fn given_the_files_of_the_spec_when_they_change_then_the_metadata_is_reread() {
        // Given
        let targets = TargetsBuilder::linked();

        // When / Then — la liste de §5.3, dans les deux dossiers où elle se répartit
        for changed in [
            "/dev/ash/.git/worktrees/sidebar/HEAD",
            "/dev/ash/.git/worktrees/sidebar/MERGE_HEAD",
            "/dev/ash/.git/worktrees/sidebar/rebase-merge/msgnum",
            "/dev/ash/.git/worktrees/sidebar/rebase-apply/next",
            "/dev/ash/.git/refs/heads/main",
            "/dev/ash/.git/packed-refs",
        ] {
            assert!(targets.concerns(Path::new(changed)), "{changed}");
        }
    }

    #[test]
    fn given_a_write_that_changes_nothing_visible_when_it_arrives_then_no_reread_is_triggered() {
        // Given — un dépôt classique, dont le dossier git est surveillé en entier :
        // c'est là qu'arrivent les écritures d'objets, de journaux et d'index
        let targets = TargetsBuilder::plain();

        // When / Then
        for changed in [
            "/dev/ash/.git/objects/ab/cdef",
            "/dev/ash/.git/logs/HEAD",
            "/dev/ash/.git/index",
            "/dev/ash/.git/COMMIT_EDITMSG",
            "/dev/other/.git/HEAD",
        ] {
            assert!(!targets.concerns(Path::new(changed)), "{changed}");
        }
    }

    #[test]
    fn given_the_lock_file_of_a_ref_being_written_when_it_appears_then_it_is_not_mistaken_for_the_ref(
    ) {
        // Given — git écrit `refs/heads/main.lock`, puis le renomme. Lire pendant
        // l'écriture, c'est lire un fichier à moitié écrit.
        let targets = TargetsBuilder::plain();

        // When / Then
        assert!(!targets.concerns(Path::new("/dev/ash/.git/refs/heads/main.lock")));
        assert!(targets.concerns(Path::new("/dev/ash/.git/refs/heads/main")));
    }

    #[test]
    fn given_a_directory_level_event_when_it_names_a_watched_root_then_it_is_taken_seriously() {
        // Given — FSEvents ne promet pas de nommer le fichier : il peut ne remonter que
        // le dossier. Le jeter ferait manquer un changement de branche.
        let targets = TargetsBuilder::linked();

        // When / Then
        assert!(targets.concerns(Path::new("/dev/ash/.git/worktrees/sidebar")));
        assert!(targets.concerns(Path::new("/dev/ash/.git/refs")));
    }
}
