//! La surveillance des métadonnées git, worktree par worktree.
//!
//! Elle applique la spec §5.3 mot pour mot : relecture **au rattachement d'un onglet**,
//! **au focus de la fenêtre**, et **à la modification** d'un fichier de contrôle ; jamais
//! par sondage, et au plus une fois toutes les 5 s par worktree.
//!
//! Un observateur **par worktree**, pas par onglet : cinq dépôts ouverts font cinq
//! abonnements FSEvents, quel que soit le nombre de terminaux. Au repos — aucune écriture
//! dans `.git` — rien ne tourne : ni fil, ni minuteur, ni lecture de fichier.
//!
//! Ce module détient l'état ; le frontend le rend
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use super::metadata::{read_metadata, WorktreeMetadata};
use super::ports::FileSystem;
use super::targets::WatchTargets;
use super::throttle::{Decision, Throttle};
use super::time::{Clock, Scheduler};
use super::watcher::{FileWatcher, WatchHandle};
use super::worktree::resolve_worktree;

/// Ce que la surveillance annonce : un worktree, et son état git à cet instant.
pub type Announce = Arc<dyn Fn(&Path, &WorktreeMetadata) + Send + Sync>;

/// Un worktree observé.
struct Watched {
    /// Le dossier git **propre** au worktree : son `HEAD`, son rebase.
    git_dir: PathBuf,
    /// Le dossier git **commun** : les refs, partagées avec les worktrees frères.
    common_dir: PathBuf,
    targets: WatchTargets,
    throttle: Throttle,
    /// Le dernier état annoncé, pour ne pas réveiller la webview deux fois pour le même.
    last: Option<WorktreeMetadata>,
    /// L'abonnement. Jamais lu : c'est son `Drop` qui compte.
    _handle: Box<dyn WatchHandle>,
}

/// La surveillance, pour tous les worktrees où vit au moins un onglet.
pub struct MetadataWatch {
    fs: Arc<dyn FileSystem>,
    watcher: Arc<dyn FileWatcher>,
    clock: Arc<dyn Clock>,
    scheduler: Arc<dyn Scheduler>,
    interval: Duration,
    announce: Announce,
    /// Trié : deux passes successives comparent leurs clés, et l'ordre rend la
    /// comparaison exacte sans rien allouer de plus.
    watched: Mutex<BTreeMap<PathBuf, Watched>>,
}

impl MetadataWatch {
    pub fn new(
        fs: Arc<dyn FileSystem>,
        watcher: Arc<dyn FileWatcher>,
        clock: Arc<dyn Clock>,
        scheduler: Arc<dyn Scheduler>,
        interval: Duration,
        announce: Announce,
    ) -> Arc<Self> {
        Arc::new(Self {
            fs,
            watcher,
            clock,
            scheduler,
            interval,
            announce,
            watched: Mutex::new(BTreeMap::new()),
        })
    }

    /// Aligne les worktrees observés sur ceux où vit un onglet.
    ///
    /// C'est le **rattachement** de la spec §5.3 : un worktree qui apparaît est lu tout de
    /// suite, un worktree que plus aucun onglet n'habite est relâché — et son abonnement
    /// avec lui. Appelée souvent, elle ne touche au disque **que** si l'ensemble a changé.
    pub fn follow(self: &Arc<Self>, roots: &[String]) {
        let wanted: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();

        let arriving = {
            let Ok(mut watched) = self.watched.lock() else {
                return;
            };
            if watched.len() == wanted.len() && wanted.iter().all(|root| watched.contains_key(root))
            {
                return;
            }
            watched.retain(|root, _| wanted.contains(root));
            wanted
                .into_iter()
                .filter(|root| !watched.contains_key(root))
                .collect::<Vec<_>>()
        };

        for root in arriving {
            self.start_watching(root);
        }
    }

