//! Le graphe tel qu'il traverse la frontière : des lignes, des couloirs, et la colonne `by`.
//!
//! Ce module ne calcule rien de ce qui se dessine — c'est [`super::graph`] — et ne lance rien
//! — c'est [`super::git_cli`]. Il **assemble** : une fenêtre de commits, l'attribution qu'Ash
//! a observée, et le placement en couloirs, en une page que l'écran rend sans rien décider.
//!
//! # Ce que la colonne `by` dit, et qui le décide
//!
//! Elle est la raison d'être de l'écran (spec §7.2) : elle nomme **l'agent** qui a écrit le
//! commit, et un commit sans attribution connue affiche simplement son auteur git. Le mot
//! affiché est composé **ici**, pas dans la fenêtre : c'est la discipline du dépôt
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et c'est ce qui garde
//! `attributed` — « Ash l'a réellement observé » — distinct du nom qu'on écrit à côté. Un
//! écran qui déciderait du repli finirait par montrer un nom d'agent qu'Ash n'a jamais vu, ce
//! qu'ADR-0014 interdit explicitement.

use std::path::Path;
use std::sync::Arc;

use crate::shared::time::Clock;

use super::attribution::{Attribution, Attributions};
use super::git_cli::{GraphLog, MAX_GRAPH_WINDOW};
use super::graph::{lay_out, GraphCommit, Link};
use super::CommitRecord;

/// La taille de fenêtre d'un graphe qui s'ouvre.
///
/// Deux cents lignes : de quoi remplir plusieurs fois la hauteur du panneau bas, donc de quoi
/// dérouler sans attendre, et assez peu pour que `git log --topo-order` réponde en quelques
/// dizaines de millisecondes sur un dépôt de plusieurs milliers de commits.
pub const DEFAULT_WINDOW: usize = 200;

/// Un trait du dessin, tel qu'il traverse la frontière.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GraphLink {
    /// La colonne au niveau de cette ligne.
    pub from: usize,
    /// La colonne au niveau de la ligne d'en dessous.
    pub to: usize,
}

impl From<Link> for GraphLink {
    fn from(link: Link) -> Self {
        Self {
            from: link.from,
            to: link.to,
        }
    }
}

/// Une ligne du graphe.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CommitRow {
    /// L'identifiant complet — la clé du panneau de détail, et rien d'autre.
    pub sha: String,
    /// L'identifiant abrégé, tel que git l'abrège : c'est ce qu'on affiche.
    pub short: String,
    pub subject: String,
    /// **La colonne `by`** : le nom d'un agent quand Ash l'a observé, le nom d'auteur git
    /// sinon. Toujours un mot — une colonne vide se lirait comme une panne.
    pub by: String,
    /// Ash a-t-il **observé** cet agent écrire ce commit (ADR-0014) ?
    ///
    /// Ce n'est pas déductible de `by` : un dépôt où l'auteur git s'appelle `claude` rendrait
    /// les deux indiscernables, et la colonne cesserait de dire ce qu'elle promet.
    pub attributed: bool,
    /// Le nom d'auteur git, toujours donné — le panneau de détail le montre même quand la
    /// colonne `by` nomme un agent.
    pub author: String,
    /// La date d'auteur, telle que git l'écrit (ISO 8601 strict).
    pub author_date: String,
    /// En secondes Unix : de quoi écrire « il y a 3 jours » sans analyser une chaîne.
    ///
    /// `number` et non `bigint` : ts-rs rend un `u64` en `bigint`, que `JSON.parse` ne
    /// produit jamais — la valeur qui arriverait vraiment serait un `number`, et le contrat
    /// mentirait. Même conduite que `Subagent.since`.
    #[cfg_attr(test, ts(type = "number"))]
    pub authored_at: u64,
    /// Les refs qui pointent ici, `HEAD -> ` compris. Vide pour la plupart des lignes.
    pub refs: Vec<String>,
    pub lane: usize,
    /// Les traits qui descendent de cette ligne vers la suivante.
    pub links: Vec<GraphLink>,
    /// L'onglet où l'agent tournait, quand il y en a un.
    pub tab_id: Option<String>,
    /// Le prompt qui a produit ce commit, **quand il existe** (ADR-0014).
    ///
    /// Vide aujourd'hui pour tout le monde : le champ du journal n'a pas encore de source, et
    /// Ash n'en fabrique pas. Voir [`Self::prompt_note`].
    pub prompt: Option<String>,
    /// Ce que le panneau de détail dit **à la place** du prompt quand il n'y en a pas.
    ///
    /// Deux absences, et ce ne sont pas les mêmes : un commit qu'Ash a vu naître sans avoir
    /// retenu le prompt, et un commit qu'Ash n'a pas vu naître du tout. Les confondre ferait
    /// croire à une perte là où il n'y a rien eu à perdre.
    pub prompt_note: String,
}

