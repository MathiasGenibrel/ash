//! La surface de la feature vers le frontend : neuf commandes, un event.
//!
//! Le frontend ne connaît de `git` que ces dix noms et les types qui traversent. Il
//! **rend** l'état ; c'est ici qu'on le lui pousse
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! Huit d'entre elles **lisent**, et rien d'autre — pas même le graphe, qui dessine
//! pourtant toute l'histoire de l'arbre. La neuvième, [`git_branch_action`], est la seule
//! qui **touche l'arbre**, et elle ne part jamais sans un geste explicite qui a nommé ses
//! deux côtés : c'est cette différence de consentement qui décide de ce que `git_cli`
//! neutralise, et de ce qu'il assume de laisser passer.

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::branch_actions::{ActionOffer, ActionOutcome, BranchAction};
use super::branches::{overview, BranchOverview};
use super::git_cli::{BranchReader, SystemGit, TreeWriter};
use super::history::{CommitGraph, CommitGraphReader, DEFAULT_WINDOW};
use super::metadata::WorktreeMetadata;
use super::metadata_watch::{Listeners, MetadataWatch};
use super::prompt::compose_conflict_prompt;
use super::stopped::StoppedOperation;
use super::system_fs::SystemFileSystem;
use super::table::{WorktreeRemoval, WorktreeRow, WorktreeTable};
use super::throttle::MIN_INTERVAL;
use super::watcher::SystemWatcher;
use super::working_agents::WorkingAgents;
use super::worktree::resolve_worktree;
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

/// L'opération arrêtée d'un worktree, quand il y en a une (spec §7.4).
///
/// Ce que le panneau des conflits affiche : l'opération, les chemins, le pas, le commit
/// d'arrêt, `ORIG_HEAD`, et les deux sorties à **montrer** — `abort` et `skip`. Ash n'en
/// exécute aucune, et n'écrit rien : c'est de la lecture de bout en bout.
///
/// `None` est le cas courant — rien n'est en cours.
///
/// **`async` pour la même raison que [`git_metadata`]** : la réponse peut coûter une
/// résolution de worktree et un `git status`, qui n'ont rien à faire sur le fil qui dessine
/// la fenêtre.
#[tauri::command]
pub async fn git_stopped_operation<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
) -> Option<StoppedOperation> {
    let watch = app.state::<Arc<MetadataWatch>>();
    watch.stopped(Path::new(&worktree_root))
}

/// Le prompt à rédiger dans l'onglet de l'agent, pour ce rebase arrêté.
///
/// Composé **ici**, dans le backend, et non côté écran : c'est le backend qui détient
/// l'état ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et la règle
/// de rédaction est une règle du produit, pas une mise en forme. L'onglet de merge (#30)
/// appellera le même compositeur sur les seuls conflits qu'il n'a pas résolus.
///
/// Rendre le prompt n'écrit rien nulle part : c'est `pty_compose` qui le pose dans le
/// terminal, et l'utilisateur seul qui l'envoie
/// ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
#[tauri::command]
pub async fn git_conflict_prompt<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
) -> Option<String> {
    let watch = app.state::<Arc<MetadataWatch>>();
    let stopped = watch.stopped(Path::new(&worktree_root))?;
    Some(compose_conflict_prompt(&stopped.prompt_subject()))
}

/// Le graphe de commits d'un worktree, colonne `by` comprise (spec §7.2).
///
/// `window` est le nombre de lignes demandées **depuis le sommet** : voir `graph.rs` pour
/// pourquoi le graphe grandit par une fenêtre et non par des pages. Le backend la borne — une
/// webview ne décide pas de faire lire dix ans d'histoire d'un coup.
///
/// Elle est **facultative**, et son absence est le cas normal : un graphe qui s'ouvre ne
/// demande pas une taille, il demande *le graphe*, et la première fenêtre est
/// [`DEFAULT_WINDOW`] — un choix de produit, du côté qui lance le processus. L'écran ne nomme
/// un nombre qu'en **élargissant**, à partir de la fenêtre que la réponse précédente lui a
/// annoncée. Sans ça, la valeur de départ vivrait des deux côtés de la frontière, et c'est
/// celle du TypeScript qui gagnerait — l'écran demande, le backend annonce.
///
/// `None` pour un répertoire hors de tout dépôt, ou dont les fichiers de contrôle ne se
/// lisent pas : c'est le même cas nominal que `git_metadata`, et il se rend pareil — le
/// panneau dit qu'il n'a rien à montrer.
///
/// **`async` pour la même raison que [`git_metadata`]** : la réponse lance un processus
/// `git`, qui n'a rien à faire sur le fil qui dessine la fenêtre.
#[tauri::command]
pub async fn git_commit_graph<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
    window: Option<usize>,
) -> Option<CommitGraph> {
    let reader = app.state::<Arc<CommitGraphReader>>();
    // Le dépôt **commun** : c'est la clé du journal, et deux worktrees d'un même projet
    // partagent donc leur attribution comme ils partagent leurs commits (ADR-0012).
    let located = resolve_worktree(&SystemFileSystem, Path::new(&worktree_root)).ok()?;
    let (_, common_dir) = located.git_dirs()?;
    Some(reader.window(
        &located.worktree.root,
        &common_dir.to_string_lossy(),
        window.unwrap_or(DEFAULT_WINDOW),
    ))
}

