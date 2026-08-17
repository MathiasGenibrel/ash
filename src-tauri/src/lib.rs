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

/// Le nom sous lequel Ash se présente — et il n'est **pas** le même en debug.
///
/// Ash est le terminal quotidien de son auteur : une instance installée tourne pendant
/// qu'on en développe une autre. Deux fenêtres du même nom, avec la même icône, c'est une
/// commande tapée dans la mauvaise, un bug attribué au mauvais binaire, et un agent `qa`
/// qui rend son verdict sur l'application qu'il n'a pas construite. La compilation en
/// debug porte donc un autre nom, et
/// [`tauri.dev.conf.json`](../tauri.dev.conf.json) lui donne l'icône aux couleurs
/// inversées et son propre identifiant de paquet.
///
/// `debug_assertions` est le bon interrupteur parce que c'est **exactement** celui que
/// `tauri build --debug` laisse allumé et que `tauri build` éteint : le nom compilé ici et
/// le nom du paquet ne peuvent donc pas diverger.
#[cfg(debug_assertions)]
pub const APP_NAME: &str = "Ash-dev";

/// Le nom d'Ash installé. Voir la variante `debug_assertions` juste au-dessus.
#[cfg(not(debug_assertions))]
pub const APP_NAME: &str = "Ash";

/// Rend [`APP_NAME`] à la webview, qui l'écrit dans ses bandes de titre.
///
/// **Une commande, et non un event ni un signal**, contrairement au thème et à la taille de
/// police : ceux-là changent pendant la session — un menu, un `⌘+` — et ont donc besoin
/// d'un aller *et* d'un retour. Le nom, lui, est fixé à la compilation par
/// `debug_assertions` : il ne peut pas changer tant que le processus tourne. Un signal
/// n'aurait jamais rien à diffuser, et le seul abonnement qu'il vendrait serait un
/// abonnement mort.
///
/// **Elle vit dans le composition root plutôt que dans une feature**, ce qui est
/// l'exception dans ce crate. Aucune feature n'est propriétaire de l'identité de
/// l'application : `theme` détient une préférence d'apparence, `settings` une fenêtre et
/// des blocs de hooks, `pty` des onglets — aucune ne *décide* du nom. Le loger dans l'une
/// d'elles lui ferait revendiquer ce qu'elle ne tient pas, et donnerait au lecteur une
/// seconde adresse pour un fait qui n'en a qu'une (voir `CLAUDE.md` : `APP_NAME` est la
/// seule source du nom affiché). Elle reste ici parce qu'elle est exactement aussi grosse
/// que la constante qu'elle rend : pas d'état, pas de chemin faillible, rien à tester.
/// **Une deuxième commande dans ce fichier serait le signe qu'une feature manque**, pas
/// une invitation à en ajouter une troisième.
#[tauri::command]
fn app_name() -> &'static str {
    APP_NAME
}

use std::path::Path;
use std::sync::{Arc, OnceLock};

