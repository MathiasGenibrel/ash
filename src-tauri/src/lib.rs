//! Ash — bibliothèque.
//!
//! Tout le code vit ici plutôt que dans `main.rs` : c'est ce qui permet à
//! `cargo test` de le compiler sans lier l'exécutable, et ce qui laisse la porte
//! ouverte au démon `ashd` d'ADR-0009, qui réutiliserait la même bibliothèque sous un
//! autre binaire.

pub mod features;

/// Ce que plusieurs features partagent sans qu'aucune ne le possède.
pub mod shared;

/// Le menu applicatif : les raccourcis de la spec §4.4, et leur chemin souris.
mod menu;

/// Banc de mesure du spike xterm.js — jetable, retiré avec le spike.
pub mod spike;

use std::path::Path;
use std::sync::Arc;

use features::agents::commands::{AgentEvent, AGENT_EVENT};
use features::agents::{Adapter, ClaudeCodeAdapter, EventFrame, EventSink, GenericAdapter};
use features::git::{resolve_worktree, SystemFileSystem};
use features::probe::SystemProbe;
use features::pty::{PtyRegistry, RepoRef, SystemPtySpawner, TabLocation, WorktreeLocator};
use features::settings::{
    AdapterProfile, BlockAt, HookBlocks, SystemCommands, SystemConfigFiles, ToolRegistry, Verifier,
};
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

/// Relie le port du socket d'events au registre des onglets, et à la webview.
///
/// C'est ici, et seulement ici, que les deux features se rencontrent : `agents` ne connaît
/// que son trait, `pty` ne sait rien des hooks. La corrélation, elle, est déjà tranchée par
/// [ADR-0007](../../docs/adr/0007-etats-par-hooks.md) — `ASH_TAB_ID`, que le registre a posé
/// sur le shell de chaque onglet, et que la descendance de ce shell hérite jusqu'au hook.
struct HookEvents {
    app: tauri::AppHandle,
    ptys: Arc<PtyRegistry>,
}

impl EventSink for HookEvents {
    fn knows(&self, tab_id: &str) -> bool {
        self.ptys.knows(tab_id)
    }

    fn deliver(&self, event: &EventFrame) {
        use tauri::Emitter;
        // Échouer à émettre signifie qu'il n'y a plus de webview à prévenir : rien à
        // rattraper, et surtout pas de panique dans un fil de fond.
        let _ = self.app.emit(AGENT_EVENT, AgentEvent::from(event));
    }
}

/// Relie la fenêtre de réglages à l'écriture des hooks, en traduisant un **identifiant**
/// d'adaptateur en instrumentation.
///
/// C'est ici, et seulement ici, que les trois features se rencontrent : `settings` ne
/// connaît aucun adaptateur concret
/// ([ADR-0008](../../docs/adr/0008-abstraction-adapter.md)), `agents` décrit ce qu'il faut
/// écrire sans jamais l'écrire, et `hooks` écrit sans savoir de quel outil il s'agit. Le
/// seul endroit qui connaît les trois est celui qui les assemble.
///
/// Il n'y a aucune décision ici — un identifiant, un dossier, un appel — et c'est
/// délibéré : le composition root n'a pas de test unitaire, donc tout ce qui s'y glisse
/// n'en a pas non plus.
struct AdapterHooks {
    adapters: Vec<Arc<dyn Adapter>>,
    files: Arc<dyn features::hooks::ConfigFiles>,
}

impl AdapterHooks {
    fn describing(
        &self,
        adapter: &str,
        config_dir: &Path,
    ) -> Option<features::agents::Instrumentation> {
        self.adapters
            .iter()
            .find(|known| known.id() == adapter)?
            .instrumentation(config_dir)
    }
}

impl HookBlocks for AdapterHooks {
    fn inspect(&self, adapter: &str, config_dir: &Path) -> Option<BlockAt> {
        let instrumentation = self.describing(adapter, config_dir)?;
        Some(BlockAt {
            file: instrumentation.file.clone(),
            presence: features::hooks::inspect(&*self.files, &instrumentation),
        })
    }

    fn install(&self, adapter: &str, config_dir: &Path) -> Result<(), String> {
        let instrumentation = self
            .describing(adapter, config_dir)
            .ok_or_else(|| format!("the {adapter} adapter has no hooks to install"))?;
        features::hooks::install(&*self.files, &instrumentation)
            .map(|_| ())
            .map_err(|why| why.to_string())
    }