    /// La fenêtre a repris le focus : tout ce qui est observé est relu.
    ///
    /// Un dépôt peut avoir bougé pendant qu'Ash était derrière une autre fenêtre — un
    /// commit fait dans un IDE, un `git pull` dans un autre terminal. Les événements ont
    /// bien été reçus, mais la limitation les a peut-être différés ; passer par elle ici
    /// aussi garantit qu'aucun focus répété ne relit plus d'une fois par fenêtre.
    pub fn on_focus(self: &Arc<Self>) {
        let roots: Vec<PathBuf> = match self.watched.lock() {
            Ok(watched) => watched.keys().cloned().collect(),
            Err(_) => return,
        };
        for root in roots {
            self.request(&root);
        }
    }

    /// Le dernier état connu d'un worktree — ce que le frontend demande en s'affichant.
    ///
    /// Rend `None` pour un worktree hors de tout dépôt, ou qu'on ne sait pas lire. Un
    /// worktree qui n'est pas encore observé est lu une fois, sans être abonné : c'est au
    /// rattachement d'un onglet de décider ce qui mérite un observateur, pas à un affichage.
    pub fn metadata(&self, root: &Path) -> Option<WorktreeMetadata> {
        if let Ok(watched) = self.watched.lock() {
            if let Some(entry) = watched.get(root) {
                return entry.last.clone();
            }
        }
        let (git_dir, common_dir) = self.dirs_of(root)?;
        read_metadata(self.fs.as_ref(), &git_dir, &common_dir).ok()
    }

    /// Relâche tous les observateurs — l'application quitte.
    ///
    /// Sans ça, l'arrêt ne serait qu'un effet de bord de la fin du processus, c'est-à-dire
    /// rien du tout le jour où la même bibliothèque tournera sous le démon `ashd`.
    pub fn stop(&self) {
        if let Ok(mut watched) = self.watched.lock() {
            watched.clear();
        }
    }

    fn start_watching(self: &Arc<Self>, root: PathBuf) {
        let Some((git_dir, common_dir)) = self.dirs_of(&root) else {
            return;
        };
        let targets = WatchTargets::for_worktree(&git_dir, &common_dir);

        // Le rappel ne tient pas la surveillance en vie : elle possède l'abonnement, qui
        // possède le rappel. Un `Arc` ici serait un cycle, et la surveillance ne serait
        // jamais relâchée.
        let weak = Arc::downgrade(self);
        let watched_root = root.clone();
        let on_change = Arc::new(move |changed: &Path| {
            if let Some(watch) = Weak::upgrade(&weak) {
                watch.on_change(&watched_root, changed);
            }
        });

        let Ok(handle) = self.watcher.watch(targets.roots(), on_change) else {
            return;
        };

        {
            let Ok(mut watched) = self.watched.lock() else {
                return;
            };
            watched.insert(
                root.clone(),
                Watched {
                    git_dir,
                    common_dir,
                    targets,
                    throttle: Throttle::new(self.interval),
                    last: None,
                    _handle: handle,
                },
            );
        }

        self.request(&root);
    }

    fn on_change(self: &Arc<Self>, root: &Path, changed: &Path) {
        let concerns = match self.watched.lock() {
            Ok(watched) => watched
                .get(root)
                .is_some_and(|entry| entry.targets.concerns(changed)),
            Err(_) => false,
        };
        if concerns {
            self.request(root);
        }
    }

    /// Une demande de relecture, passée par la limitation de débit.
    fn request(self: &Arc<Self>, root: &Path) {
        let decision = {
            let Ok(mut watched) = self.watched.lock() else {
                return;
            };
            let Some(entry) = watched.get_mut(root) else {
                return;
            };
            entry.throttle.request(self.clock.now())
        };

        match decision {
            Decision::Now => self.refresh(root),
            Decision::In(delay) => {
                let watch = Arc::clone(self);
                let root = root.to_owned();
                self.scheduler
                    .after(delay, Box::new(move || watch.deferred(&root)));
            }
            Decision::Pending => {}
        }
    }

