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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use super::git_cli::StatusReader;
use super::metadata::{read_metadata, WorktreeMetadata};
use super::porcelain::parse_status;
use super::ports::FileSystem;
use super::targets::WatchTargets;
use super::throttle::{Decision, Throttle};
use super::watcher::{FileWatcher, WatchHandle};
use super::worktree::resolve_worktree;
use crate::shared::time::{Clock, Scheduler};

/// Ce que la surveillance annonce : un worktree, et son état git à cet instant.
pub type Announce = Arc<dyn Fn(&Path, &WorktreeMetadata) + Send + Sync>;

/// Ce qu'on appelle quand un dépôt surveillé change de **forme** : un worktree lié qui
/// apparaît, ou le dernier qui disparaît.
///
/// Ce n'est pas une métadonnée, et ça ne passe donc ni par [`Announce`] ni par la
/// limitation de débit : ça ne décrit pas un worktree, ça dit qu'une **résolution** faite
/// depuis un dépôt surveillé a pu vieillir. Un dépôt qui gagne son premier worktree lié
/// passe de la forme à plat à la forme groupée
/// ([ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md)) sans qu'aucun `cwd`
/// ne bouge d'un caractère.
///
/// Le signal ne nomme rien : il n'a pas à savoir qui retient des résolutions. C'est le
/// composition root qui le relie à ce qui en garde — aujourd'hui le registre d'onglets —
/// et cette feature-ci ne connaît toujours rien des onglets.
pub type Relocate = Arc<dyn Fn() + Send + Sync>;

/// Ce que la surveillance a à dire, et à qui.
///
/// Les deux rappels voyagent ensemble parce qu'ils répondent à la même chose — une écriture
/// observée dans `.git` — et qu'ils sont posés au même endroit, une fois, par le
/// composition root. Ce sont pourtant deux sorties distinctes : l'une décrit un worktree au
/// frontend, l'autre dit qu'une résolution a vieilli, et personne n'a besoin des deux.
pub struct Listeners {
    /// L'état git d'un worktree a changé.
    pub announce: Announce,
    /// La forme d'un dépôt a changé — voir [`Relocate`].
    pub relocate: Relocate,
}

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

/// Ce qu'on fait de chaque racine que la boucle de sonde nomme : la surveiller, ou y
/// avoir renoncé.
///
/// Les deux ensembles vivent sous le même verrou parce qu'ils répondent à la même
/// question, et qu'une racine passe de l'un à l'autre.
#[derive(Default)]
struct Followed {
    /// Trié : deux passes successives comparent leurs clés, et l'ordre rend la
    /// comparaison exacte sans rien allouer de plus.
    watched: BTreeMap<PathBuf, Watched>,
    /// Les racines qu'on n'a **pas** su surveiller : un onglet ouvert hors de tout dépôt —
    /// un cas nominal — ou un abonnement que le système a refusé.
    ///
    /// Sans elles, `follow`, appelée à chaque passe de la boucle de sonde, retenterait la
    /// résolution trois fois par seconde et pour toute la session : le sondage que la
    /// spec §5.3 écarte, revenu par la porte de derrière.
    declined: BTreeSet<PathBuf>,
}

/// La surveillance, pour tous les worktrees où vit au moins un onglet.
pub struct MetadataWatch {
    fs: Arc<dyn FileSystem>,
    /// L'appel à `git`, pour ce que les fichiers de contrôle ne disent pas.
    status: Arc<dyn StatusReader>,
    watcher: Arc<dyn FileWatcher>,
    clock: Arc<dyn Clock>,
    scheduler: Arc<dyn Scheduler>,
    interval: Duration,
    listeners: Listeners,
    followed: Mutex<Followed>,
}

impl MetadataWatch {
    pub fn new(
        fs: Arc<dyn FileSystem>,
        status: Arc<dyn StatusReader>,
        watcher: Arc<dyn FileWatcher>,
        clock: Arc<dyn Clock>,
        scheduler: Arc<dyn Scheduler>,
        interval: Duration,
        listeners: Listeners,
    ) -> Arc<Self> {
        Arc::new(Self {
            fs,
            status,
            watcher,
            clock,
            scheduler,
            interval,
            listeners,
            followed: Mutex::new(Followed::default()),
        })
    }

