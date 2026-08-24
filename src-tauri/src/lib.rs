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

/// La réunion des deux genres d'onglet — `Shell | Merge` (ADR-0003).
mod tabs;

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
    Adapter, ClaudeCodeAdapter, EventFrame, EventSink, FileNotificationStore, FileTranscripts,
    GenericAdapter, Notice, NotificationPreferences, NotificationStore, Notifier, Presence,
    Supervisor, SystemToolConfig, TabAgents, ToolConfig, Transcripts, SUBAGENT_LINGER,
};
use features::card::{AgentWork as CardWork, Cards, FileModeStore, SystemCardFiles, WorkRecord};
use features::git::{
    resolve_worktree, Attribution, Attributions, BusyAgent, CommitGraphReader, Entry, FileSystem,
    GraphLog, Head, InhabitingTab, MetadataWatch, OperationKind, StoppedOperation,
    SystemFileSystem, SystemGit, TabPresence, TreeWriter, WorkHistory, Worked, WorkingAgents,
    WorktreeTable,
};
use features::journal::{
    CommitJournal, CommitLog as JournalCommits, CommitRecord, FileJournalStore, JournalStore,
    TabAgent, Tabs as JournalTabs,
};
use features::links::{Files, LaunchServices, Opener, SystemFiles};
use features::merge::{ConflictFiles, MergeOutcome, MergeSurface, StoppedWorktree, TreeGit};
use features::notifications::{Authorization, Banner, Banners, SystemBanners};
use features::probe::SystemProbe;
use features::pty::{
    AgentStates, PtyRegistry, RepoRef, SystemPtySpawner, TabId, TabLocation, WorktreeLocator,
};
use features::settings::{
    AdapterProfile, BlockAt, ConfigTarget, FileToolStore, HookBlocks, RunningTools, SystemCommands,
    SystemConfigFiles, ToolRecognition, ToolRegistry, ToolStore, ToolSuggestions, Verifier,
};
use features::shortcuts::{BindingStore, Bindings, FileBindingStore};
use features::sidebar::{
    FileSidebarStore, PinnedRepo, PinnedWorktree, SidebarState, SidebarStore, WorktreePlaces,
};
use features::theme::{FileThemeStore, FontCatalog, SystemFontCatalog, ThemeState, ThemeStore};
use features::usage::{
    AccountUsage, AnthropicUsage, Credentials, FileUsageStore, KeychainTokens, TokenSource,
    UsageApi, UsagePoller, UsagePreferences, UsageSink, UsageStore, ACCOUNT_USAGE_EVENT,
};

/// Relie le port de `pty` à la résolution de `features::git`.
///
/// C'est ici, et seulement ici, que les deux features se rencontrent : `pty` ne connaît
/// que son trait, `git` ne sait rien des onglets. L'adaptateur ne fait que traduire — la
/// règle « un dépôt sans worktree lié s'affiche à plat »
/// ([ADR-0012](../../docs/adr/0012-worktree-unite-de-travail.md)) est déjà tranchée par
/// `resolve_worktree`, qui rend alors un worktree sans dépôt.
/// Relie le port `UsageSink` de `features::usage` à la webview.
///
/// C'est ici, et seulement ici, que la feature la plus isolée du crate rencontre Tauri :
/// elle ne connaît qu'un trait d'une méthode, et n'a aucun moyen d'émettre autre chose que
/// les deux quotas ([ADR-0016](../../docs/adr/0016-ash-sort-sur-le-reseau.md)).
///
/// Échouer à émettre veut dire qu'il n'y a plus de webview à prévenir : rien à rattraper, et
/// surtout pas de panique sur le fil de fond.
struct UsageEvents<R: tauri::Runtime>(tauri::AppHandle<R>);

impl<R: tauri::Runtime> UsageSink for UsageEvents<R> {
    fn publish(&self, usage: AccountUsage) {
        use tauri::Emitter;
        let _ = self.0.emit(ACCOUNT_USAGE_EVENT, usage);
    }
}

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

/// Relie l'onglet de merge à ce que la surveillance git sait déjà de l'opération arrêtée.
///
/// Troisième rencontre du même genre que [`GitWorktrees`] : `merge` ne connaît que son
/// port, `git` ne sait rien des onglets. La lecture est celle de #29, réutilisée telle
/// quelle — aucun second chemin ne lit un rebase arrêté.
struct WatchedConflicts(Arc<MetadataWatch>);

impl StoppedWorktree for WatchedConflicts {
    fn stopped(&self, worktree_root: &Path) -> Option<StoppedOperation> {
        self.0.stopped(worktree_root)
    }

    fn head(&self, worktree_root: &Path) -> Option<Head> {
        self.0.metadata(worktree_root).map(|metadata| metadata.head)
    }
}

/// Le seul endroit d'Ash qui **réécrive** un fichier de travail de l'utilisateur.
///
/// Il est ici, au composition root, et pas dans `features::git` : le `FileSystem` de cette
/// feature-là est en lecture seule et doit le rester. La résolution de worktree et la
/// surveillance de `.git` n'ont aucune raison de gagner le droit d'écrire par le seul fait
/// qu'elles partagent un trait avec l'onglet de merge.
struct WorktreeFiles;

impl ConflictFiles for WorktreeFiles {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn write(&self, path: &Path, text: &str) -> Result<(), String> {
        std::fs::write(path, text).map_err(|why| why.to_string())
    }
}

/// Les deux verbes qui écrivent, sur le **même** `SystemGit` que tout le reste.
///
/// Un seul objet parce que c'est un seul binaire, et parce que le durcissement qui
/// l'encadre — `core.fsmonitor=false`, `core.pager=cat`, `core.editor=true`, aucun shell —
/// doit valoir pour toutes les questions à la fois. La justification de ce qui n'est
/// **pas** neutralisé — les hooks du projet — est dans `features::merge::ports::TreeGit`.
struct MergeGit(Arc<dyn TreeWriter>);

