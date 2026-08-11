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

use features::git::{resolve_worktree, SystemFileSystem};
use features::probe::SystemProbe;
use features::pty::{PtyRegistry, RepoRef, SystemPtySpawner, TabLocation, WorktreeLocator};
use features::theme::{FileThemeStore, ThemeState, ThemeStore};

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

    // Le thème est relu **avant** la construction du menu : ses trois coches disent le
    // mode en cours, et le menu est bâti une seule fois, avant que la webview n'existe.
    let theme = Arc::new(ThemeState::restore(
        Arc::new(FileThemeStore::in_home()) as Arc<dyn ThemeStore>
    ));
    let theme_mode = theme.mode();

    let app = tauri::Builder::default()
        .manage(Arc::clone(&ptys))
        .manage(Arc::clone(&theme))
        .manage(spike::Flow::default())
        .menu(move |app| menu::build(app, theme_mode))
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
            features::theme::commands::theme_mode,
            spike::spike_stream,
            spike::spike_ack,
            spike::spike_report
        ])
        .build(tauri::generate_context!())?;

    // La surveillance git naît **après** `build` et **avant** `run` : elle a besoin du
    // handle de l'application pour émettre, et l'application a besoin d'elle pour répondre
    // à `git_metadata`. Ce créneau est le seul où les deux existent.
    //
    // Elle ne peut pas être posée depuis `setup` : dans Tauri 2, ce hook ne tourne pas
    // pendant `build()` mais au démarrage de `run()`. Un `state()` juste après `build()`
    // paniquait donc — « state() called before manage() » — et l'application ne s'ouvrait
    // pas du tout. Rien ne le voyait : le composition root n'a pas de test, et le seul
    // moment où ça se manifeste est le lancement réel.
    //
    // La surveillance est ensuite reliée aux deux autres moments de la spec §5.3 : le
    // rattachement d'un onglet, et le focus de la fenêtre. Le troisième — la modification
    // d'un fichier de contrôle — n'a besoin de personne, c'est elle qui l'observe.
    // La surveillance de `.git` est aussi ce qui apprend qu'un dépôt a gagné ou perdu un
    // worktree lié. La forme d'affichage d'ADR-0012 en dépend, et avec elle la localisation
    // que le registre retient pour chaque onglet : un `git worktree add` change la bonne
    // réponse sans qu'aucun `cwd` ne bouge. C'est ici — et nulle part ailleurs — que le
    // signal de `git` rejoint le registre de `pty` ; les deux features continuent de
    // s'ignorer, comme pour la résolution elle-même.
    let relocating = Arc::clone(&ptys);
    let git_watch = features::git::commands::watch_metadata(app.handle().clone(), move || {
        relocating.invalidate_locations();
    });
    {
        use tauri::Manager;
        app.manage(Arc::clone(&git_watch));
    }

    // La boucle de sonde d'ADR-0005 démarre ici, et pas dans une commande : elle observe
    // les onglets pour toute la durée de l'application, pas pour la durée d'un appel du
    // frontend. C'est aussi ici qu'on lui donne son ordre d'arrêt — quitter l'application
    // doit éteindre les sondes, pas laisser le système le faire à notre place.
    let follow = features::git::commands::follow_worktrees(&git_watch);
    let stop = features::pty::commands::watch_tabs(app.handle().clone(), &ptys, follow);

    app.run(move |_app, event| match event {
        // Un dépôt peut avoir bougé pendant qu'Ash était derrière une autre fenêtre.
        //
        // **Sur un fil à part, et c'est indispensable** : ce rappel-ci arrive sur le fil de
        // l'interface, et relire un worktree lance un `git status` qui peut prendre des
        // secondes sur un dépôt de plusieurs gigaoctets. Le faire ici gèlerait la fenêtre
        // au moment précis où l'utilisateur y revient. La surveillance, elle, ne suppose
        // aucun fil : c'est au composition root de savoir d'où il l'appelle.
        tauri::RunEvent::WindowEvent {
            event: tauri::WindowEvent::Focused(true),
            ..
        } => {
            let refreshing = Arc::clone(&git_watch);
            std::thread::spawn(move || refreshing.on_focus());
        }
        tauri::RunEvent::Exit => {
            stop.ask();
            git_watch.stop();
        }
        _ => {}
    });

    Ok(())
}