/// Le tableau des worktrees (spec §7.3).
///
/// Ce que la vue `worktrees` du panneau bas affiche, **composé ici** : les deux colonnes du
/// milieu — `agents now` et `last worked by` — croisent les onglets, le journal
/// d'attribution et l'état git, et aucune fenêtre n'a le droit de les assembler elle-même
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
///
/// **`async` pour la même raison que [`git_metadata`]** : la réponse peut coûter un
/// `git status` par worktree du dépôt, ce qui n'a rien à faire sur le fil qui dessine la
/// fenêtre.
///
/// Demandée par la fenêtre plutôt que poussée : le tableau est fermé la plupart du temps, et
/// un event qui repartirait à chaque écriture de `.git` ferait travailler une vue que
/// personne ne regarde. Ce qui bouge sous les yeux de l'utilisateur — la branche, l'état de
/// l'arbre — arrive déjà par `ash://git-metadata`.
#[tauri::command]
pub async fn git_worktrees<R: Runtime>(app: AppHandle<R>) -> Vec<WorktreeRow> {
    let table = app.state::<Arc<WorktreeTable>>();
    table.rows()
}

/// Ce qu'une suppression de ce worktree emporterait (spec §5.4).
///
/// **Elle ne supprime rien.** Ash signale, il ne supprime jamais : la fiche énonce ce qui
/// partirait — fichiers non validés, agent en cours, opération interrompue — et rend la
/// commande **comme du texte à montrer**, exactement comme les sorties de secours d'un
/// rebase arrêté ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
///
/// Elle est lue **au moment du geste**, et non au moment où le tableau s'est dessiné : ce
/// qu'elle énonce doit être vrai quand on le lit.
#[tauri::command]
pub async fn git_worktree_removal<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
) -> Option<WorktreeRemoval> {
    let table = app.state::<Arc<WorktreeTable>>();
    table.removal(Path::new(&worktree_root))
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

/// Les branches d'un worktree, groupées, situées, et avec les agents qu'elles menacent.
///
/// Une seule réponse pour les quatre choses que la popup montre — la liste, les groupes, le
/// worktree qui détient chaque branche, et les agents en danger. **Une seule et pas quatre**,
/// parce qu'elles doivent être vraies au même instant : lues séparément, la popup pourrait
/// nommer un agent qui vient de finir, ou proposer un checkout sur une branche qu'un autre
/// worktree vient de prendre.
///
/// **`async` volontairement**, comme [`git_metadata`] et pour la même raison : deux
/// invocations de `git` n'ont rien à faire sur le fil qui dessine la fenêtre.
///
/// `None` quand `git` n'a pas répondu — absent du `PATH`, dépôt illisible, délai dépassé.
/// L'écran montre alors qu'il n'a pas su lire, il n'invente pas une liste vide.
#[tauri::command]
pub async fn git_branches<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
) -> Option<BranchOverview> {
    let root = std::path::PathBuf::from(&worktree_root);
    let reader = app.state::<Arc<dyn BranchReader>>();
    let agents = app.state::<Arc<dyn WorkingAgents>>();

    let refs = reader.refs(&root)?;
    let worktrees = reader.worktrees(&root)?;
    Some(overview(
        &root,
        &refs,
        &worktrees,
        agents.in_worktree(&root),
    ))
}

/// Ce que `⌘⏎` propose pour une branche — les trois verbes, refus compris.
///
/// Un appel séparé de [`git_branches`], et sur un geste explicite : les offres dépendent de
/// l'état du dépôt *à cet instant*, et les recalculer à l'ouverture de la popup les rendrait
/// périmées dès qu'un autre worktree prend une branche. Elles sont donc relues au moment où
/// on les montre — le même instant que celui où l'utilisateur les lira.
///
/// Vide quand la branche n'existe plus : la popup montre alors qu'il n'y a rien à faire,
/// elle n'invente pas une action sur un nom.
#[tauri::command]
pub async fn git_branch_offers<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
    branch: String,
) -> Option<Vec<ActionOffer>> {
    let root = std::path::PathBuf::from(&worktree_root);
    let reader = app.state::<Arc<dyn BranchReader>>();
    let agents = app.state::<Arc<dyn WorkingAgents>>();

    let refs = reader.refs(&root)?;
    let worktrees = reader.worktrees(&root)?;
    let shown = overview(&root, &refs, &worktrees, agents.in_worktree(&root));

    Some(
        shown
            .sections
            .iter()
            .flat_map(|section| &section.branches)
            .find(|candidate| candidate.name == branch)
            .map(|found| super::branch_actions::offers(&shown, found))
            .unwrap_or_default(),
    )
}

/// Lance une action de branche — `⌘⏎` (spec §7.1).
///
/// Elle **relit la liste avant d'agir**, et c'est ce qui la rend sûre : le nom reçu sert à
/// retrouver une branche dans ce que le dépôt contient *maintenant*, et c'est cette
/// branche-là qui part vers le processus. Une branche effacée, prise par un autre worktree, ou
/// dont le nom ressemble à une option, est refusée — jamais devinée (voir
/// [`super::branch_actions`]).
///
/// Rien ici ne met un agent en pause, et rien ne demande de confirmation : ces deux gestes
/// appartiennent à l'utilisateur, et la commande n'est appelée qu'après. Ash ne valide rien à
/// sa place ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
#[tauri::command]
pub async fn git_branch_action<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
    action: BranchAction,
    branch: String,
) -> Option<ActionOutcome> {
    let root = std::path::PathBuf::from(&worktree_root);
    let reader = app.state::<Arc<dyn BranchReader>>();
    let agents = app.state::<Arc<dyn WorkingAgents>>();
    let writer = app.state::<Arc<dyn TreeWriter>>();

    let refs = reader.refs(&root)?;
    let worktrees = reader.worktrees(&root)?;
    let shown = overview(&root, &refs, &worktrees, agents.in_worktree(&root));

    Some(super::branch_actions::run(&writer, &shown, action, &branch))
}