use features::agents::{
    Adapter, ClaudeCodeAdapter, EventFrame, EventSink, GenericAdapter, Notice, Notifier, Presence,
    Supervisor, TabAgents, SUBAGENT_LINGER,
};
use features::git::{resolve_worktree, SystemFileSystem};
use features::notifications::{Authorization, Banner, Banners, SystemBanners};
use features::probe::SystemProbe;
use features::pty::{
    AgentStates, PtyRegistry, RepoRef, SystemPtySpawner, TabId, TabLocation, WorktreeLocator,
};
use features::settings::{
    AdapterProfile, BlockAt, ConfigTarget, HookBlocks, SystemCommands, SystemConfigFiles,
    ToolRegistry, Verifier,
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

/// Relie le port du socket d'events au registre des onglets, et au superviseur d'états.
///
/// C'est ici, et seulement ici, que les deux features se rencontrent : `agents` ne connaît
/// que son trait, `pty` ne sait rien des hooks. La corrélation, elle, est déjà tranchée par
/// [ADR-0007](../../docs/adr/0007-etats-par-hooks.md) — `ASH_TAB_ID`, que le registre a posé
/// sur le shell de chaque onglet, et que la descendance de ce shell hérite jusqu'au hook.
///
/// Un événement livré ne traverse **pas** la frontière Tauri : il entre dans la machine à
/// états de son onglet, et c'est la boucle de sonde qui portera le verdict jusqu'à la
/// webview, avec le reste de ce que l'onglet montre
/// ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
struct HookEvents {
    ptys: Arc<PtyRegistry>,
    agents: Arc<Supervisor>,
}

impl EventSink for HookEvents {
    fn knows(&self, tab_id: &str) -> bool {
        self.ptys.knows(tab_id)
    }

    fn deliver(&self, event: &EventFrame) {
        self.agents.on_hook(event);
    }
}

/// L'event par lequel une sélection décidée par le backend atteint la vue.
///
/// ## Pourquoi un event, alors qu'`agents` n'en a plus
///
/// `features::agents` a **retiré** le sien, `ash://agent-event`, et son mod-doc dit qu'il
/// n'y en aura pas d'autre. La raison du retrait était précise : cet event poussait un
/// **état** — un verbe brut — dans la webview, en doublon du `TabInfo` que `ash://tab-changed`
/// porte déjà, et personne ne l'écoutait. Un état d'agent a une seule route jusqu'à l'écran
/// ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
///
/// **Ce que celui-ci porte n'est pas un état, et n'en est pas une seconde source.** C'est un
/// geste de l'utilisateur sur un objet de fenêtre, exactement comme `ash://menu-action` : le
/// clic sur une bannière et le choix d'une entrée de menu sont la même chose vue du produit,
/// et `menu.rs` — posé, lui aussi, à côté du composition root plutôt que dans une feature —
/// route déjà `select-tab` par ce chemin. La sélection, elle, vit côté frontend et doit y
/// vivre : ADR-0009 donne au backend l'**état** d'un agent, pas la **vue** qu'on en a, et le
/// jour du démon `ashd` chaque vue tiendra la sienne.
///
/// Le backend décide donc **quel** onglet, et le frontend le rend actif. C'est aussi pour ça
/// que l'event ne s'appelle pas `banner-clicked` : ce qui traverse est la décision, pas le
/// geste qui l'a produite.
///
/// Il est déclaré ici plutôt que dans un `commands.rs` parce qu'aucune feature ne peut le
/// porter : `notifications` ignore ce qu'est un onglet, et `agents` ignore ce qu'est une
/// fenêtre. Le seul endroit qui sache les deux est celui qui les assemble.
const SELECT_TAB_EVENT: &str = "ash://select-tab";

/// Relie le port de notification de `features::agents` au centre de notifications de macOS.
///
/// **L'adaptateur arrive après coup, et il n'y a pas d'autre créneau** : le superviseur est
/// assemblé avant `tauri::Builder`, puisque le registre de PTY en dépend, et le rappel du
/// clic a besoin de l'`AppHandle`, qui n'existe qu'après `build()`. Le `OnceLock` est le
/// prix de cet ordre-là ; il est ici, dans l'assemblage, plutôt que dans la feature, qui n'a
/// pas à connaître le cycle de vie d'une application Tauri. Avant qu'il ne soit posé —
/// c'est-à-dire pendant le démarrage — une notification est perdue, et c'est sans
/// conséquence : aucun agent n'a encore parlé.
///
/// **Un `OnceLock` jamais rempli serait muet à l'exécution, mais il ne peut pas arriver
/// jusque-là** : [`Self::attach`] est privée et n'a qu'un appelant, donc la retirer du
/// câblage la rend morte et `cargo clippy -- -D warnings` échoue à la compilation. C'est la
/// différence avec le `state()` appelé avant son `manage()` qui avait cassé le démarrage :
/// cette panne-ci se voit avant de tourner. **Ne la faire taire ni par `#[allow(dead_code)]`
/// ni par `#[expect(dead_code)]`** — ce serait échanger une erreur de build contre des
/// bannières qui n'arrivent jamais, sans rien qui le dise.
///
/// Il n'y a aucune décision ici — un texte déjà écrit, une bannière — et c'est délibéré :
/// ce qu'Ash notifie, quand, et avec quels mots est décidé par `features::agents::notify`,
/// où ça se prouve. `notice.tab_id` devient le `payload` de la bannière, la chaîne opaque
/// que `features::notifications` rendra telle quelle au clic.
#[derive(Default)]
struct AppNotifier {
    banners: OnceLock<Arc<dyn Banners>>,
}

impl AppNotifier {
    fn attach(&self, banners: Arc<dyn Banners>) {
        let _ = self.banners.set(banners);
    }
}

impl Notifier for AppNotifier {
    fn post(&self, notice: Notice) {
        let Some(banners) = self.banners.get() else {
            return;
        };
        // Une notification perdue ne change aucun état : rien ne dépend de sa réussite, et
        // faire remonter l'échec n'apprendrait rien de plus que ce que la section
        // `notifications` des réglages dit déjà.
        banners.post(Banner {
            payload: notice.tab_id,
            title: notice.title,
            body: notice.body,
        });
    }
}

/// Le centre de notifications quand il n'y en a pas — c'est-à-dire en développement.
///
/// `bun run tauri dev` et `bun run smoke` lancent `target/debug/ash`, un binaire nu :
/// `UNUserNotificationCenter` n'y existe pas pour Ash, et le seul fait de le demander tue le
/// processus (voir `features/notifications/macos.rs`). Ash tourne alors sans bannière, et le
/// dit honnêtement dans la fenêtre de réglages — [`Authorization::Undisclosed`] est
/// exactement « macOS ne nous le dit pas ».
///
/// Refuser de démarrer pour ça enlèverait à l'utilisateur bien plus que ça ne lui rendrait,
/// comme pour le socket d'events : les bannières valent moins que le terminal.
struct NoBanners;

impl Banners for NoBanners {
    fn post(&self, _banner: Banner) {}

    fn authorization(&self) -> Authorization {
        Authorization::Undisclosed
    }
}

/// Relie le port d'états de `pty` au superviseur de `features::agents`.
///
/// Il n'y a aucune décision ici — une question, une délégation — et c'est délibéré : le
/// composition root n'a pas de test unitaire, donc tout ce qui s'y glisse n'en a pas non
/// plus. La règle, elle, vit dans `agents/supervisor.rs`, où elle se prouve.
struct SupervisedTabs(Arc<Supervisor>);

impl AgentStates for SupervisedTabs {
    fn state(&self, tab_id: &TabId, seen: Presence) -> TabAgents {
        self.0.state(tab_id, seen)
    }

    fn forget(&self, tab_id: &TabId) {
        self.0.forget(tab_id);
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
    fn inspect(&self, adapter: &str, config_dir: &ConfigTarget) -> Option<BlockAt> {
        let instrumentation = self.describing(adapter, config_dir.resolved())?;
        Some(BlockAt {
            file: instrumentation.file.clone(),
            presence: features::hooks::inspect(&*self.files, &instrumentation),
        })
    }

    fn install(&self, adapter: &str, config_dir: &ConfigTarget) -> Result<(), String> {
        let instrumentation = self
            .describing(adapter, config_dir.resolved())
            .ok_or_else(|| format!("the {adapter} adapter has no hooks to install"))?;
        features::hooks::install(&*self.files, &instrumentation)
            .map(|_| ())
            .map_err(|why| why.to_string())
    }

    fn remove(&self, adapter: &str, config_dir: &ConfigTarget) -> Result<(), String> {
        let instrumentation = self
            .describing(adapter, config_dir.resolved())
            .ok_or_else(|| format!("the {adapter} adapter wrote nothing to remove"))?;
        features::hooks::uninstall(&*self.files, &instrumentation)
            .map(|_| ())
            .map_err(|why| why.to_string())
    }
}

/// Les adaptateurs que **cette** application embarque, et ce que la vérification en sait.
///
/// C'est ici qu'on les connaît, et nulle part ailleurs : la feature `settings` n'a donc pas
/// à connaître leurs implémentations, ni `agents` à savoir lesquels sont livrés
/// ([ADR-0008](../../docs/adr/0008-abstraction-adapter.md)). Un adaptateur de plus est une
/// ligne de plus ici, et rien à changer dans les réglages.
///
/// **Le profil est la traduction d'un adaptateur en ce que la vérification sait regarder** :
/// de la donnée, et non le trait lui-même. C'est ce qui laisse `settings` ignorer
/// `GenericAdapter` comme il ignorera les autres — et c'est ici, au seul endroit qui connaît
/// les deux, que la traduction se fait.
///
/// `generic` ne signe rien et n'impose aucun dossier, et ce n'est pas un manque : il est
/// l'adaptateur de l'outil dont on ne sait rien. La séquence en tire une **réserve** — le
/// dossier est accepté, mais rien ne prouve que la commande le lit — au lieu de lancer un
/// programme pour une question à laquelle il ne saurait pas répondre.
fn embedded_adapters() -> (Vec<Arc<dyn Adapter>>, Vec<AdapterProfile>) {
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

    (adapters, profiles)
}

/// Assemble et démarre l'application.
///
/// Composition root : c'est le seul endroit du crate où les implémentations concrètes
/// des effets système sont choisies et injectées. `SystemPtySpawner` et `SystemProbe`
/// n'apparaissent qu'ici ; partout ailleurs les features ne connaissent que leurs traits.
pub fn run() -> tauri::Result<()> {
    // Les adaptateurs sont assemblés **en premier** : ce sont eux qui traduisent les verbes
    // des hooks, donc le superviseur d'états en dépend, et le registre de PTY dépend du
    // superviseur pour savoir quoi montrer d'un onglet.
    let (adapters, profiles) = embedded_adapters();

    // La bannière macOS de la spec §8. Elle est câblée **avant** le superviseur parce que
    // celui-ci la détient, et rattachée à l'application après `build()` : voir
    // [`AppNotifier`].
    let notifier = Arc::new(AppNotifier::default());

    let agents = Arc::new(Supervisor::new(
        Arc::new(shared::time::SystemClock),
        adapters.clone(),
        Arc::clone(&notifier) as Arc<dyn Notifier>,
        // Le réglage de la spec §6.5, à sa valeur par défaut : combien de temps la ligne
        // d'un sous-agent fini reste lisible. Il est posé **ici** et non lu d'une constante
        // au fond de la feature, pour que le jour où la fenêtre de réglages le porte, il n'y
        // ait qu'un fil à rebrancher.
        SUBAGENT_LINGER,
    ));

    let ptys = Arc::new(PtyRegistry::new(
        Box::new(SystemPtySpawner),
        Arc::new(SystemProbe),
        Arc::new(GitWorktrees),
        Arc::new(SupervisedTabs(Arc::clone(&agents))),
    ));

    // L'apparence — le thème et la taille de police du terminal — est relue **avant** la
    // construction du menu : ses trois coches disent le mode en cours, et le menu est bâti
    // une seule fois, avant que la webview n'existe.
    let theme = Arc::new(ThemeState::restore(
        Arc::new(FileThemeStore::in_home()) as Arc<dyn ThemeStore>
    ));
    let theme_mode = theme.mode();

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
            app_name,
            features::pty::commands::pty_open,
            features::pty::commands::pty_write,
            features::pty::commands::pty_resize,
            features::pty::commands::pty_ack,
            features::pty::commands::pty_close,
            features::pty::commands::pty_tabs,
            features::pty::commands::pty_has_foreground_process,
            features::git::commands::git_metadata,
            features::theme::commands::theme_mode,
            features::theme::commands::terminal_font_size,
            features::settings::commands::settings_notifications,
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

    {
        // Le titre de la fenêtre vient d'ici plutôt que de la configuration : la
        // configuration de développement ne surcharge que des valeurs scalaires
        // (`productName`, `identifier`, l'icône), parce qu'y redéclarer `app.windows`
        // remplacerait le tableau entier — donc aussi la taille et le style de la barre de
        // titre, qui n'ont rien à voir avec le nom. Une seule source pour le nom, et c'est
        // [`APP_NAME`].
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_title(APP_NAME);
        }
    }

    // Le port de notification n'avait pas d'application à qui parler avant cette ligne. Il
    // la reçoit ici, dans le même créneau que la surveillance git et le socket d'events, et
    // pour la même raison : `setup` ne tourne pas pendant `build()` dans Tauri 2.
    //
    // **C'est aussi le seul créneau où le délégué du clic peut être posé** : Apple demande
    // qu'il le soit avant que l'application ne tourne, et il lui faut le handle pour émettre.
    // Nous sommes ici sur le fil principal, avant `app.run`.
    //
    // Le rappel du clic est le point d'arrivée de toute cette tranche, et il tient en une
    // ligne : la chaîne que la bannière portait est l'onglet à sélectionner. Il est
    // **asynchrone** — macOS le déclenche quand l'utilisateur clique, et aucun fil d'Ash
    // n'attend ce moment. Il ne met pas non plus la fenêtre au premier plan : cliquer une
    // notification active déjà l'application qui l'a posée, et Ash n'a rien à ajouter à un
    // geste que l'utilisateur vient de faire (ADR-0010, ADR-0015).
    let clicking = app.handle().clone();
    let banners: Arc<dyn Banners> = match SystemBanners::attach(Arc::new(move |tab_id: &str| {
        use tauri::Emitter;
        let _ = clicking.emit(SELECT_TAB_EVENT, tab_id);
    })) {
        Some(system) => Arc::new(system),
        None => Arc::new(NoBanners),
    };
    notifier.attach(Arc::clone(&banners));
    {
        // La fenêtre de réglages lit l'autorisation par le **même** centre que celui qui
        // poste : `settings_notifications` la lui demande, et ne la déduit de rien.
        use tauri::Manager;
        app.manage(Arc::clone(&banners));
    }

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
        ptys: Arc::clone(&ptys),
        agents: Arc::clone(&agents),
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
            event: tauri::WindowEvent::Focused(focused),
            ..
        } => {
            // Le focus est ce qui décide qu'une ligne `done` a été **vue**, donc qu'elle a le
            // droit de s'effacer au bout de trente secondes (spec §6.4). Sans ce signal, un
            // agent qui finit pendant qu'Ash est derrière l'éditeur laisserait sa ligne
            // affichée pour toujours. C'est immédiat et sans disque, donc sur ce fil-ci.
            agents.on_window_focus(focused);

            if focused {
                let refreshing = Arc::clone(&git_watch);
                std::thread::spawn(move || refreshing.on_focus());
            }
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
