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
    /// Le dossier où le dépôt déclare ses worktrees liés.
    ///
    /// Son **contenu** — et non ce qu'il y a dedans — décide de la forme d'affichage
    /// d'ADR-0012 : un dépôt qui n'a rien à y montrer s'affiche à plat, les autres se
    /// groupent. C'est la seule information d'un dépôt qui ne soit pas une métadonnée.
    worktrees_dir: PathBuf,
    roots: Vec<WatchRoot>,
}

impl WatchTargets {
    pub fn for_worktree(git_dir: &Path, common_dir: &Path) -> Self {
        let worktrees_dir = common_dir.join("worktrees");
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
            // Un **frère** qui apparaît ou disparaît s'écrit dans `worktrees/`, hors de
            // tout ce qui précède : le dossier git de ce worktree-ci ne couvre que le sien,
            // et la racine non récursive du dossier commun s'arrête à ses enfants directs.
            // Non récursive elle aussi : c'est la **liste** des frères qui est surveillée,
            // pas ce que chacun écrit chez lui.
            //
            // Dans un dépôt sans worktree lié, cette racine n'existe pas encore — c'est
            // justement le dossier que `git worktree add` crée — mais elle est déjà couverte
            // par la surveillance récursive du dossier git, qui la contient.
            roots.push(WatchRoot {
                path: worktrees_dir.clone(),
                recursive: false,
            });
        }

