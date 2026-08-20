//! La surface de la feature vers le frontend : une commande, un event.
//!
//! Le frontend ne connaît de `git` que ces deux noms et les types qui traversent. Il
//! **rend** l'état ; c'est ici qu'on le lui pousse
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::git_cli::SystemGit;
use super::metadata::WorktreeMetadata;
use super::metadata_watch::{Listeners, MetadataWatch};
use super::system_fs::SystemFileSystem;
use super::throttle::MIN_INTERVAL;
use super::watcher::SystemWatcher;
use crate::shared::time::{SystemClock, ThreadScheduler};

/// Nom de l'event qui porte l'état git d'un worktree.
///
/// Contrat avec `src/shared/ipc/` : une chaîne que rien ne vérifie à la compilation, comme
/// celle des onglets.
pub const METADATA_CHANGED_EVENT: &str = "ash://git-metadata";

/// L'état git d'un worktree, tel qu'il traverse la frontière.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MetadataChanged {
    /// La racine du worktree — la même clé que celle des onglets (`TabLocation`).
    pub worktree_root: String,
    pub metadata: WorktreeMetadata,
}

/// L'état git connu d'un worktree.
///
/// Ce que le frontend lit en s'affichant ; ensuite, c'est l'event qui le tient à jour.
/// `None` pour un répertoire hors de tout dépôt, ou dont les fichiers de contrôle ne se
/// lisent pas — les deux se rendent pareil : sans métadonnées git.
///
/// **`async` volontairement** : Tauri exécute une commande synchrone sur le fil de
/// l'interface. Pour un worktree déjà surveillé, la réponse est en mémoire ; pour un
/// autre, elle coûte une résolution et un `git status`, qui n'ont rien à faire sur le fil
/// qui dessine la fenêtre.
/// Le handle plutôt que `tauri::State` : une commande `async` qui **emprunte** l'état est
/// obligée de rendre un `Result`, et une erreur qui ne peut pas se produire n'a pas sa
/// place dans le contrat.
#[tauri::command]
pub async fn git_metadata<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
) -> Option<WorktreeMetadata> {
    let watch = app.state::<Arc<MetadataWatch>>();
    watch.metadata(Path::new(&worktree_root))
}

/// Construit la surveillance avec les adaptateurs du système, et la relie à la fenêtre.
///
/// Le pendant de `pty::commands::watch_tabs` : l'assemblage des effets réels d'une feature
/// se fait dans son `commands.rs`, et le composition root ne fait que déclencher et arrêter.
///
/// `on_relocation` est appelé quand un dépôt surveillé gagne ou perd un worktree lié. Il ne
/// traverse pas la frontière du frontend : ce n'est pas un état à rendre, c'est un signal
/// interne vers ce qui garde des résolutions. La feature ne sait pas qui l'écoute.
/// `on_head_moved` est appelé quand le `HEAD` d'un worktree surveillé bouge — un commit a pu y
/// naître ([ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md)). Il ne
/// traverse pas non plus la frontière du frontend, et pour la même raison que
/// `on_relocation` : la feature dit ce qu'elle a observé, elle ne sait pas qui l'écoute.
pub fn watch_metadata<R: Runtime>(
    app: AppHandle<R>,
    on_relocation: impl Fn() + Send + Sync + 'static,
    on_head_moved: impl Fn(&Path, &Path) + Send + Sync + 'static,
) -> Arc<MetadataWatch> {
    MetadataWatch::new(
        Arc::new(SystemFileSystem),
        Arc::new(SystemGit::default()),
        Arc::new(SystemWatcher),
        Arc::new(SystemClock),
        Arc::new(ThreadScheduler),
        MIN_INTERVAL,
        Listeners {
            announce: Arc::new(move |root: &Path, metadata: &WorktreeMetadata| {
                // Échouer à émettre signifie qu'il n'y a plus de webview à prévenir : rien à
                // rattraper, et surtout pas de panique dans un fil de fond.
                let _ = app.emit(
                    METADATA_CHANGED_EVENT,
                    MetadataChanged {
                        worktree_root: root.display().to_string(),
                        metadata: metadata.clone(),
                    },
                );
            }),
            relocate: Arc::new(on_relocation),
            head_moved: Arc::new(on_head_moved),
        },
    )
}

/// De quoi nommer les worktrees habités **sans attendre** que la surveillance ait fini.
///
/// La boucle de sonde d'ADR-0005 appelle ce rappel trois fois par seconde, et son fil est
/// celui qui suit le `cwd` des onglets. Or le rattachement d'un worktree lance un
/// `git status`, qui peut prendre des secondes sur un gros dépôt : l'appeler directement
/// ferait figer les titres d'onglets le temps que git réponde. Le rappel ne fait donc
/// qu'envoyer la liste ; un fil dédié la consomme.
///
/// Les listes en attente sont **écrasées** plutôt que traitées l'une après l'autre :
/// pendant un `git status`, la boucle en a peut-être empilé dix, toutes périmées sauf la
/// dernière.
pub fn follow_worktrees(watch: &Arc<MetadataWatch>) -> impl Fn(Vec<String>) + Send + 'static {
    let (sender, receiver) = std::sync::mpsc::channel::<Vec<String>>();
    // Un `Weak` : ce fil observe la surveillance, il ne doit pas être ce qui la maintient
    // en vie après l'arrêt de l'application.
    let watch = Arc::downgrade(watch);

    std::thread::spawn(move || {
        while let Ok(mut roots) = receiver.recv() {
            while let Ok(fresher) = receiver.try_recv() {
                roots = fresher;
            }
            let Some(watch) = watch.upgrade() else {
                return;
            };
            watch.follow(&roots);
        }
    });

    move |roots| {
        // Échouer à envoyer signifie que le fil est parti : il n'y a plus rien à suivre.
        let _ = sender.send(roots);
    }
}