impl TreeGit for MergeGit {
    fn stage(&self, worktree_root: &Path, path: &str) -> MergeOutcome {
        // Le `--` sépare les options des opérandes : un fichier nommé `--upload-pack=…`
        // existe, contrairement à une branche du même nom.
        let args = vec!["add".to_owned(), "--".to_owned(), path.to_owned()];
        outcome(format!("Stage {path}"), self.0.run(worktree_root, &args))
    }

    fn resume(&self, worktree_root: &Path, kind: OperationKind) -> MergeOutcome {
        let verb = match kind {
            OperationKind::Rebase => "rebase",
            OperationKind::Am => "am",
            OperationKind::Merge => "merge",
        };
        let args = vec![verb.to_owned(), "--continue".to_owned()];
        outcome(
            format!("git {verb} --continue"),
            self.0.run(worktree_root, &args),
        )
    }
}

/// Ce que rend une invocation qui n'a même pas pu être lancée — `git` absent du `PATH`.
///
/// Un échec nommé, jamais un succès silencieux : l'écran affiche cette phrase telle quelle.
fn outcome(label: String, completed: Option<features::git::Completed>) -> MergeOutcome {
    match completed {
        Some(completed) => MergeOutcome {
            label,
            success: completed.success,
            output: completed.output,
        },
        None => MergeOutcome {
            label,
            success: false,
            output: "git could not be run".to_owned(),
        },
    }
}

/// Relie le port des épingles à la même résolution, et au système de fichiers.
///
/// C'est la seconde rencontre entre `git` et une feature qui ne le connaît pas, et elle se
/// fait ici pour la raison qui vaut pour [`GitWorktrees`] : `sidebar` ne sait pas ce qu'est
/// un dépôt, `git` ne sait pas ce qu'est une épingle.
///
/// **Le chemin doit être un dossier, et c'est vérifié avant la résolution.**
/// `resolve_worktree` remonte les dossiers parents à la recherche d'un `.git` : une racine
/// épinglée devenue un **fichier** — un `git worktree move` suivi d'une archive posée au même
/// nom — se résoudrait en `/dev/ash`, et la ligne d'un worktree disparu se mettrait à
/// désigner le dépôt principal, en silence. Le cas du dossier simplement **supprimé** est
/// déjà couvert un cran plus bas, `resolve_worktree` commençant par canonicaliser ; les deux
/// donnent `None`, et la conduite qui s'ensuit est dans `features::sidebar::state` : la ligne
/// s'efface, l'épingle reste.
struct GitPins;

impl WorktreePlaces for GitPins {
    fn place(&self, root: &Path) -> Option<PinnedWorktree> {
        if SystemFileSystem.entry(root) != Some(Entry::Directory) {
            return None;
        }
        let located = resolve_worktree(&SystemFileSystem, root).ok()?;

        Some(PinnedWorktree {
            worktree_root: located.worktree.root.display().to_string(),
            worktree_name: located.worktree.name,
            repo: located.repo.map(|repo| PinnedRepo {
                id: repo.git_dir.display().to_string(),
                name: repo.name,
            }),
        })
    }
}

/// Relie le port « ce qu'Ash a vu tourner » de `settings` au registre des onglets.
///
/// C'est le sens **inverse** de la reconnaissance, et c'est pour ça qu'il passe par ici :
/// `pty` demande déjà à `settings` de reconnaître un programme, et faire dépendre `settings`
/// de `pty` en retour ferait se tenir les deux features par les deux bouts. Chacune garde
/// donc son trait, et le seul objet qui connaît les deux est celui qui les assemble
/// ([ADR-0006](../../docs/adr/0006-decouverte-automatique-des-agents.md)).
///
/// Rien n'est découvert ici : `recognized_tools()` rend ce que la dernière passe de sonde a
/// annoncé — aucun `PATH`, aucun disque, aucune autorisation.
struct TabTools(Arc<PtyRegistry>);

impl RunningTools for TabTools {
    fn running(&self) -> Vec<features::agents::RecognizedProvider> {
        self.0.recognized_tools().unwrap_or_default()
    }
}

/// Relie le port « qui travaille ici » de `git` au registre des onglets.
///
/// C'est la troisième rencontre entre `git` et une feature qui ne le connaît pas, et elle se
/// fait ici pour la même raison que les deux premières : `git` ne sait pas ce qu'est un
/// onglet, `pty` ne sait pas ce qu'est une branche. Ce qui traverse est l'avertissement de la
/// spec §7.1 — **le nom** de l'agent qu'un checkout dérangerait, pas le fait qu'il y en ait un.
///
/// La règle « en danger » vient de `git` (`working_agents::at_risk`), pas d'ici : le
/// composition root n'a pas de test unitaire, donc tout ce qui s'y glisse n'en a pas non plus.
struct TabAgentsInWorktree(Arc<PtyRegistry>);

