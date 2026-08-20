//! Le tableau des worktrees (spec §7.3) — et les deux colonnes que `git worktree list`
//! ne donne pas.
//!
//! Un client git sait dire où sont les worktrees d'un dépôt, sur quelle branche ils sont,
//! et si leur arbre est sale. Aucun ne sait dire **qui travaille dedans en ce moment**, ni
//! **qui y a travaillé en dernier** : ces deux-là demandent de connaître le `cwd` de chaque
//! onglet et l'outil qui tient son avant-plan, ce qu'Ash est seul à savoir
//! ([ADR-0011](../../../../docs/adr/0011-git-domaine-de-premier-plan.md)). C'est tout
//! l'intérêt de cet écran, et c'est pour ça que le tableau est composé **ici**, dans le
//! backend, et non assemblé par la fenêtre à partir de trois listes
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! # Ce que cette feature ne connaît pas, et demande
//!
//! Elle ne connaît ni les onglets, ni le journal d'attribution. Elle possède donc deux
//! ports — [`TabPresence`] et [`WorkHistory`] —, et c'est le composition root qui les
//! branche sur `features::pty` et `features::journal`. Le consommateur possède le port,
//! comme `pty` possède `AgentStates` sans rien savoir d'`agents`.
//!
//! # Ce que le tableau n'affirme pas
//!
//! - **`last worked by` ne connaît que ce qu'Ash a observé.** Un agent présent maintenant,
//!   ou un commit que le journal a vu naître ici
//!   ([ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md)). Un agent
//!   qui a travaillé une nuit entière sans rien valider et dont l'onglet est fermé n'y est
//!   pas : la colonne est alors vide, et vide veut dire « Ash ne sait pas », jamais
//!   « personne ».
//! - **`stale` ne se prononce que sur une observation.** Sans la moindre trace d'un agent
//!   dans ce worktree, la règle des trois jours (spec §5.4) n'a pas d'origine à soustraire,
//!   et le worktree n'est pas signalé. C'est le prix de ne rien inventer — et le coût d'une
//!   erreur ici serait de désigner comme abandonné un worktree qui ne l'est pas.
//! - **Rien n'est supprimé.** [`WorktreeRemoval`] *énonce* ce qu'une suppression emporterait et
//!   rend la commande **comme du texte à montrer**, exactement comme les `escapes` d'un
//!   rebase arrêté ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
//!   Ash signale ; il ne détruit pas le travail de quelqu'un d'autre.
//!
//! # Aucun verbe git de plus
//!
//! L'énumération des worktrees d'un dépôt se lit **dans les fichiers de contrôle** —
//! `.git/worktrees/<nom>/gitdir` —, derrière le port [`FileSystem`] que la feature possède
//! déjà. `git worktree list --porcelain` aurait dit la même chose en lançant un processus,
//! et la question de sécurité de [`super::git_cli`] se serait posée pour lui. Elle ne se
//! pose pas : rien n'est exécuté ici.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::features::agents::AgentState;
use crate::shared::time::{Clock, UnixMillis};

use super::metadata::{OperationKind, WorktreeMetadata};
use super::ports::FileSystem;
use super::worktree::{resolve_worktree, WorktreeLocation};

/// Sans agent depuis plus de **trois jours**, un worktree qui porte des modifications est
/// signalé (spec §5.4).
///
/// La durée est ici, en une seule constante, parce que c'est une règle de produit et non un
/// détail de calcul : elle se relit, et le test qui la tient la nomme.
pub const STALE_AFTER: Duration = Duration::from_secs(3 * 24 * 60 * 60);

/// Le dépôt sous lequel une ligne se range.
///
/// L'`id` est le dossier git commun — **la même clé que celle de la sidebar** (`RepoRef`),
/// et c'est ce qui permettra à la fiche de branche (#31) de parler du même dépôt que la
/// colonne de gauche.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RepoLine {
    pub id: String,
    pub name: String,
}

/// Un agent qui tourne **en ce moment** dans un worktree.
///
/// Il vient des onglets, et de rien d'autre : c'est la sonde d'ADR-0005 qui a vu l'outil
/// prendre l'avant-plan, et la reconnaissance d'ADR-0006 qui lui a donné son nom. Un onglet
/// où tourne un `vim` n'est pas un agent, et n'apparaît pas ici.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct WorktreeAgent {
    /// L'onglet où il tourne — de quoi y aller d'un clic, et rien de plus (ADR-0010).
    pub tab_id: String,
    /// Le nom de l'outil — `claude`, `codex`.
    pub command: String,
    pub state: AgentState,
    /// Quand il est **entré** dans cet état. Une date absolue, comme `TabInfo.stateSince` :
    /// la durée qui s'incrémente est un fait d'affichage.
    ///
    /// **`number` et non `bigint`**, pour la raison écrite sur `TabInfo::state_since` :
    /// `serde_json` écrit un nombre JSON, et un `bigint` déclaré ici mentirait sur ce qui
    /// arrive vraiment.
    #[cfg_attr(test, ts(type = "number"))]
    pub since: UnixMillis,
}