    /// Le rafraîchissement différé arrive à échéance.
    fn deferred(self: &Arc<Self>, root: &Path) {
        let due = {
            let Ok(mut watched) = self.watched.lock() else {
                return;
            };
            watched
                .get_mut(root)
                .is_some_and(|entry| entry.throttle.due(self.clock.now()))
        };
        if due {
            self.refresh(root);
        }
    }

    /// Relit les fichiers de contrôle et annonce ce qui a changé.
    ///
    /// La lecture a lieu **ici**, au moment du rafraîchissement : c'est ce qui fait que le
    /// dernier état gagne, même quand la demande a été différée pendant que git écrivait.
    fn refresh(&self, root: &Path) {
        let Some((git_dir, common_dir)) = self.dirs_of_watched(root) else {
            return;
        };
        // Lue hors du verrou : la lecture touche au disque, et une frappe clavier n'a pas
        // à attendre derrière elle.
        let Ok(metadata) = read_metadata(self.fs.as_ref(), &git_dir, &common_dir) else {
            return;
        };

        let changed = {
            let Ok(mut watched) = self.watched.lock() else {
                return;
            };
            let Some(entry) = watched.get_mut(root) else {
                return;
            };
            if entry.last.as_ref() == Some(&metadata) {
                false
            } else {
                entry.last = Some(metadata.clone());
                true
            }
        };

        if changed {
            (self.announce)(root, &metadata);
        }
    }

    fn dirs_of_watched(&self, root: &Path) -> Option<(PathBuf, PathBuf)> {
        let watched = self.watched.lock().ok()?;
        let entry = watched.get(root)?;
        Some((entry.git_dir.clone(), entry.common_dir.clone()))
    }

    /// Les deux dossiers git d'un worktree : le sien, et celui du dépôt commun.
    ///
    /// La résolution est celle de la feature ; rien n'est re-parsé ici. Un worktree hors
    /// de tout dépôt n'a pas de dossier git, et n'a donc rien à surveiller.
    fn dirs_of(&self, root: &Path) -> Option<(PathBuf, PathBuf)> {
        let located = resolve_worktree(self.fs.as_ref(), root).ok()?;
        let git_dir = located.worktree.git_dir?;
        let common_dir = located
            .repo
            .map_or_else(|| git_dir.clone(), |repo| repo.git_dir);
        Some((git_dir, common_dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::fakes::{ControlledTime, RecordedAnnounces, WatchedTree};
    use crate::features::git::metadata::Head;

    /// Test Data Builder : une surveillance branchée sur un arbre en mémoire, une horloge
    /// qu'on avance à la main et des reports qu'on déclenche soi-même.
    ///
    /// Défauts valides et déterministes : un dépôt `/dev/ash` sur `main`, la fenêtre de
    /// 5 s de la spec, et aucun worktree suivi.
    struct WatchBuilder {
        tree: Arc<WatchedTree>,
        time: Arc<ControlledTime>,
        announces: Arc<RecordedAnnounces>,
    }

    impl WatchBuilder {
        fn new() -> Self {
            Self {
                tree: WatchedTree::with_repos(&["/dev/ash"]),
                time: ControlledTime::new(),
                announces: RecordedAnnounces::new(),
            }
        }

        fn with_repos(repos: &[&str]) -> Self {
            let builder = Self::new();
            Self {
                tree: WatchedTree::with_repos(repos),
                ..builder
            }
        }

        fn build(&self) -> Arc<MetadataWatch> {
            MetadataWatch::new(
                Arc::clone(&self.tree) as Arc<dyn FileSystem>,
                Arc::clone(&self.tree) as Arc<dyn FileWatcher>,
                Arc::clone(&self.time) as Arc<dyn Clock>,
                Arc::clone(&self.time) as Arc<dyn Scheduler>,
                Duration::from_secs(5),
                self.announces.announce(),
            )
        }
    }

    fn branch_of(metadata: &WorktreeMetadata) -> String {
        match &metadata.head {
            Head::Branch { name } => name.clone(),
            Head::Detached { commit } => commit.clone(),
        }
    }

    #[test]
    fn given_a_tab_attaching_to_a_worktree_when_it_starts_being_followed_then_its_state_is_announced_at_once(
    ) {
        // Given
        let world = WatchBuilder::new();
        let watch = world.build();

        // When — le rattachement d'ADR-0012 : un onglet vient de se situer
        watch.follow(&["/dev/ash".to_owned()]);

        // Then — la sidebar n'attend pas cinq secondes pour dire sur quelle branche on est
        assert_eq!(
            world.announces.branches(),
            vec![("/dev/ash".to_owned(), "main".to_owned())]
        );
    }

    #[test]
    fn given_a_burst_of_git_writes_when_they_arrive_within_the_window_then_one_refresh_happens_and_the_last_state_wins(
    ) {
        // Given — un worktree suivi, puis un rebase qui écrit sa progression par étapes
        let world = WatchBuilder::new();
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned()]);
        world.announces.forget();

        // When — trois écritures dans la fenêtre de 5 s, dont la dernière est la vérité
        world.tree.set_head("/dev/ash", "ref: refs/heads/feat");
        world.tree.touch("/dev/ash/.git/HEAD");
        world.tree.set_head("/dev/ash", "ref: refs/heads/other");
        world.tree.touch("/dev/ash/.git/HEAD");
        world.tree.set_head("/dev/ash", "ref: refs/heads/final");
        world.tree.touch("/dev/ash/.git/HEAD");
        world.time.advance(Duration::from_secs(5));
        world.time.fire_due();

        // Then — un seul rafraîchissement, et c'est le dernier état qui est annoncé :
        // l'événement reçu pendant la fenêtre n'a pas été perdu, il a été différé
        assert_eq!(
            world.announces.branches(),
            vec![("/dev/ash".to_owned(), "final".to_owned())]
        );
    }

    #[test]
    fn given_a_write_that_touches_nothing_watched_when_it_arrives_then_nothing_is_reread() {
        // Given
        let world = WatchBuilder::new();
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned()]);
        let reads = world.tree.reads();
        world.announces.forget();