impl WorkingAgents for TabAgentsInWorktree {
    fn in_worktree(&self, worktree_root: &Path) -> Vec<BusyAgent> {
        let here = worktree_root.display().to_string();
        // La liste des onglets, pas une sonde de plus : `tabs()` rend déjà le `cwd` sondé,
        // l'état d'agent et la localisation résolue, tous pris à la même passe.
        self.0
            .tabs()
            .unwrap_or_default()
            .into_iter()
            .filter(|tab| {
                tab.location
                    .as_ref()
                    .is_some_and(|located| located.worktree_root == here)
            })
            .filter(|tab| features::git::at_risk(tab.state))
            .map(|tab| BusyAgent {
                tab_id: tab.tab_id,
                name: tab.process,
                state: tab.state,
                paused: tab.paused,
            })
            .collect()
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

/// Relie le port de la fiche de branche au journal d'attribution, et à git.
///
/// La fiche demande *qui a écrit quoi dans ce worktree* ; la réponse tient en deux features
/// dont aucune ne connaît la fiche. `features::git` sait seul lancer `git log` — derrière la
/// frontière de sécurité de `git_cli.rs`, et c'est toujours le seul endroit du dépôt qui
/// lance le binaire —, et `features::journal` sait seul qui tenait l'avant-plan quand chacun
/// de ces commits est né ([ADR-0014](../../docs/adr/0014-attribution-locale-des-commits.md)).
///
/// La résolution du worktree vers son dossier git **commun** est ici aussi, et pour la même
/// raison qu'elle est dans [`GitWorktrees`] : c'est la clé sous laquelle le journal range un
/// dépôt, et `features::card` n'a pas à savoir ce qu'est un dépôt.
///
/// Il n'y a aucune décision ici — une résolution, une lecture, une projection sur deux
/// champs. Ce que la table dit d'un travail vit dans `card/log.rs`, où ça se prouve.
struct CardWorkFromJournal {
    journal: Arc<CommitJournal>,
    git: SystemGit,
}

impl CardWork for CardWorkFromJournal {
    fn in_worktree(&self, worktree_root: &Path) -> Vec<WorkRecord> {
        let Ok(located) = resolve_worktree(&SystemFileSystem, worktree_root) else {
            return Vec::new();
        };
        let Some(repo) = located.repo.map(|repo| repo.git_dir.display().to_string()) else {
            return Vec::new();
        };
        self.git
            .recent_commits(worktree_root)
            .iter()
            .filter_map(|commit| {
                // Un commit qu'Ash n'a pas vu naître n'a pas d'agent, et la fiche ne lui en
                // invente pas : la colonne resterait vide dans le graphe aussi.
                let entry = self.journal.attribution(&repo, commit)?;
                if entry.agent.is_empty() {
                    return None;
                }
                Some(WorkRecord {
                    agent: entry.agent,
                    authored_at: commit.authored_at,
                })
            })
            .collect()
    }
}

/// Relie le port du journal au registre d'onglets.
///
/// C'est par lui qu'ADR-0014 tient sa promesse : l'attribution « ne dépend que de la
/// sonde ». Le registre sait quel outil tient l'avant-plan de quel onglet (ADR-0006) et où
/// cet onglet se situe (ADR-0012) ; le journal ne connaît ni les onglets, ni les PTY, ni la
/// table des outils.
///
/// Il n'y a aucune décision ici — une lecture, une projection sur quatre champs — et c'est
/// délibéré : la règle qui choisit *quel* agent d'un worktree se voit attribuer un commit
/// vit dans `journal/tabs.rs`, où elle se prouve.
struct TabAuthors(Arc<PtyRegistry>);

impl JournalTabs for TabAuthors {
    fn snapshot(&self) -> Vec<TabAgent> {
        self.0
            .tabs()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|tab| {
                Some(TabAgent {
                    tab_id: tab.tab_id,
                    worktree_root: tab.location?.worktree_root,
                    // La **commande**, pas l'adaptateur : ADR-0014 écrit `claude` et
                    // `codex`, et c'est aussi ce qui distingue `claude` de `claude-perso`,
                    // deux entrées d'un même adaptateur (spec §9).
                    agent: tab.agent.map(|agent| agent.command),
                    since: tab.state_since,
                })
            })
            .collect()
    }
}

/// Relie le port des onglets du **tableau des worktrees** au registre de PTY.
///
/// C'est la jointure que la spec §7.3 décrit en une phrase — « Ash les connaît parce qu'il
/// connaît le `cwd` de chaque onglet » — et c'est ici qu'elle se fait : `git` ne sait pas ce
/// qu'est un onglet, `pty` ne sait pas ce qu'est un tableau. Le consommateur possède le port,
/// le composition root relie, exactement comme [`TabAuthors`] juste au-dessus.
///
/// Il n'y a aucune décision ici — une lecture, une projection sur cinq champs — et c'est
/// délibéré : la règle qui décide ce qu'un agent présent dit d'un worktree vit dans
/// `git/table.rs`, où elle se prouve.
struct InhabitedWorktrees(Arc<PtyRegistry>);

impl TabPresence for InhabitedWorktrees {
    fn inhabiting(&self) -> Vec<InhabitingTab> {
        self.0
            .tabs()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|tab| {
                Some(InhabitingTab {
                    tab_id: tab.tab_id,
                    worktree_root: std::path::PathBuf::from(tab.location?.worktree_root),
                    // La **commande**, comme pour l'attribution : `claude`, et non
                    // l'adaptateur qui le traduit (ADR-0006).
                    agent: tab.agent.map(|agent| agent.command),
                    state: tab.state,
                    since: tab.state_since,
                })
            })
            .collect()
    }
}

/// Relie la colonne `last worked by` du tableau au journal d'attribution.
///
/// Le journal est la **seule** mémoire d'un agent qui survive à la fermeture de son onglet :
/// ADR-0009 interdit d'en persister une autre, et ADR-0014 borne celle-ci aux commits qu'Ash
/// a vus naître. Un agent qui a travaillé sans rien valider n'y est pas — la colonne se tait
/// alors, plutôt que de nommer quelqu'un qu'Ash n'a pas observé.
struct JournalledWork(Arc<CommitJournal>);

impl WorkHistory for JournalledWork {
    fn last_worked(&self, repo: &Path, worktree_root: &Path) -> Option<Worked> {
        let worked = self
            .0
            .last_worked_in(&repo.to_string_lossy(), worktree_root)?;
        Some(Worked {
            agent: worked.agent,
            at: worked.at,
        })
    }
}

