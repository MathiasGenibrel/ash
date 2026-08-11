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
