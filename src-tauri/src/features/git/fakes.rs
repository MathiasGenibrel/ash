//! Les doubles des effets système de la feature : le disque qui **change**, le temps
//! qu'on avance à la main, et ce qui traverse la frontière.
//!
//! Ils vivent ici plutôt que dans le module de tests de la surveillance parce qu'ils
//! doublent des **ports** — comme [`super::fake_fs::FakeFs`], qui double le système de
//! fichiers immobile.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::error::GitError;
use super::fake_fs::FakeFs;
use super::git_cli::StatusReader;
use super::metadata::{Head, WorktreeMetadata};
use super::metadata_watch::{Announce, Committed, Relocate};
use super::ports::{Entry, FileSystem};
use super::targets::WatchRoot;
use super::watcher::{FileWatcher, OnChange, WatchHandle};
use crate::shared::time::{Clock, Scheduler, UnixMillis};

/// Un arbre de fichiers **surveillé** : le disque et l'observateur, ensemble.
///
/// Les deux ne se séparent pas dans un test : « git écrit, l'observateur le dit » est un
/// seul geste, et le fabriquer en deux objets obligerait chaque scénario à les tenir
/// synchronisés. Il compte aussi ses lectures et ses abonnements — c'est par là qu'on
/// vérifie qu'au repos, rien ne tourne.
pub struct WatchedTree {
    tree: Mutex<FakeFs>,
    reads: AtomicUsize,
    lookups: AtomicUsize,
    subscriptions: Arc<Mutex<Vec<Subscription>>>,
    next_id: AtomicUsize,
    /// Ce que `git status --porcelain=v2 --branch` répondrait, par racine de worktree.
    ///
    /// Absent veut dire « `git` n'a pas répondu » — introuvable, trop lent, en erreur.
    /// C'est le défaut : la plupart des scénarios de la surveillance ne parlent pas de
    /// l'arbre de travail, et aucun d'eux ne doit lancer de processus.
    porcelain: Mutex<BTreeMap<PathBuf, String>>,
    invocations: AtomicUsize,
}

struct Subscription {
    id: usize,
    roots: Vec<WatchRoot>,
    on_change: OnChange,
}

/// L'abonnement rendu par le double : son `Drop` le retire, comme le vrai.
struct FakeHandle {
    id: usize,
    subscriptions: Arc<Mutex<Vec<Subscription>>>,
}

impl WatchHandle for FakeHandle {}

impl Drop for FakeHandle {
    fn drop(&mut self) {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            subscriptions.retain(|subscription| subscription.id != self.id);
        }
    }
}

impl WatchedTree {
    /// Des dépôts classiques, chacun sur `main`, plus un dossier hors de tout dépôt.
    pub fn with_repos(roots: &[&str]) -> Arc<Self> {
        let tree = roots
            .iter()
            .fold(FakeFs::new().dir("/dev/notes"), |tree, root| {
                tree.plain_repo(root)
                    .file(&format!("{root}/.git/HEAD"), "ref: refs/heads/main\n")
            });
        Arc::new(Self {
            tree: Mutex::new(tree),
            reads: AtomicUsize::new(0),
            lookups: AtomicUsize::new(0),
            subscriptions: Arc::new(Mutex::new(Vec::new())),
            next_id: AtomicUsize::new(0),
            porcelain: Mutex::new(BTreeMap::new()),
            invocations: AtomicUsize::new(0),
        })
    }

    /// Git écrit le `HEAD` d'un dépôt. Rien n'est notifié : c'est [`Self::touch`] qui joue
    /// l'observateur, et les deux sont séparés pour pouvoir écrire une rafale.
    pub fn set_head(&self, root: &str, content: &str) {
        self.write(&format!("{root}/.git/HEAD"), &format!("{content}\n"));
    }

    pub fn write(&self, path: &str, content: &str) {
        if let Ok(mut tree) = self.tree.lock() {
            tree.write(path, content);
        }
    }

    /// L'observateur signale qu'un chemin a bougé, comme FSEvents le ferait.
    pub fn touch(&self, path: &str) {
        let path = PathBuf::from(path);
        let listeners: Vec<OnChange> = match self.subscriptions.lock() {
            Ok(subscriptions) => subscriptions
                .iter()
                .filter(|subscription| covers(&subscription.roots, &path))
                .map(|subscription| Arc::clone(&subscription.on_change))
                .collect(),
            Err(_) => Vec::new(),
        };
        // Hors du verrou : le rappel rentre dans la surveillance, qui peut s'abonner ou se
        // désabonner en réponse.
        for listener in listeners {
            listener(&path);
        }
    }

    /// Ce que `git status` répondra pour ce worktree, la prochaine fois qu'on l'appelle.
    pub fn set_porcelain(&self, root: &str, output: &str) {
        if let Ok(mut porcelain) = self.porcelain.lock() {
            porcelain.insert(PathBuf::from(root), output.to_owned());
        }
    }