/// D'où vient ce que la colonne `last worked by` affirme.
///
/// Le mot est dans le contrat parce que les deux sources ne promettent pas la même chose,
/// et que l'écran doit pouvoir le dire : un onglet ouvert est une observation d'**à
/// l'instant**, un commit journalisé est une observation qui a survécu à la fermeture de son
/// onglet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum WorkSource {
    /// Un onglet, maintenant : l'agent est là, ou vient d'y être.
    Tab,
    /// Un commit que le journal d'attribution a vu naître ici (ADR-0014).
    Commit,
}

/// Qui a travaillé ici en dernier, et quand.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LastWork {
    pub agent: String,
    /// **`number` et non `bigint`** — voir [`WorktreeAgent::since`].
    #[cfg_attr(test, ts(type = "number"))]
    pub at: UnixMillis,
    pub source: WorkSource,
}

/// Une ligne du tableau (spec §7.3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRow {
    /// La racine du worktree — **la même clé** que celle des onglets et de l'event
    /// `ash://git-metadata`.
    pub worktree_root: String,
    pub worktree_name: String,
    pub repo: Option<RepoLine>,
    /// La branche, l'opération en cours et l'état de l'arbre — le contrat que la ligne de
    /// statut et la sidebar rendent déjà. `None` quand rien ne s'est laissé lire.
    pub metadata: Option<WorktreeMetadata>,
    pub agents_now: Vec<WorktreeAgent>,
    /// `done · waiting for your review` — l'état que la spec §7.3 nomme le plus utile du
    /// tableau. Voir [`awaiting_review`].
    pub awaiting_review: bool,
    pub last_worked_by: Option<LastWork>,
    pub stale: bool,
    /// Le worktree **principal** du dépôt : celui que `git worktree remove` refuse.
    pub main: bool,
}

/// Ce qu'une suppression emporterait — énoncé **avant** qu'elle n'ait lieu (spec §5.4).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemoval {
    pub worktree_root: String,
    pub worktree_name: String,
    /// Ce qui partirait avec, en toutes lettres. Vide quand il n'y a rien à emporter.
    pub carries: Vec<String>,
    /// Pourquoi git refusera de toute façon — le worktree principal d'un dépôt.
    pub refused: Option<String>,
    /// La commande à taper, **montrée et jamais lancée** (ADR-0015).
    pub command: String,
}

/// Les onglets, tels que cette feature a besoin de les connaître.
///
/// Le port appartient à `git` parce que c'est `git` qui pose la question ; son adaptateur
/// est dans le composition root, sur le registre de PTY.
pub trait TabPresence: Send + Sync {
    fn inhabiting(&self) -> Vec<InhabitingTab>;
}

/// Un onglet, réduit à ce que le tableau lit de lui.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InhabitingTab {
    pub tab_id: String,
    pub worktree_root: PathBuf,
    /// L'outil reconnu dans son avant-plan, ou rien — un shell à son invite, un `vim`.
    pub agent: Option<String>,
    pub state: AgentState,
    pub since: UnixMillis,
}

/// Ce qu'Ash a observé d'un agent dans un worktree, et qui a survécu à son onglet.
///
/// Une seule implémentation aujourd'hui — le journal d'attribution —, et elle est bornée :
/// voir [`super::table`] pour ce que la colonne n'affirme pas.
pub trait WorkHistory: Send + Sync {
    fn last_worked(&self, repo: &Path, worktree_root: &Path) -> Option<Worked>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worked {
    pub agent: String,
    pub at: UnixMillis,
}

/// Ce qu'un worktree dit de lui-même — la surveillance, vue par le tableau.
///
/// Un trait plutôt qu'un appel direct à [`super::MetadataWatch`] : la composition du tableau
/// est une règle, et une règle se prouve sans lancer `git status` ni monter un observateur
/// de fichiers.
pub trait WorktreeFacts: Send + Sync {
    fn metadata(&self, worktree_root: &Path) -> Option<WorktreeMetadata>;
}

/// Le tableau des worktrees, avec ses quatre sources.
pub struct WorktreeTable {
    fs: Arc<dyn FileSystem>,
    facts: Arc<dyn WorktreeFacts>,
    tabs: Arc<dyn TabPresence>,
    history: Arc<dyn WorkHistory>,
    clock: Arc<dyn Clock>,
}

impl WorktreeTable {
    pub fn new(
        fs: Arc<dyn FileSystem>,
        facts: Arc<dyn WorktreeFacts>,
        tabs: Arc<dyn TabPresence>,
        history: Arc<dyn WorkHistory>,
        clock: Arc<dyn Clock>,
    ) -> Arc<Self> {
        Arc::new(Self {
            fs,
            facts,
            tabs,
            history,
            clock,
        })
    }