        Self {
            git_dir: git_dir.to_owned(),
            common_dir: common_dir.to_owned(),
            worktrees_dir,
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

    /// Ce changement peut-il avoir changé la **forme** du dépôt ?
    ///
    /// Un worktree lié qui apparaît ou disparaît s'écrit dans `<commun>/worktrees/<nom>`,
    /// et rien d'autre ne le déclare. La question est distincte de [`Self::concerns`] :
    /// gagner un frère ne change ni la branche, ni l'opération en cours, ni l'état de
    /// l'arbre — ça change l'endroit où la sidebar range l'onglet
    /// ([ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md)). Les confondre
    /// ferait payer un `git status` à chaque écriture d'un frère.
    ///
    /// La profondeur 1 suffit, et c'est ce qui garde le filtre étroit : ce qui se passe
    /// **dans** le dossier d'un frère — son `HEAD`, son index, ses journaux — ne change pas
    /// le nombre de frères. FSEvents remonte bien les deux chemins qu'on attend ici : sur
    /// un `git worktree add`, `worktrees/` puis `worktrees/<nom>` arrivent, en plus des
    /// écritures plus profondes qu'on jette.
    pub fn concerns_layout(&self, changed: &Path) -> bool {
        changed == self.worktrees_dir || changed.parent() == Some(self.worktrees_dir.as_path())
    }

    /// Ce changement peut-il être la **naissance d'un commit** ?
    ///
    /// Le journal d'attribution d'
    /// [ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md) demande de
    /// « surveiller `.git/logs/HEAD` par dépôt, pas sonder `git log` », et c'est là que la
    /// question se pose : `logs/HEAD` est le reflog de **ce** worktree, où git écrit une
    /// ligne à chaque déplacement de `HEAD` — un commit, mais aussi un `checkout`, un
    /// `reset`, un `pull`.
    ///
    /// Troisième question, et troisième réponse indépendante, pour la même raison que
    /// [`Self::concerns_layout`] : un commit ne change ni la branche, ni l'opération en
    /// cours, et il n'entre pas dans la limitation de débit des métadonnées — un commit
    /// manqué n'est pas un affichage en retard, c'est une attribution perdue pour toujours.
    ///
    /// **Le reflog n'est pas dans [`Self::concerns`]**, et il n'a pas à y entrer : il ne
    /// change rien de ce que la ligne de statut affiche. Le test qui l'exclut est plus vieux
    /// que ce journal, et il reste vrai.
    pub fn concerns_commits(&self, changed: &Path) -> bool {
        // Le reflog du worktree, dans **son** dossier git : un worktree lié a le sien, et
        // c'est bien celui-là qu'il faut lire — `HEAD` y est propre à lui (ADR-0012).
        relative(&self.git_dir, changed) == Some(Path::new("logs/HEAD"))
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
                ("/dev/ash/.git/worktrees".to_owned(), false),
            ]
        );
    }

    #[test]
    fn given_a_repository_that_gains_a_linked_worktree_when_the_entry_appears_then_its_shape_is_known_to_have_changed(
    ) {
        // Given — un dépôt à plat : `worktrees/` n'existe pas encore, et c'est `git
        // worktree add` qui le crée. C'est le scénario où un onglet déjà ouvert doit
        // rejoindre un groupe sans que son `cwd` ne bouge (ADR-0012).
        let targets = TargetsBuilder::plain();

        // When / Then — le dossier lui-même, puis l'entrée du worktree qui y naît
        assert!(targets.concerns_layout(Path::new("/dev/ash/.git/worktrees")));
        assert!(targets.concerns_layout(Path::new("/dev/ash/.git/worktrees/toc")));
    }

    #[test]
    fn given_a_sibling_worktree_that_writes_in_its_own_git_dir_when_it_does_then_the_shape_is_not_reconsidered(
    ) {
        // Given — un agent qui travaille dans un frère écrit son index et son `HEAD` sans
        // arrêt. Reconsidérer la forme du dépôt à chaque écriture rendrait la résolution à
        // la boucle de sonde, par la porte de derrière.
        let targets = TargetsBuilder::linked();

        // When / Then
        assert!(!targets.concerns_layout(Path::new("/dev/ash/.git/worktrees/toc/HEAD")));
        assert!(!targets.concerns_layout(Path::new("/dev/ash/.git/worktrees/toc/index")));
        assert!(!targets.concerns_layout(Path::new("/dev/ash/.git/refs/heads/main")));
    }

    #[test]
    fn given_a_linked_worktree_when_a_sibling_appears_beside_it_then_the_change_is_within_watch_reach(
    ) {
        // Given — vu d'un worktree lié, le dossier des frères n'est sous aucune des racines
        // que les métadonnées demandent : sans racine à lui, l'événement n'arriverait jamais.
        let targets = TargetsBuilder::linked();

        // When
        let watches_siblings = targets
            .roots()
            .iter()
            .any(|root| root.path == Path::new("/dev/ash/.git/worktrees"));

        // Then
        assert!(watches_siblings);
        assert!(targets.concerns_layout(Path::new("/dev/ash/.git/worktrees/toc")));
    }

    #[test]
    fn given_a_worktree_when_its_reflog_grows_then_a_commit_may_have_been_born_there() {
        // Given — ADR-0014 : « surveiller `.git/logs/HEAD` par dépôt, pas sonder
        // `git log` ». Dans un worktree lié, le reflog qui compte est **le sien** : celui
        // du dépôt commun raconte l'histoire d'un frère.
        let linked = TargetsBuilder::linked();
        let plain = TargetsBuilder::plain();

        // When / Then
        assert!(linked.concerns_commits(Path::new("/dev/ash/.git/worktrees/sidebar/logs/HEAD")));
        assert!(plain.concerns_commits(Path::new("/dev/ash/.git/logs/HEAD")));
        assert!(!linked.concerns_commits(Path::new("/dev/ash/.git/logs/HEAD")));
    }

    #[test]
    fn given_the_reflog_being_written_when_its_lock_appears_then_no_commit_is_read_yet() {
        // Given — git écrit `logs/HEAD.lock` avant de renommer. Lire à ce moment-là, c'est
        // lancer un `git log` sur un dépôt à moitié écrit, et pour rien : le renommage
        // arrivera.
        let targets = TargetsBuilder::plain();

        // When / Then
        assert!(!targets.concerns_commits(Path::new("/dev/ash/.git/logs/HEAD.lock")));
        assert!(!targets.concerns_commits(Path::new("/dev/ash/.git/logs/refs/heads/main")));
        assert!(!targets.concerns_commits(Path::new("/dev/ash/.git/HEAD")));
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