        // When — un objet écrit par un `git fetch`, dans le dossier git surveillé
        world.tree.touch("/dev/ash/.git/objects/ab/cdef01");
        world.time.advance(Duration::from_secs(10));
        world.time.fire_due();

        // Then — pas une lecture de plus, et rien qui traverse la frontière
        assert_eq!(world.tree.reads(), reads);
        assert!(world.announces.branches().is_empty());
    }

    #[test]
    fn given_five_repositories_being_followed_when_nothing_writes_to_disk_then_nothing_is_read_at_all(
    ) {
        // Given — le critère « avec 5 dépôts ouverts, la consommation CPU au repos reste
        // négligeable ». Au repos, une surveillance de fichiers ne coûte rien ; un
        // sondage, lui, coûterait cinq lectures par tour. C'est ce que ce test empêche
        // de réintroduire.
        let repos = ["/dev/a", "/dev/b", "/dev/c", "/dev/d", "/dev/e"];
        let world = WatchBuilder::with_repos(&repos);
        let watch = world.build();
        let roots: Vec<String> = repos.iter().map(|repo| (*repo).to_owned()).collect();
        watch.follow(&roots);
        let reads_after_attach = world.tree.reads();

        // When — le temps passe, la surveillance reste en place, le disque se tait
        world.time.advance(Duration::from_secs(600));
        world.time.fire_due();
        watch.follow(&roots);

        // Then — cinq abonnements, un par worktree, et aucune lecture de plus
        assert_eq!(world.tree.subscriptions(), 5);
        assert_eq!(world.tree.reads(), reads_after_attach);
        assert_eq!(world.announces.branches().len(), 5);
    }

    #[test]
    fn given_a_worktree_no_tab_lives_in_anymore_when_the_set_is_synced_then_its_watcher_stops() {
        // Given — deux worktrees suivis
        let world = WatchBuilder::with_repos(&["/dev/ash", "/dev/other"]);
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned(), "/dev/other".to_owned()]);

        // When — le dernier onglet de `/dev/other` se ferme
        watch.follow(&["/dev/ash".to_owned()]);

        // Then — son observateur est relâché, et ses écritures ne réveillent plus rien
        assert_eq!(world.tree.subscriptions(), 1);
        world.announces.forget();
        world.tree.set_head("/dev/other", "ref: refs/heads/feat");
        world.tree.touch("/dev/other/.git/HEAD");
        world.time.advance(Duration::from_secs(10));
        world.time.fire_due();
        assert!(world.announces.branches().is_empty());
    }

    #[test]
    fn given_the_application_quits_when_the_watch_stops_then_no_subscription_survives_it() {
        // Given
        let world = WatchBuilder::new();
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned()]);

        // When
        watch.stop();

        // Then
        assert_eq!(world.tree.subscriptions(), 0);
    }

    #[test]
    fn given_a_state_that_has_not_moved_when_a_watched_file_is_rewritten_then_the_frontend_is_not_woken_up(
    ) {
        // Given — git réécrit `HEAD` à l'identique plus souvent qu'on ne croit
        let world = WatchBuilder::new();
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned()]);
        world.announces.forget();

        // When
        world.time.advance(Duration::from_secs(10));
        world.tree.touch("/dev/ash/.git/HEAD");

        // Then
        assert!(world.announces.branches().is_empty());
    }

    #[test]
    fn given_a_directory_outside_any_repository_when_a_tab_attaches_to_it_then_nothing_is_watched()
    {
        // Given — un onglet ouvert dans `~/Downloads` est un cas nominal
        let world = WatchBuilder::new();
        let watch = world.build();

        // When
        watch.follow(&["/dev/notes".to_owned()]);

        // Then — rien à surveiller, et rien à annoncer : pas de dossier git
        assert_eq!(world.tree.subscriptions(), 0);
        assert!(world.announces.branches().is_empty());
        assert_eq!(watch.metadata(Path::new("/dev/notes")), None);
    }

    #[test]
    fn given_a_followed_worktree_when_the_frontend_asks_for_its_state_then_it_gets_the_last_one_read(
    ) {
        // Given — le frontend rend un état, il ne le détient pas (ADR-0009) : ce qu'il
        // lit au montage doit être ce que la surveillance a déjà annoncé
        let world = WatchBuilder::new();
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned()]);

        // When
        let metadata = watch.metadata(Path::new("/dev/ash"));

        // Then
        assert_eq!(metadata.as_ref().map(branch_of), Some("main".to_owned()));
    }

    #[test]
    fn given_the_window_regains_focus_when_the_repository_moved_behind_our_back_then_it_is_reread()
    {
        // Given — un commit fait dans un IDE pendant qu'Ash était en arrière-plan : sur
        // certains dépôts distants ou volumes réseau, FSEvents ne dit rien
        let world = WatchBuilder::new();
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned()]);
        world.announces.forget();
        world.tree.set_head("/dev/ash", "ref: refs/heads/feat");
        world.time.advance(Duration::from_secs(10));

        // When
        watch.on_focus();

        // Then
        assert_eq!(
            world.announces.branches(),
            vec![("/dev/ash".to_owned(), "feat".to_owned())]
        );
    }

    #[test]
    fn given_a_focus_right_after_a_refresh_when_it_happens_again_and_again_then_the_window_still_holds(
    ) {
        // Given — passer d'une fenêtre à l'autre est le geste le plus courant du produit ;
        // sans limitation, chaque aller-retour relirait `n` dépôts
        let world = WatchBuilder::with_repos(&["/dev/ash", "/dev/other"]);
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned(), "/dev/other".to_owned()]);
        let reads_after_attach = world.tree.reads();

        // When — trois focus dans la même seconde
        world.time.advance(Duration::from_secs(1));
        watch.on_focus();
        watch.on_focus();
        watch.on_focus();

        // Then — rien n'est relu tout de suite ; un seul report par worktree attend
        assert_eq!(world.tree.reads(), reads_after_attach);
        assert_eq!(world.time.pending(), 2);
    }
}