    /// Le tableau entier, rangé par dépôt puis par nom de worktree.
    ///
    /// Ce qu'il montre : **tous** les worktrees des dépôts qu'un onglet habite — c'est la
    /// troisième porte de la spec §5.2, « `git worktree list` le déclare pour un dépôt déjà
    /// affiché », lue dans les fichiers de contrôle plutôt qu'en lançant git. Un worktree
    /// sans onglet y figure donc, et c'est précisément celui dont on veut savoir s'il est
    /// `stale`.
    pub fn rows(&self) -> Vec<WorktreeRow> {
        let tabs = self.tabs.inhabiting();
        let now = self.clock.wall();

        let mut rows: Vec<WorktreeRow> = self
            .shown(&tabs)
            .into_iter()
            .filter_map(|located| self.row(located, &tabs, now))
            .collect();
        // Rangées comme la colonne de gauche les range : par dépôt, puis par nom. Un ordre
        // stable est ce qui permet de relire le tableau après un rafraîchissement sans
        // chercher où la ligne est partie.
        rows.sort_by(|left, right| {
            let repo = |row: &WorktreeRow| {
                row.repo
                    .as_ref()
                    .map(|repo| repo.name.clone())
                    .unwrap_or_default()
            };
            repo(left)
                .cmp(&repo(right))
                .then_with(|| left.worktree_name.cmp(&right.worktree_name))
                .then_with(|| left.worktree_root.cmp(&right.worktree_root))
        });
        rows
    }

    /// Ce qu'une suppression de ce worktree emporterait (spec §5.4).
    ///
    /// **Rien n'est supprimé, et rien n'est exécuté** : la fiche est lue au moment du geste,
    /// pas au moment où le tableau s'est dessiné — ce qu'elle énonce doit être vrai quand on
    /// la lit, pas quand on l'a demandée.
    pub fn removal(&self, worktree_root: &Path) -> Option<WorktreeRemoval> {
        let row = self.row(
            self.locate(worktree_root)?,
            &self.tabs.inhabiting(),
            self.clock.wall(),
        )?;
        Some(plan(&row))
    }

    /// Les worktrees à montrer : ceux qu'un onglet habite, et leurs frères.
    fn shown(&self, tabs: &[InhabitingTab]) -> Vec<WorktreeLocation> {
        let inhabited: BTreeSet<PathBuf> =
            tabs.iter().map(|tab| tab.worktree_root.clone()).collect();

        let mut roots: BTreeSet<PathBuf> = BTreeSet::new();
        for root in inhabited {
            let Some(located) = self.locate(&root) else {
                continue;
            };
            roots.insert(located.worktree.root.clone());
            if let Some(repo) = located.repo.as_ref() {
                roots.extend(siblings(self.fs.as_ref(), &repo.git_dir));
            }
        }

        roots.iter().filter_map(|root| self.locate(root)).collect()
    }

    fn locate(&self, root: &Path) -> Option<WorktreeLocation> {
        resolve_worktree(self.fs.as_ref(), root).ok()
    }

