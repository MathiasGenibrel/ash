//! La surveillance de fichiers, derrière un trait que la feature possède.
//!
//! C'est l'effet système central de ce module : sans lui, connaître l'état git de `n`
//! dépôts demanderait de le redemander en boucle — ce que la spec §5.3 exclut
//! explicitement. Derrière le trait, macOS répond par FSEvents ; dans les tests, un double
//! qui rejoue des chemins sans jamais toucher au disque.

use std::path::Path;
use std::sync::Arc;

use notify::{RecursiveMode, Watcher};

use super::error::GitError;
use super::targets::WatchRoot;

/// Ce qu'on appelle quand un chemin surveillé a bougé.
///
/// Appelée depuis le fil de l'observateur, plusieurs fois pour une même écriture : c'est
/// à l'appelant de filtrer et de limiter le débit, pas à l'adaptateur.
pub type OnChange = Arc<dyn Fn(&Path) + Send + Sync + 'static>;

/// Un abonnement vivant. **L'arrêt est le `Drop`** : un observateur qu'on oublie de
/// relâcher est un observateur qui survit à son worktree.
pub trait WatchHandle: Send + Sync {}

/// L'observateur de fichiers.
pub trait FileWatcher: Send + Sync {
    /// Surveille ces dossiers, et rend l'abonnement.
    ///
    /// Un dossier illisible est ignoré ; l'appel n'échoue que si **aucun** n'a pu être
    /// surveillé — un dépôt qu'on ne peut pas observer du tout est une information, une
    /// racine manquante sur trois n'en est pas une.
    fn watch(
        &self,
        roots: &[WatchRoot],
        on_change: OnChange,
    ) -> Result<Box<dyn WatchHandle>, GitError>;
}

/// L'observateur du système — FSEvents sur macOS, via `notify`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemWatcher;

/// L'abonnement de `notify` : le relâcher désabonne le flux FSEvents.
///
/// Rien ne le lit — c'est son `Drop` qui rend le service, et c'est pour ça qu'il faut
/// le garder.
struct NotifyHandle {
    _watcher: notify::RecommendedWatcher,
}

impl WatchHandle for NotifyHandle {}

impl FileWatcher for SystemWatcher {
    fn watch(
        &self,
        roots: &[WatchRoot],
        on_change: OnChange,
    ) -> Result<Box<dyn WatchHandle>, GitError> {
        let first = roots
            .first()
            .map(|root| root.path.clone())
            .unwrap_or_default();

        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                // Une erreur de flux n'a personne à qui être remontée depuis ce fil, et elle
                // ne dit rien que la prochaine lecture ne dirait mieux.
                if let Ok(event) = event {
                    for path in &event.paths {
                        on_change(path);
                    }
                }
            })
            .map_err(|why| GitError::Io {
                path: first.clone(),
                why: why.to_string(),
            })?;

        let watched = roots
            .iter()
            .filter(|root| {
                let mode = if root.recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                watcher.watch(&root.path, mode).is_ok()
            })
            .count();

        if watched == 0 {
            return Err(GitError::UnreadablePath(first));
        }

        Ok(Box::new(NotifyHandle { _watcher: watcher }))
    }
}