/// Relie le port du journal au seul endroit du dépôt où le binaire `git` est lancé.
///
/// `features/journal` pose la question — *quels commits `HEAD` porte-t-il ?* — et
/// `features/git` sait seul y répondre, derrière la frontière de sécurité de `git_cli.rs`.
/// Aucune des deux features ne dépend de l'autre : c'est ici qu'elles se rencontrent, comme
/// `pty` et `agents` se rencontrent dans [`SupervisedTabs`].
///
/// **Public**, seul de tous les branchements de ce fichier : `tests/journal_real_rebase.rs`
/// assemble le journal sur un vrai dépôt et un vrai `git`, et il doit le faire par le même
/// chemin que la production — une seconde définition du même branchement dériverait sans
/// que rien ne le dise.
pub struct GitCommits(pub SystemGit);

impl JournalCommits for GitCommits {
    fn recent(&self, worktree_root: &Path) -> Vec<CommitRecord> {
        self.0.recent_commits(worktree_root)
    }
}

/// Relie le port du graphe au journal d'attribution — **la colonne `by`** (spec §7.2).
///
/// C'est le troisième branchement du même genre, et il boucle la paire : `features/journal`
/// demande des commits à `features/git` par son port `CommitLog`, `features/git` demande des
/// agents à `features/journal` par son port `Attributions`. Les deux features restent sans
/// dépendance l'une envers l'autre — chacune possède la question qu'elle pose —, et le seul
/// endroit qui connaît les deux est celui qui les assemble.
///
/// Il n'y a aucune décision ici : la résolution en deux temps d'ADR-0014 vit dans
/// `journal/resolve.rs`, où elle se prouve, et le choix du mot affiché dans
/// `git/history.rs`, où il se prouve aussi.
struct JournalAttributions(Arc<CommitJournal>);