    /// Aligne les worktrees observés sur ceux où vit un onglet.
    ///
    /// C'est le **rattachement** de la spec §5.3 : un worktree qui apparaît est lu tout de
    /// suite, un worktree que plus aucun onglet n'habite est relâché — et son abonnement
    /// avec lui. Appelée trois fois par seconde par la boucle de sonde, elle ne touche au
    /// disque **que** pour une racine qu'elle n'a encore ni suivie ni écartée.
    pub fn follow(self: &Arc<Self>, roots: &[String]) {
        let wanted: BTreeSet<PathBuf> = roots.iter().map(PathBuf::from).collect();

        let arriving: Vec<PathBuf> = {
            let Ok(mut followed) = self.followed.lock() else {
                return;
            };
            followed.watched.retain(|root, _| wanted.contains(root));
            // Une racine où plus aucun onglet ne vit est oubliée, y compris quand on avait
            // renoncé à la surveiller : la rouvrir mérite un nouvel essai.
            followed.declined.retain(|root| wanted.contains(root));
            wanted
                .into_iter()
                .filter(|root| {
                    !followed.watched.contains_key(root) && !followed.declined.contains(root)
                })
                .collect()
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
        let roots: Vec<PathBuf> = match self.followed.lock() {
            Ok(mut followed) => {
                // Le focus est aussi le moment de reconsidérer ce qu'on avait écarté : un
                // `git init` a pu avoir lieu pendant qu'Ash était derrière une autre
                // fenêtre. La prochaine passe de la boucle de sonde retentera.
                followed.declined.clear();
                followed.watched.keys().cloned().collect()
            }
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
        if let Ok(followed) = self.followed.lock() {
            if let Some(entry) = followed.watched.get(root) {
                return entry.last.clone();
            }
        }
        let (git_dir, common_dir) = self.dirs_of(root)?;
        self.read(root, &git_dir, &common_dir)
    }

    /// Relâche tous les observateurs — l'application quitte.
    ///
    /// Sans ça, l'arrêt ne serait qu'un effet de bord de la fin du processus, c'est-à-dire
    /// rien du tout le jour où la même bibliothèque tournera sous le démon `ashd`.
    pub fn stop(&self) {
        if let Ok(mut followed) = self.followed.lock() {
            followed.watched.clear();
            followed.declined.clear();
        }
    }

    fn start_watching(self: &Arc<Self>, root: PathBuf) {
        let Some((git_dir, common_dir)) = self.dirs_of(&root) else {
            self.decline(root);
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
            self.decline(root);
            return;
        };

        {
            let Ok(mut followed) = self.followed.lock() else {
                return;
            };
            followed.watched.insert(
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

    /// Renonce à surveiller cette racine — jusqu'au prochain focus, ou jusqu'à ce que plus
    /// aucun onglet n'y vive.
    fn decline(&self, root: PathBuf) {
        if let Ok(mut followed) = self.followed.lock() {
            followed.declined.insert(root);
        }
    }

    /// Une écriture observée : deux questions distinctes, et deux réponses indépendantes.
    ///
    /// L'état du worktree se relit — au débit près. Sa **forme**, elle, ne se relit pas :
    /// elle se signale, et à quelqu'un d'autre.
    fn on_change(self: &Arc<Self>, root: &Path, changed: &Path) {
        let (concerns, shape) = match self.followed.lock() {
            Ok(followed) => followed.watched.get(root).map_or((false, false), |entry| {
                (
                    entry.targets.concerns(changed),
                    entry.targets.concerns_layout(changed),
                )
            }),
            Err(_) => (false, false),
        };

        // Hors du verrou : le rappel repart vers le composition root, qui n'a rien à faire
        // derrière le verrou d'une surveillance de fichiers.
        if shape {
            (self.listeners.relocate)();
        }
        if concerns {
            self.request(root);
        }
    }

    /// Une demande de relecture, passée par la limitation de débit.
    fn request(self: &Arc<Self>, root: &Path) {
        let decision = {
            let Ok(mut followed) = self.followed.lock() else {
                return;
            };
            let Some(entry) = followed.watched.get_mut(root) else {
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
            let Ok(mut followed) = self.followed.lock() else {
                return;
            };
            followed
                .watched
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
        // Lue hors du verrou : elle touche au disque et lance un processus, et une frappe
        // clavier n'a pas à attendre derrière elle.
        let Some(metadata) = self.read(root, &git_dir, &common_dir) else {
            return;
        };

        let changed = {
            let Ok(mut followed) = self.followed.lock() else {
                return;
            };
            let Some(entry) = followed.watched.get_mut(root) else {
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
            (self.listeners.announce)(root, &metadata);
        }
    }

    /// Les métadonnées complètes d'un worktree : les fichiers de contrôle, puis `git`.
    ///
    /// L'ordre compte. La branche et l'opération viennent de `.git` et ne coûtent que
    /// quelques lectures ; l'état de l'arbre coûte un processus, qui peut être lent ou
    /// absent. Son échec ne fait pas échouer le reste — la ligne de statut perd `+3 ~1`,
    /// elle ne perd pas sa branche.
    ///
    /// **Aucun verrou n'est tenu pendant l'appel** : `self.status.read` peut attendre
    /// plusieurs secondes sur un gros dépôt, et le verrou de [`Followed`] est pris par la
    /// surveillance à chaque écriture observée.
    fn read(
        &self,
        worktree_root: &Path,
        git_dir: &Path,
        common_dir: &Path,
    ) -> Option<WorktreeMetadata> {
        let mut metadata = read_metadata(self.fs.as_ref(), git_dir, common_dir).ok()?;
        metadata.status = self.status.read(worktree_root).as_deref().map(parse_status);
        Some(metadata)
    }

    fn dirs_of_watched(&self, root: &Path) -> Option<(PathBuf, PathBuf)> {
        let followed = self.followed.lock().ok()?;
        let entry = followed.watched.get(root)?;
        Some((entry.git_dir.clone(), entry.common_dir.clone()))
    }

    /// Les deux dossiers git d'un worktree : le sien, et celui du dépôt commun.
    ///
    /// La résolution est celle de la feature, et la règle qui répartit les fichiers entre
    /// les deux dossiers appartient à [`WorktreeLocation`] : rien n'est re-décidé ici. Un
    /// worktree hors de tout dépôt n'a pas de dossier git, et n'a donc rien à surveiller.
    fn dirs_of(&self, root: &Path) -> Option<(PathBuf, PathBuf)> {
        resolve_worktree(self.fs.as_ref(), root).ok()?.git_dirs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::fakes::{
        ControlledTime, RecordedAnnounces, RecordedRelocations, WatchedTree,
    };
    use crate::features::git::metadata::{Head, Upstream};

    /// Test Data Builder : une surveillance branchée sur un arbre en mémoire, une horloge
    /// qu'on avance à la main et des reports qu'on déclenche soi-même.
    ///
    /// Défauts valides et déterministes : un dépôt `/dev/ash` sur `main`, la fenêtre de
    /// 5 s de la spec, et aucun worktree suivi.
    struct WatchBuilder {
        tree: Arc<WatchedTree>,
        time: Arc<ControlledTime>,
        announces: Arc<RecordedAnnounces>,
        relocations: Arc<RecordedRelocations>,
    }

    impl WatchBuilder {
        fn new() -> Self {
            Self::with_repos(&["/dev/ash"])
        }

        fn with_repos(repos: &[&str]) -> Self {
            Self {
                tree: WatchedTree::with_repos(repos),
                time: ControlledTime::new(),
                announces: RecordedAnnounces::new(),
                relocations: RecordedRelocations::new(),
            }
        }

        fn build(&self) -> Arc<MetadataWatch> {
            MetadataWatch::new(
                Arc::clone(&self.tree) as Arc<dyn FileSystem>,
                Arc::clone(&self.tree) as Arc<dyn StatusReader>,
                Arc::clone(&self.tree) as Arc<dyn FileWatcher>,
                Arc::clone(&self.time) as Arc<dyn Clock>,
                Arc::clone(&self.time) as Arc<dyn Scheduler>,
                Duration::from_secs(5),
                Listeners {
                    announce: self.announces.announce(),
                    relocate: self.relocations.relocate(),
                },
            )
        }
    }

    /// Ce que `git status --porcelain=v2 --branch` répondrait pour un arbre où deux
    /// fichiers sont apparus, un a changé, et l'amont a deux commits d'avance.
    const BUSY_TREE: &str = "# branch.oid 200f7b93\n\
                             # branch.head main\n\
                             # branch.upstream origin/main\n\
                             # branch.ab +2 -1\n\
                             1 .M N... 100644 100644 100644 4bcfe98e 4bcfe98e mod.txt\n\
                             ? un.txt\n\
                             ? deux.txt\n";

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
    fn given_a_worktree_with_local_changes_when_it_is_followed_then_the_tree_and_the_upstream_travel_with_the_branch(
    ) {
        // Given — `+3 ~1 ↑2 ↓1` : la moitié de la ligne de statut que `.git` ne porte pas
        let world = WatchBuilder::new();
        world.tree.set_porcelain("/dev/ash", BUSY_TREE);
        let watch = world.build();

        // When
        watch.follow(&["/dev/ash".to_owned()]);

        // Then — un seul appel a suffi pour les deux moitiés
        let (_, metadata) = world
            .announces
            .announced()
            .pop()
            .expect("le rattachement annonce l'état du worktree");
        let status = metadata.status.expect("git a répondu");
        assert_eq!(status.tree.added, 2);
        assert_eq!(status.tree.modified, 1);
        assert_eq!(
            status.upstream,
            Some(Upstream {
                ahead: 2,
                behind: 1
            })
        );
        assert_eq!(world.tree.invocations(), 1);
    }

    #[test]
    fn given_git_that_does_not_answer_when_a_worktree_is_followed_then_its_branch_is_still_announced(
    ) {
        // Given — `git` absent du `PATH`, dépôt trop gros pour le délai, sortie en
        // erreur : le double ne répond pas. Perdre la branche pour autant serait perdre
        // la seule information qui ne dépend d'aucun processus.
        let world = WatchBuilder::new();
        let watch = world.build();

        // When
        watch.follow(&["/dev/ash".to_owned()]);

        // Then — la ligne de statut perd `+3 ~1`, elle ne perd pas `main`
        let (_, metadata) = world
            .announces
            .announced()
            .pop()
            .expect("un git muet n'empêche pas d'annoncer le worktree");
        assert_eq!(
            metadata.head,
            Head::Branch {
                name: "main".to_owned()
            }
        );
        assert_eq!(metadata.status, None);
        assert_eq!(world.tree.invocations(), 1);
    }

    #[test]
    fn given_a_file_written_in_the_worktree_when_the_watch_reacts_then_git_is_asked_once_per_window(
    ) {
        // Given — un agent qui écrit produit une rafale : chaque écriture ne doit pas
        // valoir un `fork`. C'est la même limitation que pour la lecture de `.git`.
        let world = WatchBuilder::new();
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned()]);
        let invocations_after_attach = world.tree.invocations();

        // When — trois écritures surveillées dans la fenêtre de 5 s
        for _ in 0..3 {
            world.tree.touch("/dev/ash/.git/HEAD");
        }
        world.tree.set_porcelain("/dev/ash", BUSY_TREE);
        world.time.advance(Duration::from_secs(5));
        world.time.fire_due();

        // Then — un seul appel de plus, et il a lu le dernier état
        assert_eq!(world.tree.invocations(), invocations_after_attach + 1);
        let (_, metadata) = world
            .announces
            .announced()
            .pop()
            .expect("le rafraîchissement différé annonce le dernier état");
        assert_eq!(metadata.status.map(|status| status.tree.added), Some(2));
    }

    #[test]
    fn given_five_repositories_being_followed_when_nothing_writes_to_disk_then_nothing_is_read_at_all(
    ) {
        // Given — le critère « avec 5 dépôts ouverts, la consommation CPU au repos reste
        // négligeable ». Au repos, une surveillance de fichiers ne coûte rien ; un
        // sondage, lui, coûterait cinq lectures **et cinq `git status`** par tour. C'est
        // ce que ce test empêche de réintroduire.
        let repos = ["/dev/a", "/dev/b", "/dev/c", "/dev/d", "/dev/e"];
        let world = WatchBuilder::with_repos(&repos);
        let watch = world.build();
        let roots: Vec<String> = repos.iter().map(|repo| (*repo).to_owned()).collect();
        watch.follow(&roots);
        let reads_after_attach = world.tree.reads();
        let invocations_after_attach = world.tree.invocations();

        // When — dix minutes passent, et la boucle de sonde continue de nommer les mêmes
        // racines trois fois par seconde
        world.time.advance(Duration::from_secs(600));
        world.time.fire_due();
        for _ in 0..1_800 {
            watch.follow(&roots);
        }

        // Then — cinq abonnements, un par worktree, aucune lecture de plus, et surtout
        // **aucun `git` lancé** : un par worktree au rattachement, plus rien ensuite
        assert_eq!(world.tree.subscriptions(), 5);
        assert_eq!(world.tree.reads(), reads_after_attach);
        assert_eq!(invocations_after_attach, 5);
        assert_eq!(world.tree.invocations(), invocations_after_attach);
        assert_eq!(world.announces.branches().len(), 5);
        // Et pas un signal de forme non plus : ce qui invalide les localisations retenues
        // par le registre d'onglets vient d'une écriture observée, jamais d'une passe.
        assert_eq!(world.relocations.count(), 0);
    }

    #[test]
    fn given_a_followed_repository_when_a_linked_worktree_appears_in_it_then_the_resolved_locations_are_declared_stale(
    ) {
        // Given — un dépôt sans worktree lié, avec un onglet dedans : il s'affiche à plat
        // (ADR-0012). Un `git worktree add` lancé depuis un autre terminal le fait passer à
        // la forme groupée sans que le `cwd` de l'onglet ne bouge — donc sans que rien, du
        // côté de la sonde, n'ait de raison de redemander où il se range.
        let world = WatchBuilder::new();
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned()]);
        world.announces.forget();
        let reads_after_attach = world.tree.reads();
        let invocations_after_attach = world.tree.invocations();

        // When — git écrit l'entrée du nouveau worktree
        world.tree.touch("/dev/ash/.git/worktrees/toc");

        // Then — le signal part, et il ne traîne ni relecture ni `git status` derrière lui :
        // la forme d'un dépôt n'est pas une de ses métadonnées
        assert_eq!(world.relocations.count(), 1);
        assert_eq!(world.tree.reads(), reads_after_attach);
        assert_eq!(world.tree.invocations(), invocations_after_attach);
        assert!(world.announces.branches().is_empty());
    }

    #[test]
    fn given_a_followed_repository_when_a_sibling_worktree_writes_in_its_own_git_dir_then_nothing_is_declared_stale(
    ) {
        // Given — un agent qui travaille dans un worktree frère écrit son index en rafale.
        // Chacune de ces écritures rendrait la résolution à la boucle de sonde si le filtre
        // regardait le contenu de `worktrees/` au lieu de sa liste.
        let world = WatchBuilder::new();
        let watch = world.build();
        watch.follow(&["/dev/ash".to_owned()]);

        // When
        for _ in 0..10 {
            world.tree.touch("/dev/ash/.git/worktrees/toc/index");
            world.tree.touch("/dev/ash/.git/worktrees/toc/logs/HEAD");
        }

        // Then
        assert_eq!(world.relocations.count(), 0);
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
    fn given_a_tab_outside_any_repository_when_the_probe_loop_keeps_naming_it_then_it_is_not_resolved_again_at_every_pass(
    ) {
        // Given — `follow` est appelée trois fois par seconde par la boucle de sonde
        // d'ADR-0005. Une racine qu'on ne sait pas surveiller ne doit pas y être réexaminée
        // à chaque passe : ce serait le sondage que la spec §5.3 écarte, et le critère
        // « au repos, la consommation CPU reste négligeable » avec lui.
        let world = WatchBuilder::new();
        let watch = world.build();
        let roots = vec!["/dev/notes".to_owned()];
        watch.follow(&roots);
        let lookups_after_the_first_pass = world.tree.lookups();

        // When — cent passes de la boucle
        for _ in 0..100 {
            watch.follow(&roots);
        }

        // Then — pas une question de plus au disque
        assert_eq!(world.tree.lookups(), lookups_after_the_first_pass);
    }

    #[test]
    fn given_a_directory_that_became_a_repository_when_the_window_regains_focus_then_it_starts_being_watched(
    ) {
        // Given — un `git init` dans le dossier d'un onglet déjà ouvert. Écarter une racine
        // pour toujours transformerait l'économie du test précédent en angle mort.
        let world = WatchBuilder::new();
        let watch = world.build();
        let roots = vec!["/dev/notes".to_owned()];
        watch.follow(&roots);
        world
            .tree
            .write("/dev/notes/.git/HEAD", "ref: refs/heads/main\n");

        // When
        watch.on_focus();
        watch.follow(&roots);

        // Then
        assert_eq!(world.tree.subscriptions(), 1);
        assert_eq!(
            world.announces.branches(),
            vec![("/dev/notes".to_owned(), "main".to_owned())]
        );
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
        assert_eq!(
            metadata.map(|metadata| metadata.head),
            Some(Head::Branch {
                name: "main".to_owned()
            })
        );
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