    fn row(
        &self,
        located: WorktreeLocation,
        tabs: &[InhabitingTab],
        now: UnixMillis,
    ) -> Option<WorktreeRow> {
        let root = located.worktree.root.clone();
        let here: Vec<&InhabitingTab> = tabs
            .iter()
            .filter(|tab| tab.worktree_root == root)
            .collect();

        let agents_now: Vec<WorktreeAgent> = here
            .iter()
            .filter_map(|tab| {
                Some(WorktreeAgent {
                    tab_id: tab.tab_id.clone(),
                    command: tab.agent.clone()?,
                    state: tab.state,
                    since: tab.since,
                })
            })
            .collect();

        let metadata = self.facts.metadata(&root);
        let journalled = located
            .repo
            .as_ref()
            .map(|repo| repo.git_dir.clone())
            // Un dépôt sans worktree lié n'a pas de groupe, et le journal le range pourtant
            // sous son dossier git commun : ici, c'est celui du worktree lui-même.
            .or_else(|| located.worktree.git_dir.clone())
            .and_then(|repo| self.history.last_worked(&repo, &root));

        let last_worked_by = last_worked_by(&agents_now, journalled);

        Some(WorktreeRow {
            worktree_root: root.display().to_string(),
            worktree_name: located.worktree.name.clone(),
            repo: located.repo.as_ref().map(|repo| RepoLine {
                id: repo.git_dir.display().to_string(),
                name: repo.name.clone(),
            }),
            awaiting_review: awaiting_review(&agents_now),
            stale: stale(&agents_now, metadata.as_ref(), last_worked_by.as_ref(), now),
            main: is_main(&located),
            agents_now,
            last_worked_by,
            metadata,
        })
    }
}

/// `done · waiting for your review` : un agent a fini, et personne n'a regardé (spec §7.3).
///
/// **Il n'y a pas de seconde notion de « vu » dans ce fichier**, et c'est délibéré : `done`
/// ne survit à sa lecture que trente secondes, et ces trente secondes ne partent qu'au
/// moment où la fenêtre Ash prend le focus (spec §6.4, `agents/machine.rs`). Un onglet qui
/// est encore `done` est donc, par construction, un onglet que personne n'a regardé — la
/// question est déjà tranchée en amont, et la reposer ici avec une autre mémoire ferait deux
/// réponses possibles à « a-t-on vu ? ».
///
/// `error` n'en fait pas partie : il a son propre traitement dans la sidebar — rail rouge,
/// nom barré — et une bannière l'a déjà annoncé. Ce que cette colonne nomme est le cas
/// silencieux, celui où **rien** ne réclame l'attention alors que quelque chose l'attend.
fn awaiting_review(agents: &[WorktreeAgent]) -> bool {
    agents.iter().any(|agent| agent.state == AgentState::Done)
}

/// La plus récente des deux observations, et rien d'autre.
///
/// L'onglet l'emporte quand il est plus récent, le journal quand il l'est : ce sont deux
/// observations de même nature — Ash a vu — et les départager par leur date est la seule
/// règle qui ne préfère pas une source à la vérité.
fn last_worked_by(agents: &[WorktreeAgent], journalled: Option<Worked>) -> Option<LastWork> {
    let from_tabs = agents
        .iter()
        .max_by_key(|agent| agent.since)
        .map(|agent| LastWork {
            agent: agent.command.clone(),
            at: agent.since,
            source: WorkSource::Tab,
        });
    let from_journal = journalled.map(|worked| LastWork {
        agent: worked.agent,
        at: worked.at,
        source: WorkSource::Commit,
    });

    match (from_tabs, from_journal) {
        (Some(tab), Some(commit)) => Some(if commit.at > tab.at { commit } else { tab }),
        (tab, commit) => tab.or(commit),
    }
}

/// `stale` : sans agent depuis plus de trois jours, **et** des fichiers modifiés (spec §5.4).
///
/// Les trois conditions sont conjointes, et chacune est une garde :
///
/// - **aucun agent maintenant**, sans quoi le mot serait faux à la seconde où on le lit ;
/// - **un arbre sale**, connu : `git status` muet (`None`) veut dire « on ne sait pas », et
///   on ne signale pas sur une ignorance ;
/// - **une observation datée**, plus vieille que trois jours. Sans elle, il n'y a rien à
///   soustraire : un worktree qu'Ash n'a jamais vu habité n'est pas pour autant abandonné.
///
/// La soustraction porte sur deux dates **murales**, ce que `shared::time` réserve d'ordinaire
/// à la description d'un instant. C'est assumé ici, et sans autre choix possible : le fait
/// daté — un commit — l'est en heure murale, et aucune horloge monotone ne survit à un
/// redémarrage. Une machine recalée entre-temps ferait au pire apparaître ou disparaître un
/// mot ; elle ne peut rien détruire, puisque Ash ne supprime jamais.
fn stale(
    agents: &[WorktreeAgent],
    metadata: Option<&WorktreeMetadata>,
    last: Option<&LastWork>,
    now: UnixMillis,
) -> bool {
    if !agents.is_empty() {
        return false;
    }
    let dirty = metadata
        .and_then(|metadata| metadata.status.as_ref())
        .is_some_and(|status| !status.tree.is_clean());
    let Some(last) = last else {
        return false;
    };

    dirty && now.saturating_sub(last.at) > millis(STALE_AFTER)
}

/// Le worktree principal d'un dépôt : son dossier git **est** le dossier commun.
///
/// C'est celui que `git worktree remove` refuse, et le dire dans la fiche de suppression
/// vaut mieux que laisser l'utilisateur découvrir le refus en tapant la commande.
fn is_main(located: &WorktreeLocation) -> bool {
    match (
        located.worktree.git_dir.as_ref(),
        located.repo.as_ref().map(|repo| &repo.git_dir),
    ) {
        (Some(git_dir), Some(common)) => git_dir == common,
        // Hors de tout dépôt, ou dépôt sans worktree lié : il n'y a rien à supprimer.
        _ => true,
    }
}

/// Ce que la suppression emporterait, mis en mots.
///
/// La phrase est composée **ici** et non dans l'écran, pour la raison qui compose déjà la
/// fiche du journal dans son `commands.rs` : c'est le backend qui détient l'état, et une
/// phrase qui décide de ce qu'on va perdre ne doit pas pouvoir diverger d'un écran à
/// l'autre.
fn plan(row: &WorktreeRow) -> WorktreeRemoval {
    let mut carries = Vec::new();

    if let Some(status) = row.metadata.as_ref().and_then(|meta| meta.status.as_ref()) {
        let files = u64::from(status.tree.added)
            + u64::from(status.tree.modified)
            + u64::from(status.tree.deleted)
            + u64::from(status.tree.conflicted);
        if files > 0 {
            carries.push(format!(
                "{files} uncommitted {} — nothing here is in the repository yet",
                if files == 1 { "file" } else { "files" }
            ));
        }
        if let Some(upstream) = status.upstream.as_ref() {
            if upstream.ahead > 0 {
                carries.push(format!(
                    "{} commit{} that no remote has",
                    upstream.ahead,
                    if upstream.ahead == 1 { "" } else { "s" }
                ));
            }
        }
    } else {
        // `git status` n'a pas répondu : dire « rien à emporter » serait affirmer ce qu'on
        // ne sait pas, juste avant un geste qui détruit.
        carries.push("git did not answer: what this worktree holds is unknown".to_owned());
    }

    for agent in &row.agents_now {
        carries.push(format!(
            "{} is running here right now, in a tab that would lose its directory",
            agent.command
        ));
    }

    if let Some(operation) = row
        .metadata
        .as_ref()
        .and_then(|meta| meta.operation.as_ref())
    {
        let kind = match operation.kind {
            OperationKind::Rebase => "rebase",
            OperationKind::Am => "patch series",
            OperationKind::Merge => "merge",
        };
        carries.push(format!("a {kind} is in progress and would be abandoned"));
    }

    WorktreeRemoval {
        worktree_root: row.worktree_root.clone(),
        worktree_name: row.worktree_name.clone(),
        carries,
        refused: row.main.then(|| {
            "this is the repository's main worktree — git refuses to remove it".to_owned()
        }),
        // Montrée, jamais lancée : c'est la même conduite que les `escapes` d'un rebase
        // arrêté (ADR-0015). `--force` n'y est pas, et n'y sera pas : la commande qu'Ash
        // écrit est celle qui refuse de détruire du travail non validé.
        command: format!("git worktree remove {}", row.worktree_root),
    }
}

/// Les frères d'un worktree, lus dans les fichiers de contrôle du dépôt commun.
///
/// Deux familles, et il faut les deux : le worktree **principal** — le parent du `.git`
/// commun, qui n'a pas d'entrée dans `worktrees/` — et chaque worktree **lié**, dont
/// `.git/worktrees/<nom>/gitdir` nomme le `.git`.
///
/// Une entrée dont la cible n'existe plus est **écartée** : git garde le dossier jusqu'au
/// prochain `prune`, et afficher un worktree effacé du disque serait proposer d'en supprimer
/// un qui n'est plus là.
fn siblings(fs: &dyn FileSystem, common_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if common_dir.file_name() == Some(std::ffi::OsStr::new(".git")) {
        if let Some(main) = common_dir.parent().and_then(|root| fs.canonicalize(root)) {
            roots.push(main);
        }
    }

    for entry in fs.list_dir(&common_dir.join("worktrees")) {
        let Ok(line) = fs.read_to_string(&entry.join("gitdir")) else {
            continue;
        };
        // Le fichier nomme le `.git` du worktree ; sa racine est le dossier qui le porte.
        let git_file = PathBuf::from(line.trim());
        let Some(root) = git_file.parent().and_then(|root| fs.canonicalize(root)) else {
            continue;
        };
        roots.push(root);
    }

    roots
}

fn millis(duration: Duration) -> UnixMillis {
    UnixMillis::try_from(duration.as_millis()).unwrap_or(UnixMillis::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::fake_fs::FakeFs;
    use crate::features::git::metadata::{Head, Status, TreeStatus, Upstream};
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::time::Instant;

    const MAIN: &str = "/dev/ash";
    const LINKED: &str = "/wt/ash-sidebar";
    /// Un mardi quelconque, en millisecondes : toutes les dates des tests s'y rapportent.
    const NOW: UnixMillis = 1_755_000_000_000;
    const DAY: UnixMillis = 24 * 60 * 60 * 1_000;

    /// Test Data Builder : un dépôt à deux worktrees, un onglet dans chacun si on veut.
    struct TableBuilder {
        fs: FakeFs,
        facts: BTreeMap<PathBuf, WorktreeMetadata>,
        tabs: Vec<InhabitingTab>,
        history: BTreeMap<PathBuf, Worked>,
        now: UnixMillis,
    }

    impl TableBuilder {
        fn new() -> Self {
            let fs = FakeFs::new()
                .repo_hosting(MAIN, &["sidebar"])
                .worktree_gitdir(MAIN, "sidebar", LINKED)
                .linked_worktree(LINKED, "gitdir: /dev/ash/.git/worktrees/sidebar\n");
            Self {
                fs,
                facts: BTreeMap::new(),
                tabs: Vec::new(),
                history: BTreeMap::new(),
                now: NOW,
            }
        }

        /// Un onglet dans un worktree, avec l'outil que la sonde y a reconnu.
        fn tab(mut self, root: &str, agent: Option<&str>, state: AgentState) -> Self {
            self.tabs.push(InhabitingTab {
                tab_id: format!("01J0TAB{}", self.tabs.len()),
                worktree_root: PathBuf::from(root),
                agent: agent.map(str::to_owned),
                state,
                since: self.now,
            });
            self
        }

        fn dirty(self, root: &str) -> Self {
            self.status(
                root,
                Some(Status {
                    tree: TreeStatus {
                        added: 0,
                        modified: 3,
                        deleted: 0,
                        conflicted: 0,
                    },
                    upstream: Some(Upstream {
                        ahead: 0,
                        behind: 0,
                    }),
                    conflicts: Vec::new(),
                }),
            )
        }

        fn clean(self, root: &str) -> Self {
            self.status(
                root,
                Some(Status {
                    tree: TreeStatus {
                        added: 0,
                        modified: 0,
                        deleted: 0,
                        conflicted: 0,
                    },
                    upstream: None,
                    conflicts: Vec::new(),
                }),
            )
        }

        fn status(mut self, root: &str, status: Option<Status>) -> Self {
            self.facts.insert(
                PathBuf::from(root),
                WorktreeMetadata {
                    head: Head::Branch {
                        name: "feat/table".to_owned(),
                    },
                    operation: None,
                    status,
                },
            );
            self
        }

        /// Un commit que le journal a vu naître ici, il y a `days_ago` jours.
        fn journalled(mut self, root: &str, agent: &str, days_ago: u64) -> Self {
            self.history.insert(
                PathBuf::from(root),
                Worked {
                    agent: agent.to_owned(),
                    at: self.now - days_ago * DAY,
                },
            );
            self
        }

        fn build(self) -> Arc<WorktreeTable> {
            let doubles = Arc::new(Doubles {
                facts: self.facts,
                tabs: Mutex::new(self.tabs),
                history: self.history,
                now: self.now,
            });
            WorktreeTable::new(
                Arc::new(self.fs),
                Arc::clone(&doubles) as Arc<dyn WorktreeFacts>,
                Arc::clone(&doubles) as Arc<dyn TabPresence>,
                Arc::clone(&doubles) as Arc<dyn WorkHistory>,
                doubles as Arc<dyn Clock>,
            )
        }
    }

    /// Les trois ports et l'horloge, en mémoire.
    struct Doubles {
        facts: BTreeMap<PathBuf, WorktreeMetadata>,
        tabs: Mutex<Vec<InhabitingTab>>,
        history: BTreeMap<PathBuf, Worked>,
        now: UnixMillis,
    }

    impl WorktreeFacts for Doubles {
        fn metadata(&self, worktree_root: &Path) -> Option<WorktreeMetadata> {
            self.facts.get(worktree_root).cloned()
        }
    }

    impl TabPresence for Doubles {
        fn inhabiting(&self) -> Vec<InhabitingTab> {
            self.tabs
                .lock()
                .map(|tabs| tabs.clone())
                .unwrap_or_default()
        }
    }

    impl WorkHistory for Doubles {
        fn last_worked(&self, _repo: &Path, worktree_root: &Path) -> Option<Worked> {
            self.history.get(worktree_root).cloned()
        }
    }

    impl Clock for Doubles {
        fn now(&self) -> Instant {
            Instant::now()
        }

        fn wall(&self) -> UnixMillis {
            self.now
        }
    }

    fn row_of<'a>(rows: &'a [WorktreeRow], root: &str) -> &'a WorktreeRow {
        rows.iter()
            .find(|row| row.worktree_root == root)
            .unwrap_or_else(|| panic!("le tableau devrait porter {root} : {rows:#?}"))
    }