    fn remove(&self, adapter: &str, config_dir: &Path) -> Result<(), String> {
        let instrumentation = self
            .describing(adapter, config_dir)
            .ok_or_else(|| format!("the {adapter} adapter wrote nothing to remove"))?;
        features::hooks::uninstall(&*self.files, &instrumentation.file)
            .map(|_| ())
            .map_err(|why| why.to_string())
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

    // La fenêtre de réglages ne propose que les adaptateurs que **cette** application
    // embarque : c'est ici qu'on les connaît, et la feature `settings` n'a donc pas à
    // connaître leurs implémentations
    // ([ADR-0008](../../docs/adr/0008-abstraction-adapter.md)). Un adaptateur de plus est
    // une ligne de plus ici, et rien à changer dans les réglages.
    //
    // **Le profil est la traduction d'un adaptateur en ce que la vérification sait
    // regarder** : de la donnée, et non le trait lui-même. C'est ce qui laisse `settings`
    // ignorer `GenericAdapter` comme il ignorera les autres — et c'est ici, au seul endroit
    // qui connaît les deux, que la traduction se fait.
    //
    // `generic` ne signe rien et n'impose aucun dossier, et ce n'est pas un manque : il est
    // l'adaptateur de l'outil dont on ne sait rien. La séquence en tire une **réserve** —
    // le dossier est accepté, mais rien ne prouve que la commande le lit — au lieu de
    // lancer un programme pour une question à laquelle il ne saurait pas répondre.
    let mut adapters: Vec<Arc<dyn Adapter>> = vec![Arc::new(GenericAdapter)];
    let mut profiles = vec![AdapterProfile {
        id: GenericAdapter.id().to_owned(),
        default_config: None,
        signature: Vec::new(),
        config_env: None,
        probe_args: vec!["--version".to_owned()],
    }];

    // Claude Code, et le seul adaptateur qui pose vraiment des hooks aujourd'hui.
    //
    // Il n'est enregistré que si l'on sait où est `ash-event` : sans ce chemin absolu, le
    // bloc écrit chez l'utilisateur nommerait une commande que le shell du hook ne
    // trouverait pas, et l'outil paraîtrait instrumenté sans jamais rien émettre. Ne pas le
    // proposer du tout est plus honnête que de le proposer cassé.
    //
    // **Les quatre champs du profil viennent de ce que l'adaptateur sait déjà** :
    // `CLAUDE_CONFIG_DIR` est la variable par laquelle on lui impose un dossier — c'est
    // elle qui fait que `claude` et `claude-perso` sont deux configurations —, `~/.claude`
    // est le dossier qu'il lit quand personne ne lui en impose un, et `--version` est
    // l'invocation qui le fait répondre sans rien faire.
    //
    // **La signature est `projects` seul, et pas `settings.json`.** Le fichier de réglages
    // est précisément celui qu'Ash s'apprête à écrire : l'exiger rendrait invalide un
    // dossier de configuration tout neuf, donc interdirait d'y poser les hooks — et
    // l'entrée resterait bloquée sur la seule chose que l'installation aurait réparée.
    match ClaudeCodeAdapter::beside_the_app() {
        Some(claude) => {
            profiles.push(AdapterProfile {
                id: claude.id().to_owned(),
                default_config: Some("~/.claude".to_owned()),
                signature: vec!["projects".to_owned()],
                config_env: Some("CLAUDE_CONFIG_DIR".to_owned()),
                probe_args: vec!["--version".to_owned()],
            });
            adapters.push(Arc::new(claude));
        }
        None => eprintln!("ash: ash-event est introuvable ; claude-code n'est pas proposé"),
    }

    let tools = Arc::new(ToolRegistry::new(
        Arc::new(Verifier::new(
            Arc::new(SystemConfigFiles),
            Arc::new(SystemCommands),
            profiles,
        )),
        Arc::new(AdapterHooks {
            adapters,
            files: Arc::new(features::hooks::SystemConfigFiles),
        }),
    ));

    let app = tauri::Builder::default()
        .manage(Arc::clone(&ptys))
        .manage(Arc::clone(&theme))
        .manage(Arc::clone(&tools))
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
            features::settings::commands::settings_tools,
            features::settings::commands::settings_declare_tool,
            features::settings::commands::settings_forget_tool,
            features::settings::commands::settings_retarget_tool,
            features::settings::commands::settings_verify_tool,
            features::settings::commands::settings_verify_all,
            features::settings::commands::settings_verify_draft,
            features::settings::commands::settings_reset_tool,
            features::settings::commands::settings_undo_reset,
            features::settings::commands::settings_install_hooks,
            features::settings::commands::settings_remove_hooks,
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

    // Le socket d'events d'ADR-0007 s'ouvre dans le même créneau, et pour la même raison :
    // il lui faut le handle de l'application pour émettre, et `setup` ne tourne pas pendant
    // `build()` dans Tauri 2 mais au démarrage de `run()`.
    //
    // **Ne pas pouvoir l'ouvrir n'empêche pas Ash de démarrer.** Un second Ash lancé par
    // mégarde, ou un `~/.ash/` en lecture seule, coûtent les états d'agent — pas le
    // terminal, ni la sidebar, ni git. Refuser de s'ouvrir pour ça enlèverait à
    // l'utilisateur bien plus que ce que ça lui rendrait ; le message sur la sortie
    // d'erreur est ce qui rend la panne trouvable.
    let events = features::agents::listen(Arc::new(HookEvents {
        app: app.handle().clone(),
        ptys: Arc::clone(&ptys),
    }));
    let events = match events {
        Ok(socket) => Some(socket),
        Err(why) => {
            eprintln!("ash: les états d'agent n'arriveront pas : {why}");
            None
        }
    };

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
            // Le fichier du socket ne part pas avec le processus : le laisser derrière soi
            // est ce qui empêcherait le démarrage suivant de se lier.
            if let Some(events) = events.as_ref() {
                events.stop();
            }
        }
        _ => {}
    });

    Ok(())
}