impl Attributions for JournalAttributions {
    fn of(&self, repo: &str, commits: &[CommitRecord]) -> Vec<Option<Attribution>> {
        self.0
            .attributions(repo, commits)
            .into_iter()
            .map(|entry| {
                entry.map(|entry| Attribution {
                    agent: entry.agent,
                    tab_id: entry.tab_id,
                    prompt: entry.prompt,
                })
            })
            .collect()
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

    fn remove(
        &self,
        adapter: &str,
        config_dir: &ConfigTarget,
    ) -> Result<features::hooks::Removal, String> {
        let instrumentation = self
            .describing(adapter, config_dir.resolved())
            .ok_or_else(|| format!("the {adapter} adapter wrote nothing to remove"))?;
        features::hooks::uninstall(&*self.files, &instrumentation).map_err(|why| why.to_string())
    }

    fn foresee_removal(
        &self,
        adapter: &str,
        config_dir: &ConfigTarget,
    ) -> Option<features::hooks::Withdrawal> {
        let instrumentation = self.describing(adapter, config_dir.resolved())?;
        features::hooks::foresee(&*self.files, &instrumentation)
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

    // Une seule horloge pour toute l'application : le superviseur date les états avec, et la
    // reconnaissance d'ADR-0006 mesure la fraîcheur de ce qu'elle a lu avec la même.
    let clock = Arc::new(shared::time::SystemClock);

    // Les trois interrupteurs de la spec §9, relus du disque avant tout : le superviseur les
    // consulte à chaque changement d'état, et c'est lui qui poste. Ils sont aussi confiés à
    // Tauri plus bas — la fenêtre de réglages les montre et les bascule, sans jamais les
    // détenir.
    let notification_preferences = Arc::new(NotificationPreferences::restore(Arc::new(
        FileNotificationStore::in_home(),
    )
        as Arc<dyn NotificationStore>));

    let agents = Arc::new(Supervisor::new(
        Arc::clone(&clock) as Arc<dyn shared::time::Clock>,
        adapters.clone(),
        Arc::clone(&notifier) as Arc<dyn Notifier>,
        Arc::clone(&notification_preferences),
        // Le réglage de la spec §6.5, à sa valeur par défaut : combien de temps la ligne
        // d'un sous-agent fini reste lisible. Il est posé **ici** et non lu d'une constante
        // au fond de la feature, pour que le jour où la fenêtre de réglages le porte, il n'y
        // ait qu'un fil à rebrancher.
        SUBAGENT_LINGER,
        // Par où la fin d'un transcript se lit. Le seul accès au disque de ce chemin, et il
        // n'a lieu qu'à l'arrivée d'un hook portant un `transcript_path` — jamais à une passe
        // de sonde.
        Arc::new(FileTranscripts) as Arc<dyn Transcripts>,
        // Par où la fenêtre de contexte se lit : la configuration de l'outil, qui nomme le
        // modèle. Même rythme que la ligne au-dessus — à l'arrivée d'un hook, et pas
        // ailleurs — et **rien d'autre que de la lecture** (ADR-0006) : aucun fichier écrit,
        // aucune autorisation macOS, aucun appel réseau.
        Arc::new(SystemToolConfig) as Arc<dyn ToolConfig>,
    ));

    // L'apparence — le thème et la taille de police du terminal — est relue **avant** la
    // construction du menu : ses trois coches disent le mode en cours, et le menu est bâti
    // une seule fois, avant que la webview n'existe.
    let theme = Arc::new(ThemeState::restore(
        Arc::new(FileThemeStore::in_home()) as Arc<dyn ThemeStore>
    ));
    let theme_mode = theme.mode();

    // Les polices installées, lues à la première demande et pas au démarrage : c'est la
    // fenêtre de réglages qui les fait lire, et parcourir les dossiers de polices de macOS
    // n'a rien à faire sur le chemin d'ouverture de la fenêtre principale.
    let fonts = Arc::new(SystemFontCatalog::on_this_mac()) as Arc<dyn FontCatalog>;

    // Les liaisons de raccourcis, relues **avant** le menu et pour la même raison que
    // l'apparence : c'est d'elles que chaque accélérateur du menu vient, et le menu est bâti
    // une seule fois au démarrage (`menu::build`). Une touche posée après coup serait une
    // seconde source de vérité. Les défauts, eux, viennent du menu — `menu::action_bindings`
    // est le seul point de contact entre les deux, et il ne va que dans ce sens.
    let shortcuts = Arc::new(Bindings::restore(
        Arc::new(FileBindingStore::in_home()) as Arc<dyn BindingStore>,
        menu::action_bindings(),
    ));

    // Les outils déclarés, **relus de la session précédente** (`~/.ash/tools.json`). Relus
    // avant la fenêtre, comme l'apparence et la sidebar, mais pour une raison de plus : la
    // reconnaissance d'ADR-0006 les consulte à chaque passe de la boucle de sonde, donc dès
    // le premier onglet — un outil déclaré doit être reconnu sans que personne n'ait ouvert
    // les réglages. Rien n'est vérifié ici : les quatre tests de la spec §9.1 lisent des
    // dossiers et lancent une commande, et une entrée relue repart *non vérifiée*.
    let tools = Arc::new(ToolRegistry::restore(
        Arc::new(Verifier::new(
            Arc::new(SystemConfigFiles),
            Arc::new(SystemCommands),
            profiles,
        )),
        Arc::new(AdapterHooks {
            adapters,
            files: Arc::new(features::hooks::SystemConfigFiles),
        }),
        Arc::new(FileToolStore::in_home()) as Arc<dyn ToolStore>,
    ));

    // Ce que la colonne garde d'une session à l'autre : les worktrees épinglés et les lignes
    // repliées (spec §3.1, §5.2). Relu **avant** la fenêtre, comme l'apparence : la sidebar le
    // demande en s'affichant, et une épingle qui apparaîtrait une seconde après l'ouverture se
    // lirait comme un sursaut.
    let sidebar_rows = Arc::new(SidebarState::restore(
        Arc::new(FileSidebarStore::in_home()) as Arc<dyn SidebarStore>,
        Arc::new(GitPins),
    ));

    // La reconnaissance d'ADR-0006, nommée parce qu'elle a **deux** lecteurs : le registre
    // de PTY, qui la reçoit par son port, et les suggestions de la fenêtre de réglages, qui
    // partagent sa mémoire courte — donc son unique lecture du `settings.json` d'un outil.
    let recognition = Arc::new(ToolRecognition::new(
        Arc::clone(&tools),
        Arc::clone(&clock) as Arc<dyn shared::time::Clock>,
    ));

    let ptys = Arc::new(PtyRegistry::new(
        Box::new(SystemPtySpawner),
        Arc::new(SystemProbe),
        Arc::new(GitWorktrees),
        // La reconnaissance d'ADR-0006 : la table embarquée d'`agents`, les entrées
        // déclarées de `settings`, et la précédence des secondes sur la première. C'est ici
        // — et seulement ici — que les trois features se rejoignent ; aucune ne connaît les
        // deux autres.
        Arc::clone(&recognition) as Arc<dyn features::pty::AgentRecognition>,
        Arc::new(SupervisedTabs(Arc::clone(&agents))),
        // La même `SystemProbe` que la sonde : c'est la feature qui connaît les processus au
        // sens du système, et la seule où l'`unsafe` est confiné. La pause d'ADR-0015 est
        // `SIGSTOP` sur le groupe que `tcgetpgrp` désigne — donc exactement ce que la sonde
        // sait déjà nommer.
        Arc::new(SystemProbe),
    ));

    // Le même `SystemGit` que la surveillance, sous ses deux autres traits : lire les refs et
    // les worktrees, et lancer les verbes qui touchent l'arbre. Un seul objet, parce que
    // c'est un seul binaire — et que tout ce qui l'encadre (le préfixe neutralisant, le
    // délai, l'absence de shell) doit valoir pour les trois questions à la fois. Le journal
    // interroge le même, par copie : `SystemGit` n'est qu'un délai, et `GitCommits` le tient
    // par valeur.
    let git = Arc::new(SystemGit::default());

    // Le journal d'attribution d'ADR-0014. Il naît **avant** la fenêtre parce que son
    // horloge est lue une fois, ici : ce qui est plus vieux qu'Ash n'a pas pu être observé
    // par lui, et cette borne est ce qui l'empêche de s'attribuer l'histoire d'un dépôt au
    // premier `git checkout`.
    //
    // Il est branché sur le registre d'onglets, et sur rien d'autre : l'attribution ne
    // dépend que de la sonde, donc elle marche pour tous les outils, hooks ou pas. Ce qui
    // l'alimente — la surveillance de `.git/logs/HEAD` — est câblé plus bas, après `build`,
    // avec le reste de la surveillance git.
    let journal = CommitJournal::watching(
        Arc::new(GitCommits(*git)),
        Arc::new(FileJournalStore::in_home()) as Arc<dyn JournalStore>,
        Arc::new(TabAuthors(Arc::clone(&ptys))),
        &shared::time::SystemClock,
    );

    // Le lecteur du graphe (#27, spec §7.2). Il naît ici parce que c'est ici que ses trois
    // ports se rencontrent : le seul endroit du dépôt où `git` est lancé, le journal
    // d'attribution, et l'horloge dont la règle des 30 jours a besoin.
    let commit_graph = Arc::new(CommitGraphReader::new(
        Arc::new(SystemGit::default()) as Arc<dyn GraphLog>,
        Arc::new(JournalAttributions(Arc::clone(&journal))),
        Arc::new(shared::time::SystemClock),
    ));

    // La fiche de branche d'ADR-0013. Elle ne connaît ni git, ni le journal, ni les
    // onglets : elle demande où elle vit, ce que le bloc porte, et qui a écrit dans ce
    // worktree — et c'est ici que les trois questions trouvent leurs répondants.
    let cards = Cards::new(
        Arc::new(SystemCardFiles),
        Arc::new(FileModeStore::in_home()),
        Arc::new(CardWorkFromJournal {
            journal: Arc::clone(&journal),
            git: SystemGit::default(),
        }),
        Arc::new(shared::time::SystemClock),
        std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("/")),
    );

    // L'interrupteur d'ADR-0016, relu **avant** la fenêtre : il décide si le fil de fond
    // sortira sur le réseau, et il ne doit pas y avoir un instant où l'appel part parce que
    // la préférence n'a pas encore été lue.
    let usage_preferences = Arc::new(UsagePreferences::restore(
        Arc::new(FileUsageStore::in_home()) as Arc<dyn UsageStore>,
    ));

    let app = tauri::Builder::default()
        .manage(Arc::clone(&ptys))
        // Le localisateur est **partagé** avec la réunion des onglets (`tabs.rs`) : un
        // onglet de merge se range dans la sidebar par la même résolution qu'un shell, et
        // deux résolutions parallèles finiraient par mettre le même worktree sur deux lignes.
        .manage(Arc::new(GitWorktrees) as Arc<dyn WorktreeLocator>)
        .manage(Arc::clone(&journal))
        .manage(Arc::clone(&commit_graph))
        .manage(Arc::clone(&cards))
        .manage(Arc::clone(&git) as Arc<dyn features::git::BranchReader>)
        .manage(Arc::clone(&git) as Arc<dyn features::git::TreeWriter>)
        .manage(Arc::new(TabAgentsInWorktree(Arc::clone(&ptys))) as Arc<dyn WorkingAgents>)
        .manage(Arc::clone(&theme))
        .manage(Arc::clone(&fonts))
        .manage(Arc::clone(&shortcuts))
        .manage(Arc::clone(&tools))
        .manage(Arc::new(ToolSuggestions::new(
            Arc::clone(&tools),
            Arc::clone(&recognition),
            Arc::new(TabTools(Arc::clone(&ptys))) as Arc<dyn RunningTools>,
        )))
        .manage(Arc::clone(&sidebar_rows))
        .manage(Arc::clone(&notification_preferences))
        .manage(Arc::clone(&usage_preferences))
        // Ce que la sidebar demande à la fenêtre de réglages de montrer, tant qu'elle ne
        // l'a pas lu (ADR-0006, ADR-0010).
        .manage(Arc::new(
            features::settings::commands::PendingFocus::default(),
        ))
        // Ce que le menu sait de l'instant : le worktree sous les yeux, et si `⌘⌃M` y a
        // quelque chose à ouvrir. C'est la seule entrée d'Ash qui s'éteigne — voir
        // `menu::MergeReach` pour les trois formes possibles et celle qui a été retenue.
        .manage(Arc::new(menu::MergeReach::default()))
        // Les deux ports de `features::links` — la troisième frontière de sécurité du
        // dépôt (voir son `mod.rs`). Ils sont assemblés ici et nulle part ailleurs : c'est
        // ce qui fait qu'aucune autre feature n'a de chemin vers `/usr/bin/open`.
        .manage(Arc::new(SystemFiles) as Arc<dyn Files>)
        .manage(Arc::new(LaunchServices) as Arc<dyn Opener>)
        .manage(spike::Flow::default())
        .menu({
            let shortcuts = Arc::clone(&shortcuts);
            // `false` : au démarrage, aucun onglet n'existe encore, donc aucun worktree
            // n'est sous les yeux — l'entrée « Resolve Conflicts » naît éteinte, et la
            // fenêtre l'allumera si son premier onglet tombe sur un rebase arrêté
            // (`menu::MergeReach`).
            move |app| menu::build(app, theme_mode, &shortcuts, false)
        })
        .on_menu_event(|app, event| menu::dispatch(app, event.id().as_ref()))
        .invoke_handler(tauri::generate_handler![
            app_name,
            features::pty::commands::pty_open,
            features::pty::commands::pty_write,
            features::pty::commands::pty_compose,
            features::pty::commands::pty_resize,
            features::pty::commands::pty_ack,
            features::pty::commands::pty_close,
            features::pty::commands::pty_tabs,
            features::quit::commands::quit_now,
            tabs::tabs,
            features::merge::commands::merge_open,
            features::merge::commands::merge_close,
            features::merge::commands::merge_view,
            features::merge::commands::merge_resolve,
            features::merge::commands::merge_continue,
            features::merge::commands::merge_rest_prompt,
            features::pty::commands::pty_has_foreground_process,
            features::pty::commands::pty_pause,
            features::pty::commands::pty_resume,
            features::git::commands::git_metadata,
            features::git::commands::git_commit_graph,
            features::links::commands::links_openable,
            features::links::commands::links_open,
            features::journal::commands::journal_summary,
            features::journal::commands::journal_purge,
            features::card::commands::branch_card,
            features::card::commands::branch_card_write_log,
            features::card::commands::branch_card_place,
            features::git::commands::git_branches,
            features::git::commands::git_branch_offers,
            features::git::commands::git_branch_action,
            features::git::commands::git_stopped_operation,
            features::git::commands::git_conflict_prompt,
            features::git::commands::git_worktrees,
            features::git::commands::git_worktree_removal,
            features::sidebar::commands::sidebar_rows,
            features::sidebar::commands::sidebar_pin,
            features::sidebar::commands::sidebar_collapse,
            features::theme::commands::theme_mode,
            features::theme::commands::terminal_font_size,
            features::theme::commands::step_terminal_font_size,
            features::theme::commands::sidebar_column,
            features::theme::commands::set_sidebar_column_width,
            features::theme::commands::set_sidebar_column_collapsed,
            features::theme::commands::toggle_sidebar_column,
            features::theme::commands::bottom_panel,
            features::theme::commands::set_bottom_panel_height,
            features::theme::commands::show_bottom_panel_view,
            features::theme::commands::close_bottom_panel,
            features::theme::commands::terminal_font,
            features::theme::commands::monospace_fonts,
            features::theme::commands::choose_terminal_font,
            features::theme::commands::sidebar_density,
            features::theme::commands::choose_sidebar_density,
            features::theme::commands::status_bar_layout,
            features::theme::commands::toggle_status_bar_segment,
            features::theme::commands::set_status_bar_layout,
            features::theme::commands::reset_status_bar_layout,
            // Les deux surfaces de l'apparence et la liste des raccourcis sont servies par
            // `menu.rs` et par `features::theme` : le choix de thème passe par le menu parce
            // qu'il doit corriger ses coches, la taille de police non — voir les deux
            // fonctions.
            menu::theme_set_mode,
            // La section `shortcuts` de la fenêtre de réglages (spec §4.4, issue #22). Ses
            // commandes sont dans `menu.rs` et non dans `features::shortcuts` parce qu'elles
            // ont toutes à **refaire le menu** : une feature n'a pas à connaître la forme
            // d'un menu, exactement comme pour `theme_set_mode` au-dessus.
            menu::menu_shortcuts,
            menu::shortcut_owner,
            menu::shortcut_keys,
            menu::shortcut_listening,
            menu::shortcut_preview,
            menu::shortcut_bind,
            menu::shortcut_clear,
            menu::shortcut_reset,
            menu::shortcut_reset_all,
            menu::shortcut_resolve,
            menu::menu_worktree_in_view,
            features::settings::commands::settings_notifications,
            features::settings::commands::settings_set_notification,
            features::settings::commands::settings_tools,
            features::settings::commands::settings_suggestions,
            features::settings::commands::settings_reveal_tool,
            features::settings::commands::settings_pending_focus,
            features::settings::commands::settings_proposed_config,
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
            features::settings::commands::settings_removal_plan,
            features::settings::commands::settings_remove_all_hooks,
            features::settings::commands::settings_usage,
            features::settings::commands::settings_set_usage_polling,
            features::usage::commands::usage_snapshot,
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

    // Le sondage des quotas (spec §4.2) naît dans le même créneau que la surveillance git,
    // et pour la même raison : il a besoin du handle pour émettre, et il ne peut pas être
    // posé depuis `setup`, qui ne tourne qu'au démarrage de `run()`.
    //
    // Ce qui s'assemble ici est la réunion des quatre ports de la feature — le trousseau, la
    // destination réseau, l'horloge, et la webview —, et c'est le seul endroit du crate où
    // ils se rencontrent. Le fil est **détaché** : la condition 1 d'ADR-0016 dit que
    // personne ne l'attend, et c'est vrai jusqu'ici — rien de ce qui suit ne le joint.
    let usage = Arc::new(UsagePoller::new(
        Arc::new(AnthropicUsage::new()) as Arc<dyn UsageApi>,
        Credentials::from(Arc::new(KeychainTokens) as Arc<dyn TokenSource>),
        usage_preferences,
        Arc::new(shared::time::SystemClock),
        Arc::new(UsageEvents(app.handle().clone())),
    ));
    {
        use tauri::Manager;
        app.manage(Arc::clone(&usage));
    }
    {
        // Le **niveau** de départ, lu à la fenêtre plutôt qu'attendu d'elle.
        //
        // La condition 2 d'ADR-0016 est un front *et* un niveau, et le front seul ne suffit
        // pas ici : `RunEvent::Focused` n'annonce qu'un **changement**, donc une fenêtre qui
        // naîtrait déjà devant n'en émettrait aucun, et le portillon resterait fermé jusqu'au
        // premier aller-retour vers une autre application. Les quotas ne seraient jamais
        // demandés d'une session qu'on n'a pas quittée.
        use tauri::Manager;
        let in_front = app
            .get_webview_window("main")
            .and_then(|window| window.is_focused().ok())
            .unwrap_or(false);
        usage.on_window_focus(in_front);
    }
    // Le fil, demandé à la feature plutôt que posé ici : c'est elle qui sait qu'un appel ne
    // part que d'un fil détaché, et c'est ce qui laisse la boucle **privée**. Aucune méthode
    // publique du poller n'attend le réseau, donc aucune commande ne peut en attendre un.
    usage.beat_in_background();

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
    //
    // C'est aussi elle qui apprend qu'un commit vient de naître : `.git/logs/HEAD` est la
    // seule écriture qui le dise, et ADR-0014 écarte le sondage de `git log`. Le signal part
    // vers le journal par un fil dédié — la lecture des commits lance un processus `git`, et
    // le fil de FSEvents porte toutes les autres écritures observées du dépôt.
    let relocating = Arc::clone(&ptys);
    let recording = features::journal::commands::record_commits(&journal);
    let git_watch = features::git::commands::watch_metadata(
        app.handle().clone(),
        move || {
            relocating.invalidate_locations();
        },
        move |worktree_root: &Path, common_dir: &Path| {
            recording(worktree_root, common_dir);
        },
        // Et c'est aussi elle qui apprend qu'un rebase vient de s'arrêter — ou de reprendre
        // — dans le worktree qu'on regarde. Sans ce fil, `⌘⌃M` resterait éteint jusqu'à ce
        // qu'on change d'onglet et qu'on revienne, alors que le rebase démarre le plus
        // souvent dans le terminal qui est sous les yeux (`menu::MergeReach`).
        {
            let menu_handle = app.handle().clone();
            move |worktree_root: &Path| menu::worktree_changed(&menu_handle, worktree_root)
        },
    );
    {
        use tauri::Manager;
        app.manage(Arc::clone(&git_watch));
    }

    // L'onglet de merge naît dans le même créneau, et pour la même raison : il lit
    // l'opération arrêtée **par** la surveillance, qui n'existait pas avant `build`.
    //
    // Il ne détient rien qu'un identifiant et une racine de worktree : tout ce qu'il montre
    // est relu dans le worktree et dans l'index à chaque appel. C'est ce qui fait que le
    // fermer ne perd rien (spec §7.4).
    {
        use tauri::Manager;
        app.manage(Arc::new(MergeSurface::new(
            Arc::new(WatchedConflicts(Arc::clone(&git_watch))),
            Arc::new(WorktreeFiles),
            Arc::new(MergeGit(Arc::clone(&git) as Arc<dyn TreeWriter>)),
        )));
    }

    // Le tableau des worktrees (spec §7.3), assemblé **après** la surveillance parce qu'il
    // lit ce qu'elle sait : c'est elle qui rend l'état d'un worktree sans relancer un
    // `git status` pour une racine déjà observée.
    //
    // Ses trois autres sources sont les trois features qui ne se connaissent pas : le
    // système de fichiers pour énumérer les worktrees d'un dépôt — **aucun verbe git de plus**
    // n'est lancé pour ça —, les onglets pour `agents now`, et le journal pour
    // `last worked by`.
    {
        use tauri::Manager;
        app.manage(WorktreeTable::new(
            Arc::new(SystemFileSystem),
            Arc::clone(&git_watch) as Arc<dyn features::git::WorktreeFacts>,
            Arc::new(InhabitedWorktrees(Arc::clone(&ptys))),
            Arc::new(JournalledWork(Arc::clone(&journal))),
            Arc::clone(&clock) as Arc<dyn shared::time::Clock>,
        ));
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

    // Quitter Ash quand un agent est reconnu demande confirmation (issue #177). Assemblé
    // ici, dans le même créneau que le socket et la surveillance git, et pour la même
    // raison : il faut le handle de l'application pour émettre, et nous sommes sur le fil
    // principal, avant `app.run`.
    //
    // La feature ne connaît ni registre ni fenêtre : elle reçoit un port qui lui rend les
    // onglets, et une fonction qui pose la question à l'écran.
    let gate = Arc::new(features::quit::QuitGate::default());
    {
        use tauri::Manager;
        app.manage(Arc::clone(&gate));
    }

    /// Les onglets, relus au moment du geste — jamais un souvenir : un agent apparu depuis
    /// le dernier rendu doit figurer dans la question.
    struct LiveTabs(Arc<features::pty::PtyRegistry>);

    impl features::quit::ObservedTabs for LiveTabs {
        fn tabs(&self) -> Vec<features::pty::TabInfo> {
            // **Un registre qui ne répond pas laisse partir.** C'est la seule règle tenable :
            // un terminal qu'on ne peut plus quitter parce qu'un verrou est empoisonné est
            // un piège bien pire qu'une question ratée, et l'utilisateur n'aurait aucun
            // recours — le geste qu'on lui refuse est précisément celui de s'en aller.
            self.0.tabs().unwrap_or_default()
        }
    }

    let asking = app.handle().clone();
    let quitting = Arc::new(features::quit::QuitQuestion::new(
        Arc::new(LiveTabs(Arc::clone(&ptys))),
        Arc::clone(&gate),
        Box::new(move |running| {
            use tauri::Emitter;
            let _ = asking.emit(features::quit::commands::CONFIRM_QUIT_EVENT, running);
        }),
    ));

    // `⌘Q`, `Ash ▸ Quitter` et le menu du Dock sont **le même** chemin — `terminate:` —, et
    // aucun `RunEvent` de Tauri ne le voit passer. Voir `features/quit/macos.rs` : c'est la
    // seule raison d'être de son `unsafe`.
    if !features::quit::intercept_terminate({
        let quitting = Arc::clone(&quitting);
        move || quitting.may_leave()
    }) {
        eprintln!("ash: ⌘Q ne demandera rien : la méthode de terminaison n'a pas pu être posée");
    }

    app.run(move |_app, event| match event {
        // La quatrième demande de sortie : fermer la dernière fenêtre. Elle ne passe pas par
        // `terminate:`, donc pas par le délégué — et elle est interceptée **ici**, sur la
        // fermeture, plutôt que sur l'`ExitRequested` que Tauri émet ensuite : à ce
        // moment-là la fenêtre est déjà détruite, et « annuler » laisserait une application
        // sans rien à l'écran. Annuler doit tout laisser intact.
        tauri::RunEvent::WindowEvent {
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            if !quitting.may_leave() {
                api.prevent_close();
            }
        }
        // Ce qui arrive après `quit_now` — donc avec le laissez-passer ouvert, qui se consomme
        // ici. C'est aussi ce que `AppHandle::exit` déclencherait de partout ailleurs, et la
        // question y est reposée telle quelle : aucun appel à `exit` n'échappe à la règle.
        //
        // **`code: None` est exclu, et c'est la seule subtilité du branchement.** Tauri émet
        // cette forme-là dans un seul cas — la dernière fenêtre vient d'être *détruite* —, et
        // la question a déjà été posée un instant plus tôt, sur `CloseRequested`. La reposer
        // ici arriverait après coup : il n'y aurait plus de fenêtre pour montrer la modale, et
        // empêcher la sortie laisserait une application qu'on ne peut ni voir ni quitter.
        tauri::RunEvent::ExitRequested {
            api, code: Some(_), ..
        } => {
            if !quitting.may_leave() {
                api.prevent_exit();
            }
        }
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
            // Le **front** de la condition 2 d'ADR-0016 : le sondage des quotas s'éteint en
            // quittant le premier plan, et un appel est redemandé en y revenant. Rien de
            // réseau ne part de ce fil-ci — `on_window_focus` ne fait que réveiller celui de
            // fond, qui rouvre le portillon. Voir `features/usage/poller.rs`.
            usage.on_window_focus(focused);

            if focused {
                let refreshing = Arc::clone(&git_watch);
                std::thread::spawn(move || refreshing.on_focus());
            }
        }
        tauri::RunEvent::Exit => {
            stop.ask();
            git_watch.stop();
            usage.stop();
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