/// Une branche repliée par la règle des 30 jours, telle qu'elle traverse la frontière.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FoldedBranch {
    pub name: String,
    /// La date de son dernier commit, en secondes Unix. Voir [`CommitRow::authored_at`].
    #[cfg_attr(test, ts(type = "number"))]
    pub last_activity: u64,
}

/// Une fenêtre de graphe, telle que l'écran la rend.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CommitGraph {
    pub rows: Vec<CommitRow>,
    /// Combien de couloirs réserver en largeur.
    pub lanes: usize,
    pub folded: Vec<FoldedBranch>,
    /// La fenêtre demandée — celle qu'il faudra redemander plus grande pour voir plus loin.
    pub window: usize,
    /// Reste-t-il de l'histoire au-delà ?
    ///
    /// Se lit d'une seule façon : la fenêtre a rendu autant de commits qu'elle en demandait.
    /// C'est faux une fois sur `window`, quand l'histoire fait pile cette taille — et la
    /// conséquence est un bouton qui ne rend rien de plus, pas une histoire perdue.
    pub has_more: bool,
}

/// Ce que le panneau de détail dit quand aucun prompt n'a été retenu, pour un commit
/// **attribué**.
///
/// Le mot compte : Ash a bien vu l'agent écrire, il n'a simplement pas gardé la question qui
/// l'a déclenché. Dire « aucun agent » serait faux.
fn no_prompt_but_attributed(agent: &str) -> String {
    format!("ash saw {agent} write this commit, but kept no prompt for it")
}

/// Ce que le panneau de détail dit pour un commit **non attribué**.
///
/// C'est le cas courant, et il ne doit pas se lire comme une panne : un commit tapé à la main
/// n'a jamais eu d'agent, et un commit né avant qu'Ash regarde n'a pas pu être observé.
const NOT_OBSERVED: &str = "no agent was observed writing this commit";

impl CommitRow {
    /// Une ligne, à partir de ce que git dit et de ce qu'Ash a vu.
    fn of(commit: &GraphCommit, seen: Option<&Attribution>, lane: usize, links: &[Link]) -> Self {
        let prompt = seen
            .and_then(|seen| seen.prompt.clone())
            .filter(|prompt| !prompt.is_empty());
        let prompt_note = match (seen, prompt.is_some()) {
            (_, true) => String::new(),
            (Some(seen), false) => no_prompt_but_attributed(&seen.agent),
            (None, false) => NOT_OBSERVED.to_owned(),
        };
        Self {
            sha: commit.sha.clone(),
            short: commit.short.clone(),
            subject: commit.subject.clone(),
            by: seen.map_or_else(|| commit.author.clone(), |seen| seen.agent.clone()),
            attributed: seen.is_some(),
            author: commit.author.clone(),
            author_date: commit.author_date.clone(),
            authored_at: commit.authored_at,
            refs: commit.refs.clone(),
            lane,
            links: links.iter().copied().map(GraphLink::from).collect(),
            tab_id: seen.map(|seen| seen.tab_id.clone()),
            prompt,
            prompt_note,
        }
    }
}