    /// Combien de fois `git` a été lancé depuis le début du scénario.
    ///
    /// Le compteur qui garantit le critère « aucun `git status` dans la boucle de sonde » :
    /// au repos, il ne bouge pas.
    pub fn invocations(&self) -> usize {
        self.invocations.load(Ordering::Relaxed)
    }

    /// Combien de lectures de fichier depuis le début du scénario.
    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }

    /// Combien de questions posées au disque, lectures comprises.
    ///
    /// Une résolution de worktree ne lit pas forcément un fichier — elle interroge des
    /// chemins. C'est par ce compteur-là qu'on voit une racine réexaminée à chaque passe
    /// de la boucle de sonde, ce que [`Self::reads`] laisserait passer.
    pub fn lookups(&self) -> usize {
        self.lookups.load(Ordering::Relaxed)
    }

    /// Combien d'abonnements vivants — un par worktree observé.
    pub fn subscriptions(&self) -> usize {
        self.subscriptions
            .lock()
            .map(|subscriptions| subscriptions.len())
            .unwrap_or_default()
    }

    fn with_tree<T>(&self, read: impl FnOnce(&FakeFs) -> T, absent: T) -> T {
        match self.tree.lock() {
            Ok(tree) => read(&tree),
            Err(_) => absent,
        }
    }
}

/// Un chemin est-il couvert par l'une de ces racines ?
fn covers(roots: &[WatchRoot], path: &Path) -> bool {
    roots.iter().any(|root| {
        if root.recursive {
            path.starts_with(&root.path)
        } else {
            path == root.path || path.parent() == Some(root.path.as_path())
        }
    })
}

impl FileSystem for WatchedTree {
    fn entry(&self, path: &Path) -> Option<Entry> {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        self.with_tree(|tree| tree.entry(path), None)
    }

    fn read_to_string(&self, path: &Path) -> Result<String, String> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.lookups.fetch_add(1, Ordering::Relaxed);
        self.with_tree(|tree| tree.read_to_string(path), Err("verrou".to_owned()))
    }

    fn has_entries(&self, path: &Path) -> bool {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        self.with_tree(|tree| tree.has_entries(path), false)
    }

    fn list_dir(&self, path: &Path) -> Vec<PathBuf> {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        self.with_tree(|tree| tree.list_dir(path), Vec::new())
    }

    fn canonicalize(&self, path: &Path) -> Option<PathBuf> {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        self.with_tree(|tree| tree.canonicalize(path), None)
    }
}

impl StatusReader for WatchedTree {
    fn read(&self, worktree_root: &Path) -> Option<String> {
        self.invocations.fetch_add(1, Ordering::Relaxed);
        self.porcelain.lock().ok()?.get(worktree_root).cloned()
    }
}

impl FileWatcher for WatchedTree {
    fn watch(
        &self,
        roots: &[WatchRoot],
        on_change: OnChange,
    ) -> Result<Box<dyn WatchHandle>, GitError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut subscriptions = self
            .subscriptions
            .lock()
            .map_err(|_| GitError::UnreadablePath(PathBuf::from("<verrou>")))?;
        subscriptions.push(Subscription {
            id,
            roots: roots.to_vec(),
            on_change,
        });
        Ok(Box::new(FakeHandle {
            id,
            subscriptions: Arc::clone(&self.subscriptions),
        }))
    }
}

/// Le temps, sous contrôle du test : une horloge qu'on avance, des reports qu'on déclenche.
///
/// Aucun test de la surveillance ne dort. C'est ce qui permet d'écrire « une rafale
/// pendant la fenêtre de 5 s » en trois lignes, et de le relire en une seconde.
pub struct ControlledTime {
    origin: Instant,
    elapsed: Mutex<Duration>,
    deferred: Mutex<Vec<Deferred>>,
}

struct Deferred {
    due_at: Duration,
    action: Box<dyn FnOnce() + Send>,
}

impl ControlledTime {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            origin: Instant::now(),
            elapsed: Mutex::new(Duration::ZERO),
            deferred: Mutex::new(Vec::new()),
        })
    }

    /// Le temps passe, sans que rien ne se déclenche : c'est [`Self::fire_due`] qui joue
    /// le réveil, pour que le test dise **quand** il a lieu.
    pub fn advance(&self, delay: Duration) {
        if let Ok(mut elapsed) = self.elapsed.lock() {
            *elapsed += delay;
        }
    }

    /// Déclenche les reports arrivés à échéance.
    pub fn fire_due(&self) {
        let now = self.elapsed();
        let due: Vec<Deferred> = match self.deferred.lock() {
            Ok(mut deferred) => {
                let (due, waiting) = std::mem::take(&mut *deferred)
                    .into_iter()
                    .partition(|entry| entry.due_at <= now);
                *deferred = waiting;
                due
            }
            Err(_) => Vec::new(),
        };
        for entry in due {
            (entry.action)();
        }
    }

    /// Combien de reports attendent leur échéance.
    pub fn pending(&self) -> usize {
        self.deferred
            .lock()
            .map(|deferred| deferred.len())
            .unwrap_or_default()
    }

    fn elapsed(&self) -> Duration {
        self.elapsed
            .lock()
            .map(|elapsed| *elapsed)
            .unwrap_or_default()
    }
}

