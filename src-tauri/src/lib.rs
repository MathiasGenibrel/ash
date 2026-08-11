//! Ash — bibliothèque.
//!
//! Tout le code vit ici plutôt que dans `main.rs` : c'est ce qui permet à
//! `cargo test` de le compiler sans lier l'exécutable, et ce qui laisse la porte
//! ouverte au démon `ashd` d'ADR-0009, qui réutiliserait la même bibliothèque sous un
//! autre binaire.

pub mod features;

/// Le menu applicatif : les raccourcis de la spec §4.4, et leur chemin souris.
mod menu;

/// Banc de mesure du spike xterm.js — jetable, retiré avec le spike.
pub mod spike;

use std::path::Path;
use std::sync::Arc;

use features::git::{resolve_worktree, MetadataWatch, SystemFileSystem};
use features::probe::SystemProbe;
use features::pty::{PtyRegistry, RepoRef, SystemPtySpawner, TabLocation, WorktreeLocator};

/// Relie le port de `pty` à la résolution de `features::git`.
///
/// C'est ici, et seulement ici, que les deux features se rencontrent : `pty` ne connaît
/// que son trait, `git` ne sait rien des onglets. L'adaptateur ne fait que traduire — la
/// règle « un dépôt sans worktree lié s'affiche à plat »
/// ([ADR-0012](../../docs/adr/0012-worktree-unite-de-travail.md)) est déjà tranchée par
/// `resolve_worktree`, qui rend alors un worktree sans dépôt.
struct GitWorktrees;

impl WorktreeLocator for GitWorktrees {
    fn locate(&self, cwd: &Path) -> Option<TabLocation> {
        // Un `cwd` qu'on ne sait pas situer — chemin illisible, `.git` cassé, dépôt
        // disparu — n'est pas une erreur à remonter à l'utilisateur au milieu d'une passe
        // de sonde : l'onglet reste affiché, sans localisation.
        let located = resolve_worktree(&SystemFileSystem, cwd).ok()?;

        Some(TabLocation {
            worktree_root: located.worktree.root.display().to_string(),
            worktree_name: located.worktree.name,
            repo: located.repo.map(|repo| RepoRef {
                id: repo.git_dir.display().to_string(),
                name: repo.name,
            }),
        })
    }
}

/// Assemble et démarre l'application.
///
/// Composition root : c'est le seul endroit du crate où les implémentations concrètes
/// des effets système sont choisies et injectées. `SystemPtySpawner` et `SystemProbe`
/// n'apparaissent qu'ici ; partout ailleurs les features ne connaissent que leurs traits.
pub fn run() -> tauri::Result<()> {
    let ptys = Arc::new(PtyRegistry::new(
        Box::new(SystemPtySpawner),
        Arc::new(SystemProbe),
        Arc::new(GitWorktrees),
    ));

    let app = tauri::Builder::default()
        .manage(Arc::clone(&ptys))
        .manage(spike::Flow::default())
        // La surveillance git a besoin du handle de l'application pour émettre, et
        // l'application a besoin d'elle pour répondre à `git_metadata` : `setup` est le
        // seul point où les deux existent.
        .setup(|app| {
            use tauri::Manager;
            let watch = features::git::commands::watch_metadata(app.handle().clone());
            app.manage(watch);
            Ok(())
        })
        .menu(menu::build)
        .on_menu_event(|app, event| menu::dispatch(app, event.id().as_ref()))
        .invoke_handler(tauri::generate_handler![
            features::pty::commands::pty_open,
            features::pty::commands::pty_write,
            features::pty::commands::pty_resize,
            features::pty::commands::pty_ack,
            features::pty::commands::pty_close,
            features::pty::commands::pty_tabs,
            features::pty::commands::pty_has_foreground_process,
            features::git::commands::git_metadata,
            spike::spike_stream,
            spike::spike_ack,
            spike::spike_report
        ])
        .build(tauri::generate_context!())?;

    // La surveillance git, posée par `setup`. Elle est reprise ici pour être reliée aux
    // deux autres moments de la spec §5.3 : le rattachement d'un onglet, et le focus de
    // la fenêtre. Le troisième — la modification d'un fichier de contrôle — n'a besoin de
    // personne, c'est elle qui l'observe.
    let git_watch: Arc<MetadataWatch> = {
        use tauri::Manager;
        Arc::clone(app.state::<Arc<MetadataWatch>>().inner())
    };

    // La boucle de sonde d'ADR-0005 démarre ici, et pas dans une commande : elle observe
    // les onglets pour toute la durée de l'application, pas pour la durée d'un appel du
    // frontend. C'est aussi ici qu'on lui donne son ordre d'arrêt — quitter l'application
    // doit éteindre les sondes, pas laisser le système le faire à notre place.
    let follow = Arc::clone(&git_watch);
    let stop = features::pty::commands::watch_tabs(app.handle().clone(), &ptys, move |roots| {
        follow.follow(&roots);
    });

    app.run(move |_app, event| match event {
        // Un dépôt peut avoir bougé pendant qu'Ash était derrière une autre fenêtre.
        tauri::RunEvent::WindowEvent {
            event: tauri::WindowEvent::Focused(true),
            ..
        } => git_watch.on_focus(),
        tauri::RunEvent::Exit => {
            stop.ask();
            git_watch.stop();
        }
        _ => {}
    });

    Ok(())
}