/// Le lecteur du graphe : un `git log`, une jointure, un placement.
///
/// Les trois dépendances sont des **ports**, et la troisième mérite un mot : l'horloge n'est
/// pas ici pour dater ce qui s'affiche, mais parce que la règle des 30 jours est une règle
/// datée. Un `SystemTime::now()` au milieu de la règle ferait un test qui casse tout seul le
/// trente-et-unième jour.
pub struct CommitGraphReader {
    log: Arc<dyn GraphLog>,
    attributions: Arc<dyn Attributions>,
    clock: Arc<dyn Clock>,
}

impl CommitGraphReader {
    pub fn new(
        log: Arc<dyn GraphLog>,
        attributions: Arc<dyn Attributions>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            log,
            attributions,
            clock,
        }
    }

    /// La fenêtre du graphe d'un worktree, attribution comprise.
    ///
    /// `repo` est le dossier git **commun** : c'est la clé du journal, et deux worktrees d'un
    /// même projet partagent donc la même attribution — ce qui est exactement ce qu'on veut,
    /// puisqu'ils partagent aussi les commits.
    ///
    /// La fenêtre repart **toujours du sommet**, et « voir plus loin » la redemande plus
    /// grande. Ce n'est pas une paresse de pagination : les couloirs d'une ligne dépendent de
    /// tout ce qui la précède, donc une page qui commencerait au milieu ne saurait pas quels
    /// traits y arrivent (voir [`super::graph`]).
    pub fn window(&self, worktree_root: &Path, repo: &str, window: usize) -> CommitGraph {
        let window = window.clamp(1, MAX_GRAPH_WINDOW);
        let commits = self.log.window(worktree_root, window);
        let has_more = commits.len() >= window;

        // **Une seule demande pour toute la page** : le journal relit son fichier à chaque
        // question, et deux cents questions feraient deux cents lectures du même fichier.
        let keys: Vec<CommitRecord> = commits.iter().map(as_record).collect();
        let seen = self.attributions.of(repo, &keys);

        let layout = lay_out(&commits, self.clock.wall());
        let rows = layout
            .rows
            .iter()
            .filter_map(|placed| {
                let commit = commits.get(placed.commit)?;
                let attributed = seen.get(placed.commit).and_then(Option::as_ref);
                Some(CommitRow::of(
                    commit,
                    attributed,
                    placed.lane,
                    &placed.links,
                ))
            })
            .collect();

        CommitGraph {
            rows,
            lanes: layout.lanes,
            folded: layout
                .folded
                .into_iter()
                .map(|branch| FoldedBranch {
                    name: branch.name,
                    last_activity: branch.last_activity,
                })
                .collect(),
            window,
            has_more,
        }
    }
}