impl Clock for ControlledTime {
    fn now(&self) -> Instant {
        self.origin + self.elapsed()
    }

    /// L'heure murale du test : une date fixe, avancée du même délai que l'horloge
    /// monotone. Aucune règle de cette feature n'en dépend — la surveillance date des
    /// rafales, pas des événements — mais elle reste **déterministe**, sans quoi un test
    /// qui la lirait un jour dépendrait de l'heure de la machine.
    fn wall(&self) -> UnixMillis {
        let elapsed = UnixMillis::try_from(self.elapsed().as_millis()).unwrap_or_default();
        FAKE_EPOCH + elapsed
    }
}

/// L'origine murale des tests — le 1ᵉʳ janvier 2026 à minuit UTC, en millisecondes.
const FAKE_EPOCH: UnixMillis = 1_767_225_600_000;

impl Scheduler for ControlledTime {
    fn after(&self, delay: Duration, action: Box<dyn FnOnce() + Send + 'static>) {
        let due_at = self.elapsed() + delay;
        if let Ok(mut deferred) = self.deferred.lock() {
            deferred.push(Deferred { due_at, action });
        }
    }
}

/// Ce que la surveillance a poussé vers le frontend.
#[derive(Default)]
pub struct RecordedAnnounces(Mutex<Vec<(String, WorktreeMetadata)>>);

impl RecordedAnnounces {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Le rappel à injecter dans la surveillance.
    pub fn announce(self: &Arc<Self>) -> Announce {
        let recorded = Arc::clone(self);
        Arc::new(move |root: &Path, metadata: &WorktreeMetadata| {
            if let Ok(mut announces) = recorded.0.lock() {
                announces.push((root.display().to_string(), metadata.clone()));
            }
        })
    }

    /// Les worktrees annoncés, avec la branche annoncée pour chacun, dans l'ordre.
    pub fn branches(&self) -> Vec<(String, String)> {
        self.announced()
            .into_iter()
            .map(|(root, metadata)| {
                let branch = match metadata.head {
                    Head::Branch { name } => name,
                    Head::Detached { commit } => commit,
                };
                (root, branch)
            })
            .collect()
    }

    /// Les métadonnées annoncées, entières — pour ce que la branche seule ne dit pas.
    pub fn announced(&self) -> Vec<(String, WorktreeMetadata)> {
        self.0
            .lock()
            .map(|announces| announces.clone())
            .unwrap_or_default()
    }

    /// Oublie ce qui précède — le `Given` d'un scénario en produit toujours un peu.
    pub fn forget(&self) {
        if let Ok(mut announces) = self.0.lock() {
            announces.clear();
        }
    }
}

/// Combien de fois la surveillance a dit qu'un dépôt avait changé de forme.
///
/// Le compteur est ce qui rend le signal observable des deux côtés : qu'il parte quand un
/// worktree lié apparaît, et **qu'il ne parte pas** au repos.
#[derive(Default)]
pub struct RecordedRelocations(AtomicUsize);

impl RecordedRelocations {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Le rappel à injecter dans la surveillance.
    pub fn relocate(self: &Arc<Self>) -> Relocate {
        let recorded = Arc::clone(self);
        Arc::new(move || {
            recorded.0.fetch_add(1, Ordering::Relaxed);
        })
    }

    pub fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

/// Les mouvements de `HEAD` remontés, worktree par dépôt.
///
/// Le pendant de [`RecordedRelocations`] pour la troisième sortie de la surveillance : elle
/// ne dit pas *quel* commit est né — la surveillance ne le sait pas — mais **où** aller le
/// lire, et sous quelle identité de dépôt le ranger.
#[derive(Default)]
pub struct RecordedCommits(Mutex<Vec<(String, String)>>);

impl RecordedCommits {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Le rappel à injecter dans la surveillance.
    pub fn committed(self: &Arc<Self>) -> Committed {
        let recorded = Arc::clone(self);
        Arc::new(move |root: &Path, common_dir: &Path| {
            if let Ok(mut moves) = recorded.0.lock() {
                moves.push((root.display().to_string(), common_dir.display().to_string()));
            }
        })
    }

    pub fn moves(&self) -> Vec<(String, String)> {
        self.0.lock().map(|moves| moves.clone()).unwrap_or_default()
    }
}