    #[test]
    fn given_one_tab_in_a_repository_when_the_table_is_built_then_its_sibling_worktrees_are_there_too(
    ) {
        // Given — un seul onglet, dans le worktree principal. Le worktree lié n'a personne
        // dedans, et c'est justement celui dont on veut savoir ce qu'il devient (spec §5.2).
        let table = TableBuilder::new()
            .tab(MAIN, Some("claude"), AgentState::Working)
            .build();

        // When
        let rows = table.rows();

        // Then
        let roots: Vec<&str> = rows.iter().map(|row| row.worktree_root.as_str()).collect();
        assert_eq!(roots, vec![MAIN, LINKED]);
        assert_eq!(row_of(&rows, LINKED).agents_now, Vec::new());
        assert!(row_of(&rows, MAIN).main, "le parent du `.git` commun");
        assert!(!row_of(&rows, LINKED).main);
    }

    #[test]
    fn given_agents_in_two_worktrees_when_the_table_is_built_then_each_row_names_only_its_own() {
        // Given — la colonne que `git worktree list` ne donne pas : Ash la connaît parce
        // qu'il connaît le `cwd` de chaque onglet (spec §7.3).
        let table = TableBuilder::new()
            .tab(MAIN, Some("claude"), AgentState::Working)
            .tab(LINKED, Some("codex"), AgentState::Waiting)
            // Un onglet où tourne un shell n'est pas un agent, et n'a rien à faire dans
            // `agents now` (ADR-0006 : reconnaître, ce n'est pas déclarer).
            .tab(LINKED, None, AgentState::Idle)
            .build();

        // When
        let rows = table.rows();

        // Then
        let names = |row: &WorktreeRow| {
            row.agents_now
                .iter()
                .map(|agent| agent.command.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(row_of(&rows, MAIN)), vec!["claude"]);
        assert_eq!(names(row_of(&rows, LINKED)), vec!["codex"]);
    }

    #[test]
    fn given_an_agent_that_finished_and_no_one_looked_when_the_table_is_built_then_the_row_awaits_review(
    ) {
        // Given — l'état que la spec §7.3 nomme le plus utile du tableau. Il n'a pas de
        // seconde notion de « vu » : un `done` que quelqu'un a regardé n'est plus `done`
        // trente secondes plus tard (spec §6.4).
        let table = TableBuilder::new()
            .tab(LINKED, Some("claude"), AgentState::Done)
            .tab(MAIN, Some("claude"), AgentState::Working)
            .build();

        // When
        let rows = table.rows();

        // Then
        assert!(row_of(&rows, LINKED).awaiting_review);
        assert!(!row_of(&rows, MAIN).awaiting_review);
    }

    #[test]
    fn given_a_worktree_whose_agent_is_gone_when_a_commit_was_journalled_there_then_it_says_who_worked(
    ) {
        // Given — plus personne dans le worktree lié, mais Ash a vu `codex` y faire naître un
        // commit avant que l'onglet ne ferme (ADR-0014).
        let table = TableBuilder::new()
            .tab(MAIN, Some("claude"), AgentState::Working)
            .journalled(LINKED, "codex", 2)
            .build();

        // When
        let rows = table.rows();

        // Then
        let worked = row_of(&rows, LINKED)
            .last_worked_by
            .clone()
            .expect("le journal a vu ce commit naître ici");
        assert_eq!(worked.agent, "codex");
        assert_eq!(worked.source, WorkSource::Commit);
        assert_eq!(worked.at, NOW - 2 * DAY);
    }

    #[test]
    fn given_a_worktree_ash_never_saw_an_agent_in_when_the_table_is_built_then_it_claims_nothing() {
        // Given — un agent a pu y passer la nuit sans rien valider, et son onglet est fermé.
        // Le journal ne le connaît pas, et Ash n'a rien d'autre : la colonne se tait plutôt
        // que de nommer le dernier agent du dépôt, qui n'a jamais mis les pieds là.
        let table = TableBuilder::new()
            .tab(MAIN, Some("claude"), AgentState::Working)
            .build();

        // When
        let rows = table.rows();

        // Then
        assert_eq!(row_of(&rows, LINKED).last_worked_by, None);
    }

    #[test]
    fn given_a_dirty_worktree_untouched_for_four_days_when_the_table_is_built_then_it_is_stale() {
        // Given — la règle datée de la spec §5.4. L'heure vient de l'horloge injectée : ce
        // test dit lui-même quel jour on est, et il dira la même chose dans trois jours.
        let table = TableBuilder::new()
            .tab(MAIN, Some("claude"), AgentState::Working)
            .dirty(LINKED)
            .journalled(LINKED, "codex", 4)
            .build();

        // When
        let rows = table.rows();

        // Then
        assert!(row_of(&rows, LINKED).stale);
    }

    #[test]
    fn given_a_worktree_dirty_for_four_days_but_still_inhabited_when_the_table_is_built_then_it_is_not_stale(
    ) {
        // Given — les deux conditions de la spec §5.4 sont conjointes, et « sans agent » se
        // lit au présent : un agent qui travaille dedans maintenant interdit le mot.
        let table = TableBuilder::new()
            .tab(LINKED, Some("codex"), AgentState::Working)
            .dirty(LINKED)
            .journalled(LINKED, "codex", 4)
            .build();

        // When
        let rows = table.rows();

        // Then
        assert!(!row_of(&rows, LINKED).stale);
    }

    #[test]
    fn given_a_clean_worktree_untouched_for_four_days_when_the_table_is_built_then_it_is_not_stale()
    {
        // Given — un worktree propre et vieux n'est pas oublié : il est fini. Le signaler
        // apprendrait à ignorer le mot le jour où il compte.
        let table = TableBuilder::new()
            .tab(MAIN, Some("claude"), AgentState::Working)
            .clean(LINKED)
            .journalled(LINKED, "codex", 4)
            .build();

        // When
        let rows = table.rows();

        // Then
        assert!(!row_of(&rows, LINKED).stale);
    }

    #[test]
    fn given_a_dirty_worktree_whose_last_agent_left_yesterday_when_the_table_is_built_then_it_is_not_stale(
    ) {
        // Given — trois jours, pas un de moins : un worktree d'hier est un worktree en cours.
        let table = TableBuilder::new()
            .tab(MAIN, Some("claude"), AgentState::Working)
            .dirty(LINKED)
            .journalled(LINKED, "codex", 1)
            .build();

        // When
        let rows = table.rows();

        // Then
        assert!(!row_of(&rows, LINKED).stale);
    }

    #[test]
    fn given_a_worktree_that_holds_uncommitted_work_and_an_agent_when_removal_is_asked_then_it_says_what_would_be_lost(
    ) {
        // Given — la spec §5.4 : la suppression doit énoncer ce qu'elle emporte **avant** de
        // le faire. Ash ne la fait pas ; il l'énonce, et rend la commande comme du texte
        // (ADR-0015).
        let table = TableBuilder::new()
            .tab(LINKED, Some("claude"), AgentState::Working)
            .dirty(LINKED)
            .build();

        // When
        let plan = table
            .removal(Path::new(LINKED))
            .expect("le worktree existe");

        // Then
        assert!(
            plan.carries
                .iter()
                .any(|line| line.contains("3 uncommitted files")),
            "{:?}",
            plan.carries
        );
        assert!(
            plan.carries
                .iter()
                .any(|line| line.contains("claude is running here")),
            "{:?}",
            plan.carries
        );
        assert_eq!(plan.command, format!("git worktree remove {LINKED}"));
        assert_eq!(plan.refused, None);
    }

    #[test]
    fn given_the_main_worktree_when_removal_is_asked_then_it_says_git_will_refuse() {
        // Given — le worktree principal ne se supprime pas : le dire vaut mieux que laisser
        // découvrir le refus après coup.
        let table = TableBuilder::new()
            .tab(MAIN, Some("claude"), AgentState::Working)
            .clean(MAIN)
            .build();

        // When
        let plan = table.removal(Path::new(MAIN)).expect("le worktree existe");

        // Then
        assert!(plan.refused.is_some());
    }

    #[test]
    fn given_a_worktree_whose_status_git_never_answered_when_removal_is_asked_then_it_admits_it() {
        // Given — `git status` muet est un cas nominal (ADR-0011). Juste avant un geste qui
        // détruit, « rien à emporter » serait une affirmation qu'on n'a pas les moyens de
        // faire.
        let table = TableBuilder::new()
            .tab(MAIN, Some("claude"), AgentState::Working)
            .status(LINKED, None)
            .build();

        // When
        let plan = table
            .removal(Path::new(LINKED))
            .expect("le worktree existe");

        // Then
        assert!(
            plan.carries.iter().any(|line| line.contains("unknown")),
            "{:?}",
            plan.carries
        );
    }
}