/// La clé d'attribution d'ADR-0014, tirée d'un commit dessinable.
///
/// Les quatre champs sont **exactement** ceux que la résolution en deux temps compare : le
/// `sha`, puis `(author_date, subject)`. Rien n'est reformaté en route — normaliser une date
/// ici, ce serait ne plus reconnaître après un rebase ce qu'Ash a lui-même écrit.
fn as_record(commit: &GraphCommit) -> CommitRecord {
    CommitRecord {
        sha: commit.sha.clone(),
        author_date: commit.author_date.clone(),
        authored_at: commit.authored_at,
        subject: commit.subject.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    use crate::shared::time::UnixMillis;

    const NOW: UnixMillis = 1_786_000_000_000;
    const REPO: &str = "/dev/ash/.git";
    const WORKTREE: &str = "/dev/ash";

    /// L'horloge figée du dépôt : la règle des 30 jours se prouve à une date écrite, jamais à
    /// celle du jour où le test tourne.
    struct FrozenClock;

    impl Clock for FrozenClock {
        fn now(&self) -> std::time::Instant {
            std::time::Instant::now()
        }

        fn wall(&self) -> UnixMillis {
            NOW
        }
    }

    /// Un `git log` de graphe en mémoire.
    struct FakeLog(Vec<GraphCommit>);

    impl GraphLog for FakeLog {
        fn window(&self, _worktree_root: &Path, limit: usize) -> Vec<GraphCommit> {
            self.0.iter().take(limit).cloned().collect()
        }
    }

    /// Ce qu'Ash a vu, indexé par `sha` : le double du journal, sans son fichier.
    struct FakeAttributions(HashMap<String, Attribution>);

    impl Attributions for FakeAttributions {
        fn of(&self, _repo: &str, commits: &[CommitRecord]) -> Vec<Option<Attribution>> {
            commits
                .iter()
                .map(|commit| self.0.get(&commit.sha).cloned())
                .collect()
        }
    }

    /// Test Data Builder : un lecteur de graphe branché sur des doubles.
    struct ReaderBuilder {
        commits: Vec<GraphCommit>,
        seen: HashMap<String, Attribution>,
    }

    impl ReaderBuilder {
        fn new() -> Self {
            Self {
                commits: Vec::new(),
                seen: HashMap::new(),
            }
        }

        fn commit(mut self, sha: &str, subject: &str, author: &str) -> Self {
            self.commits.push(GraphCommit {
                sha: sha.to_owned(),
                short: sha.to_owned(),
                // Sans parent : ce qui se prouve ici est la colonne `by`, et les couloirs ont
                // leur propre suite de tests, dans le module qui les calcule.
                parents: Vec::new(),
                author_date: "2026-08-12T14:03:21+02:00".to_owned(),
                authored_at: NOW / 1_000,
                author: author.to_owned(),
                refs: Vec::new(),
                subject: subject.to_owned(),
            });
            self
        }

        fn written_by(mut self, sha: &str, agent: &str, prompt: Option<&str>) -> Self {
            self.seen.insert(
                sha.to_owned(),
                Attribution {
                    agent: agent.to_owned(),
                    tab_id: "01J0TAB".to_owned(),
                    prompt: prompt.map(str::to_owned),
                },
            );
            self
        }

        fn read(self) -> CommitGraph {
            let reader = CommitGraphReader::new(
                Arc::new(FakeLog(self.commits)),
                Arc::new(FakeAttributions(self.seen)),
                Arc::new(FrozenClock),
            );
            reader.window(&PathBuf::from(WORKTREE), REPO, DEFAULT_WINDOW)
        }
    }

    #[test]
    fn given_a_commit_ash_saw_an_agent_write_when_the_graph_is_read_then_the_by_column_names_the_agent(
    ) {
        // Given — c'est la raison d'être de l'écran (spec §7.2) : sans cette ligne, le graphe
        // serait un `git log --graph` avec des pixels.
        let graph = ReaderBuilder::new()
            .commit("8f3a1c2", "feat: onglets", "mathias")
            .written_by("8f3a1c2", "claude", None)
            .read();

        // When
        let row = &graph.rows[0];

        // Then — et le nom d'auteur git reste disponible, il n'est pas remplacé
        assert_eq!(row.by, "claude");
        assert!(row.attributed);
        assert_eq!(row.author, "mathias");
    }

    #[test]
    fn given_a_commit_typed_by_hand_when_the_graph_is_read_then_the_by_column_falls_back_to_its_git_author(
    ) {
        // Given — ADR-0014 : « la colonne `by` ne montre un nom d'agent que quand Ash l'a
        // réellement observé ». Un commit sans correspondance n'est pas orphelin.
        let graph = ReaderBuilder::new()
            .commit("1b2c3d4", "fix: à la main", "mathias")
            .read();

        // When
        let row = &graph.rows[0];

        // Then
        assert_eq!(row.by, "mathias");
        assert!(!row.attributed);
        assert_eq!(row.tab_id, None);
    }

    #[test]
    fn given_an_attributed_commit_whose_prompt_was_never_kept_when_it_is_read_then_the_detail_says_so_without_inventing_one(
    ) {
        // Given — le champ `prompt` du journal n'a pas encore de source, et ce sera le cas de
        // tous les commits jusqu'à ce qu'une tranche à part lui en donne une. Fabriquer un
        // texte de remplacement ferait croire à un prompt qui n'a jamais existé.
        let graph = ReaderBuilder::new()
            .commit("8f3a1c2", "feat: onglets", "mathias")
            .written_by("8f3a1c2", "claude", None)
            .read();

        // When
        let row = &graph.rows[0];

        // Then — l'absence est dite, et elle dit **laquelle** des deux absences c'est
        assert_eq!(row.prompt, None);
        assert!(row.prompt_note.contains("claude"), "{}", row.prompt_note);
        assert!(row.prompt_note.contains("no prompt"), "{}", row.prompt_note);
    }

    #[test]
    fn given_an_unattributed_commit_when_its_detail_is_read_then_it_does_not_read_as_a_lost_prompt()
    {
        // Given — les deux absences de prompt ne sont pas la même : ici il n'y a rien eu à
        // perdre, et le dire autrement ferait soupçonner une panne du journal.
        let graph = ReaderBuilder::new()
            .commit("1b2c3d4", "fix: à la main", "mathias")
            .read();

        // When
        let note = &graph.rows[0].prompt_note;

        // Then
        assert_eq!(note, NOT_OBSERVED);
    }

    #[test]
    fn given_a_commit_whose_prompt_was_kept_when_it_is_read_then_the_detail_carries_it_and_says_nothing_else(
    ) {
        // Given — le jour où le prompt aura une source, c'est lui qui doit s'afficher, et la
        // phrase d'absence doit disparaître d'elle-même.
        let graph = ReaderBuilder::new()
            .commit("8f3a1c2", "feat: onglets", "mathias")
            .written_by("8f3a1c2", "claude", Some("ajoute les onglets"))
            .read();

        // When
        let row = &graph.rows[0];

        // Then
        assert_eq!(row.prompt.as_deref(), Some("ajoute les onglets"));
        assert_eq!(row.prompt_note, "");
    }

    #[test]
    fn given_a_window_that_git_filled_to_the_brim_when_it_is_read_then_the_graph_says_there_is_more(
    ) {
        // Given — un dépôt plus grand que la fenêtre. Sans ce drapeau, l'écran n'aurait aucun
        // moyen de savoir qu'il montre une histoire tronquée.
        let commits: Vec<GraphCommit> = (0..DEFAULT_WINDOW)
            .map(|index| GraphCommit {
                sha: format!("c{index}"),
                short: format!("c{index}"),
                parents: vec![format!("c{}", index + 1)],
                author_date: "2026-08-12T14:03:21+02:00".to_owned(),
                authored_at: NOW / 1_000,
                author: "mathias".to_owned(),
                refs: Vec::new(),
                subject: format!("commit {index}"),
            })
            .collect();
        let reader = CommitGraphReader::new(
            Arc::new(FakeLog(commits)),
            Arc::new(FakeAttributions(HashMap::new())),
            Arc::new(FrozenClock),
        );

        // When
        let graph = reader.window(&PathBuf::from(WORKTREE), REPO, DEFAULT_WINDOW);

        // Then
        assert!(graph.has_more);
        assert_eq!(graph.rows.len(), DEFAULT_WINDOW);
    }

    #[test]
    fn given_a_repository_shorter_than_the_window_when_it_is_read_then_nothing_promises_more() {
        // Given — le cas d'un dépôt neuf, et celui d'un graphe déroulé jusqu'au bout.
        let graph = ReaderBuilder::new()
            .commit("aaa", "chore: initial import", "mathias")
            .read();

        // Then
        assert!(!graph.has_more);
    }
}