/// Le seul endroit où FSEvents est branché sur un vrai dépôt.
///
/// Tout le reste de la feature se teste contre le double de [`FileWatcher`], qui rejoue les
/// chemins qu'on **suppose** recevoir. Cette supposition n'est vérifiée nulle part ailleurs,
/// et elle est porteuse : si FSEvents ne remontait que les écritures **dans** le dossier
/// d'un worktree lié — jamais son entrée — le filtre de forme de
/// [`super::targets::WatchTargets::concerns_layout`] ne dirait jamais oui, et un dépôt qui
/// gagne ou perd un worktree resterait affiché comme avant, avec toute la suite verte.
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::features::git::targets::WatchTargets;

    /// Ce que le vrai `git` écrit, dans un vrai dossier — le reste de la feature s'en passe.
    fn git(at: &Path, args: &[&str]) {
        let done = std::process::Command::new("git")
            .args(args)
            .current_dir(at)
            .output()
            .expect("git est installé : les autres tests d'intégration en dépendent déjà");
        assert!(done.status.success(), "git {args:?} a échoué : {done:?}");
    }

    /// Un dépôt d'un commit, sous un chemin **canonique**.
    ///
    /// La canonisation n'est pas un détail de confort : FSEvents remonte des chemins réels,
    /// et sur macOS `/var` est un lien vers `/private/var`. Comparer un chemin remonté à un
    /// chemin non canonique ne rend jamais vrai — c'est un faux négatif silencieux, et le
    /// test le rencontre avant l'utilisateur.
    fn repository(name: &str) -> PathBuf {
        let repo = std::env::temp_dir()
            .join(format!("ash-watch-{}-{name}", std::process::id()))
            .join("omelette");
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("le dossier temporaire est accessible en écriture");
        let repo = repo.canonicalize().expect("le dossier vient d'être créé");
        git(&repo, &["init", "--quiet"]);
        git(&repo, &["config", "user.email", "ash@example.test"]);
        git(&repo, &["config", "user.name", "Ash"]);
        std::fs::write(repo.join("omelette.md"), "oeufs").expect("le dépôt est accessible");
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "--quiet", "-m", "one"]);
        repo
    }

    /// Les chemins remontés par l'observateur, tels quels.
    #[derive(Default)]
    struct Delivered(Mutex<Vec<PathBuf>>);

    impl Delivered {
        fn on_change(self: &Arc<Self>) -> OnChange {
            let delivered = Arc::clone(self);
            Arc::new(move |path: &Path| {
                if let Ok(mut paths) = delivered.0.lock() {
                    paths.push(path.to_owned());
                }
            })
        }

        /// Vrai dès qu'un chemin remonté déclare un changement de forme.
        ///
        /// L'attente est généreuse — FSEvents groupe et diffère — et se termine dès le
        /// premier chemin qui compte : en pratique le test dure moins d'une seconde.
        fn announces_layout(&self, targets: &WatchTargets) -> bool {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if let Ok(paths) = self.0.lock() {
                    if paths.iter().any(|path| targets.concerns_layout(path)) {
                        return true;
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            false
        }

        fn forget(&self) {
            if let Ok(mut paths) = self.0.lock() {
                paths.clear();
            }
        }
    }

    #[test]
    fn given_a_real_repository_under_watch_when_it_gains_then_loses_a_linked_worktree_then_each_change_of_shape_is_delivered(
    ) {
        // Given — un dépôt à plat, surveillé exactement comme Ash le surveille : les racines
        // que `WatchTargets` demande, et rien de plus.
        let repo = repository("shape");
        let git_dir = repo.join(".git");
        let targets = WatchTargets::for_worktree(&git_dir, &git_dir);
        let delivered = Arc::new(Delivered::default());
        let _handle = SystemWatcher
            .watch(targets.roots(), delivered.on_change())
            .expect("un dépôt qui vient d'être créé est observable");

        // When — un `git worktree add` lancé depuis un autre terminal, puis son retrait
        git(
            &repo,
            &["worktree", "add", "--quiet", "../toc", "-b", "toc"],
        );
        let announced_the_arrival = delivered.announces_layout(&targets);
        delivered.forget();
        git(&repo, &["worktree", "remove", "../toc"]);
        let announced_the_departure = delivered.announces_layout(&targets);

        // Then — dans les deux sens : sans ça, l'onglet resterait à plat après un `add`, ou
        // groupé tout seul après un `remove`, jusqu'au redémarrage (ADR-0012)
        let _ = std::fs::remove_dir_all(repo.parent().unwrap_or(&repo));
        assert!(announced_the_arrival);
        assert!(announced_the_departure);
    }
}
