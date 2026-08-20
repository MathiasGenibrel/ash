//! Ce que l'onglet de merge montre, et les trois gestes qu'il permet.
//!
//! Tout part d'une relecture : l'opération arrêtée que `features::git` sait déjà lire, et
//! les fichiers du worktree tels que git les a laissés. **Rien n'est retenu entre deux
//! appels** — c'est la contrepartie du critère « fermer l'onglet ne perd rien ».
//!
//! Les trois gestes, dans l'ordre où ils se posent :
//!
//! 1. **trancher un hunk** — le seul qui écrive dans un fichier de l'utilisateur ;
//! 2. **continuer** — le seul qui écrive un commit ;
//! 3. **passer le reste à l'agent** — celui qui n'écrit rien du tout, et qui réutilise le
//!    compositeur de `features::git::prompt` sur les seuls conflits qui restent.
//!
//! # Les chemins qu'Ash refuse de toucher
//!
//! Les chemins viennent de `git status --porcelain=v2`, qui les rend **échappés**
//! (`core.quotePath=true`, voir `features::git::porcelain`) : un nom exotique arrive entre
//! guillemets, avec des séquences `\\nnn`. Ash ne les dé-échappe pas — il les affiche, et
//! **refuse d'ouvrir** ceux-là. Écrire un dé-échappement ici, c'est écrire une seconde fois
//! l'analyseur de chemins de git, sur un chemin qui finit par un `std::fs::write` dans un
//! dépôt qu'on n'a pas choisi. Le fichier reste listé et compté ; il se résout dans un
//! éditeur, et le compte à droite de `continue` en tient compte.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::features::git::{
    compose_conflict_prompt, Head, Operation, PromptSubject, StoppedOperation,
};

use super::conflict::{hunks, resolve, ConflictFile};
use super::error::MergeError;
use super::ports::{ConflictFiles, MergeOutcome, StoppedWorktree, TreeGit};
use super::sides::{continuation, sides, MergeSides};
use super::tabs::{title, MergeTabInfo, MergeTabs, TabId};

/// L'onglet de merge, tel qu'il traverse la frontière.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MergeView {
    pub tab_id: TabId,
    pub worktree_root: String,
    pub title: String,
    /// `None` quand plus rien n'est arrêté dans ce worktree — le rebase a été terminé ou
    /// abandonné ailleurs. L'onglet reste ouvert et le dit.
    pub stopped: Option<StoppedView>,
}

/// L'opération arrêtée, prête à s'afficher en trois panneaux.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StoppedView {
    pub operation: Operation,
    /// Les deux côtés, **nommés par leur branche** (spec §7.4).
    pub sides: MergeSides,
    /// Les fichiers en conflit que git a nommés, dans son ordre.
    pub files: Vec<ConflictFile>,
    /// Combien de conflits git compte **au-delà** de la liste, qui est bornée à cent.
    ///
    /// Zéro dans tous les cas ordinaires. Non nul, il éteint `continue` : Ash ne sait pas
    /// dire de ceux-là s'ils sont résolus, et un bouton allumé sur une ignorance mentirait.
    pub hidden: u32,
    /// Le compte à droite de `continue` : combien de fichiers portent encore des marqueurs.
    pub unresolved: u32,
    /// `ORIG_HEAD` abrégé — le filet de secours, affiché et jamais utilisé.
    pub orig_head: Option<String>,
    /// `abort` et `skip`, **du texte** : la spec §7.4 les veut visibles avant d'entrer, et
    /// ADR-0015 interdit qu'Ash les exécute — `--abort` jette le travail de l'utilisateur.
    pub escapes: Vec<String>,
    /// Comment `continue` s'appelle pour cette opération — le libellé du bouton.
    pub continue_command: String,
    /// Le bouton est allumé. Faux tant qu'il reste un conflit, comme le veut le critère.
    pub can_continue: bool,
}

/// La feature, assemblée avec ses trois ports.
///
/// Un seul objet plutôt que trois paramètres traînés de fonction en fonction : c'est lui
/// que le composition root tient et que les commandes reçoivent.
pub struct MergeSurface {
    tabs: MergeTabs,
    worktrees: Arc<dyn StoppedWorktree>,
    files: Arc<dyn ConflictFiles>,
    git: Arc<dyn TreeGit>,
}

impl MergeSurface {
    pub fn new(
        worktrees: Arc<dyn StoppedWorktree>,
        files: Arc<dyn ConflictFiles>,
        git: Arc<dyn TreeGit>,
    ) -> Self {
        Self {
            tabs: MergeTabs::default(),
            worktrees,
            files,
            git,
        }
    }

    /// Ouvre l'onglet de merge d'un worktree — `⌘⌃M`, une fois que #32 l'aura déclaré.
    ///
    /// Refuse quand rien n'est arrêté : un onglet de merge sur un worktree tranquille
    /// n'aurait rien à montrer, et c'est aussi ce qui dit à #32 quand son raccourci est
    /// actif — la même réponse que `git_stopped_operation`, sur le même worktree.
    pub fn open(&self, worktree_root: &Path, tab_id: TabId) -> Result<TabId, MergeError> {
        if self.worktrees.stopped(worktree_root).is_none() {
            return Err(MergeError::NothingStopped(
                worktree_root.display().to_string(),
            ));
        }
        Ok(self.tabs.open(worktree_root, tab_id))
    }

    /// Ferme un onglet. Rien n'est écrit, rien n'est perdu.
    pub fn close(&self, tab_id: &str) {
        self.tabs.close(tab_id);
    }

    /// Les onglets de merge ouverts, dans l'ordre.
    pub fn list(&self) -> Vec<MergeTabInfo> {
        self.tabs.list(self.worktrees.as_ref())
    }

    /// Ce que l'onglet montre **maintenant** — relu de bout en bout.
    pub fn view(&self, tab_id: &str) -> Result<MergeView, MergeError> {
        let root = self.tabs.worktree_of(tab_id)?;
        Ok(self.read(tab_id, &root))
    }

    /// Tranche un hunk : réécrit le fichier, et le met dans l'index s'il n'a plus de conflit.
    ///
    /// L'ordre compte, et il n'est pas réversible : le fichier est écrit **avant** le
    /// `git add`. Un `git add` qui précéderait l'écriture mettrait dans l'index un fichier
    /// portant encore ses marqueurs, c'est-à-dire un commit avec des `<<<<<<<` dedans.
    pub fn resolve(
        &self,
        tab_id: &str,
        path: &str,
        hunk: u32,
        resolution: &str,
    ) -> Result<MergeView, MergeError> {
        let root = self.tabs.worktree_of(tab_id)?;
        let full = self
            .safe_path(&root, path)
            .ok_or_else(|| MergeError::Unreadable(path.to_owned()))?;
        let text = self
            .files
            .read(&full)
            .ok_or_else(|| MergeError::Unreadable(path.to_owned()))?;

        let written = resolve(&text, hunk, resolution)
            .ok_or_else(|| MergeError::HunkMoved(path.to_owned()))?;
        self.files
            .write(&full, &written)
            .map_err(MergeError::NotWritten)?;

        // Plus un seul marqueur : le fichier est prêt, et c'est git qui le constate.
        if hunks(&written).is_empty() {
            self.git.stage(&root, path);
        }

        Ok(self.read(tab_id, &root))
    }

    /// `git <op> --continue`. Ne part que sur un geste, et jamais tant qu'un conflit reste.
    ///
    /// Le refus est ici et non seulement dans l'écran : un bouton éteint est une politesse,
    /// une garde est une garantie. Ce qui les relie est le même compte.
    pub fn resume(&self, tab_id: &str) -> Result<MergeOutcome, MergeError> {
        let root = self.tabs.worktree_of(tab_id)?;
        let view = self.read(tab_id, &root);
        let stopped = view
            .stopped
            .ok_or_else(|| MergeError::NothingStopped(root.display().to_string()))?;
        if !stopped.can_continue {
            return Ok(MergeOutcome {
                label: stopped.continue_command,
                success: false,
                output: format!(
                    "{} conflicted file(s) still carry markers — nothing was run",
                    stopped.unresolved + stopped.hidden
                ),
            });
        }
        Ok(self.git.resume(&root, stopped.operation.kind))
    }

    /// Le prompt à passer à l'agent — sur les seuls conflits qui **restent**.
    ///
    /// Il ne s'écrit nulle part : c'est `pty_compose` qui le pose dans le terminal, et
    /// l'utilisateur seul qui l'envoie
    /// ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)). Le
    /// compositeur est celui de `features::git::prompt`, appelé avec un sous-ensemble de
    /// chemins — c'est exactement ce pour quoi `PromptSubject` est plus pauvre que
    /// `StoppedOperation`.
    ///
    /// `None` quand il ne reste rien à passer : un prompt qui ne nomme aucun fichier
    /// demanderait à un agent de résoudre un conflit qui n'existe plus.
    pub fn rest_prompt(&self, tab_id: &str) -> Result<Option<String>, MergeError> {
        let root = self.tabs.worktree_of(tab_id)?;
        let Some(stopped) = self.worktrees.stopped(&root) else {
            return Ok(None);
        };
        let view = self.read(tab_id, &root);
        let Some(shown) = view.stopped else {
            return Ok(None);
        };
        let remaining: Vec<String> = shown
            .files
            .iter()
            .filter(|file| !file.resolved)
            .map(|file| file.path.clone())
            .collect();
        if remaining.is_empty() {
            return Ok(None);
        }

        let total = shown.unresolved + shown.hidden;
        Ok(Some(compose_conflict_prompt(&PromptSubject {
            operation: &stopped.operation,
            paths: &remaining,
            total: (total as usize > remaining.len()).then_some(total),
            stopped_at: stopped.stopped_at.as_ref(),
            test_command: stopped.test_command.as_deref(),
        })))
    }

    /// La relecture, une seule fois écrite : `view`, `resolve` et `resume` y passent tous.
    fn read(&self, tab_id: &str, root: &Path) -> MergeView {
        let stopped = self.worktrees.stopped(root);
        let head = self.worktrees.head(root);
        MergeView {
            tab_id: tab_id.to_owned(),
            worktree_root: root.display().to_string(),
            title: title(stopped.as_ref().map(|it| &it.operation), head.as_ref()),
            stopped: match (stopped, head) {
                (Some(stopped), Some(head)) => Some(self.shown(root, &stopped, &head)),
                _ => None,
            },
        }
    }

    fn shown(&self, root: &Path, stopped: &StoppedOperation, head: &Head) -> StoppedView {
        let files: Vec<ConflictFile> = stopped
            .conflicts
            .iter()
            .map(|path| self.file(root, path))
            .collect();

        // Le compte des conflits que git connaît et que la liste ne porte pas. `saturating`
        // parce que les deux nombres viennent de la même lecture mais pas du même champ :
        // un total plus petit que la liste serait une incohérence de git, pas une soustraction
        // à laisser déborder.
        let hidden = stopped
            .conflicted_total
            .unwrap_or(0)
            .saturating_sub(files.len() as u32);
        let unresolved = files.iter().filter(|file| !file.resolved).count() as u32;

        StoppedView {
            sides: sides(&stopped.operation, head),
            continue_command: continuation(stopped.operation.kind),
            can_continue: unresolved == 0 && hidden == 0,
            operation: stopped.operation.clone(),
            files,
            hidden,
            unresolved,
            orig_head: stopped.orig_head.clone(),
            escapes: stopped.escapes.clone(),
        }
    }

    fn file(&self, root: &Path, path: &str) -> ConflictFile {
        let content = self
            .safe_path(root, path)
            .and_then(|full| self.files.read(&full));
        match content {
            Some(text) => {
                let hunks = hunks(&text);
                ConflictFile {
                    path: path.to_owned(),
                    resolved: hunks.is_empty(),
                    hunks,
                    unreadable: false,
                }
            }
            // Illisible : listé, compté comme **non résolu**, et jamais réécrit. Le compter
            // résolu allumerait `continue` sur un fichier qu'Ash n'a pas su ouvrir.
            None => ConflictFile {
                path: path.to_owned(),
                hunks: Vec::new(),
                resolved: false,
                unreadable: true,
            },
        }
    }

    /// Le chemin réel d'un conflit, ou **rien**.
    ///
    /// Trois refus, et chacun a coûté une faille ailleurs : un chemin **échappé** par git
    /// (`"src/\303\251.rs"`), qu'Ash ne dé-échappe pas ; un chemin **absolu**, qui sortirait
    /// du worktree ; un chemin qui **remonte** (`..`), qui en sortirait aussi. Ce qui reste
    /// est une suite de segments ordinaires, jointe à une racine que le backend détient.
    fn safe_path(&self, root: &Path, path: &str) -> Option<PathBuf> {
        if path.is_empty() || path.starts_with('"') {
            return None;
        }
        let relative = Path::new(path);
        let ordinary = relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
        ordinary.then(|| root.join(relative))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::merge::fakes::{FakeFiles, FakeGit, FakeWorktree};

    const ROOT: &str = "/dev/ash";
    const TWO_HUNKS: &str = "a\n<<<<<<< HEAD\nmain\n=======\nfeat\n>>>>>>> feat\n\
                             b\n<<<<<<< HEAD\nun\n=======\ndeux\n>>>>>>> feat\nc\n";
    const ONE_HUNK: &str = "<<<<<<< HEAD\nmain\n=======\nfeat\n>>>>>>> feat\n";

    /// Test Data Builder : un onglet de merge ouvert sur un rebase arrêté.
    struct Desk {
        surface: MergeSurface,
        files: Arc<FakeFiles>,
        git: Arc<FakeGit>,
        tab_id: TabId,
    }

    impl Desk {
        fn on(worktree: FakeWorktree, files: FakeFiles) -> Self {
            let files = Arc::new(files);
            let git = Arc::new(FakeGit::new());
            let surface = MergeSurface::new(
                Arc::new(worktree),
                Arc::clone(&files) as Arc<dyn ConflictFiles>,
                Arc::clone(&git) as Arc<dyn TreeGit>,
            );
            let tab_id = surface
                .open(Path::new(ROOT), "01TAB".to_owned())
                .expect("le worktree du décor a une opération arrêtée");
            Self {
                surface,
                files,
                git,
                tab_id,
            }
        }

        /// Le décor courant : un rebase, deux fichiers, deux hunks dans le premier.
        fn rebase() -> Self {
            Self::on(
                FakeWorktree::rebase(),
                FakeFiles::new()
                    .with("/dev/ash/src/probe.rs", TWO_HUNKS)
                    .with("/dev/ash/src/main.ts", ONE_HUNK),
            )
        }

        fn shown(&self) -> StoppedView {
            self.surface
                .view(&self.tab_id)
                .expect("l'onglet existe")
                .stopped
                .expect("l'opération est arrêtée")
        }
    }

    #[test]
    fn given_a_stopped_rebase_with_two_conflicted_files_when_the_tab_is_shown_then_continue_is_dark_and_counts_them(
    ) {
        // Given — le critère d'acceptation : « `continue` reste visible mais éteint tant
        // qu'il reste des conflits, avec le compte »
        let desk = Desk::rebase();

        // When
        let shown = desk.shown();

        // Then
        assert_eq!(shown.unresolved, 2);
        assert!(!shown.can_continue);
        assert_eq!(shown.continue_command, "git rebase --continue");
    }

    #[test]
    fn given_a_file_whose_last_hunk_is_settled_when_resolving_it_then_it_is_written_and_staged() {
        // Given — un fichier à un seul hunk : le trancher le termine
        let desk = Desk::rebase();

        // When
        let view = desk
            .surface
            .resolve(&desk.tab_id, "src/main.ts", 0, "both")
            .expect("le hunk existe");

        // Then — le fichier du worktree porte la décision, et git l'a dans son index
        assert_eq!(
            desk.files.content("/dev/ash/src/main.ts").as_deref(),
            Some("both\n")
        );
        assert_eq!(desk.git.ran(), vec!["add src/main.ts".to_owned()]);
        let shown = view.stopped.expect("l'opération est toujours arrêtée");
        assert_eq!(shown.unresolved, 1);
    }

    #[test]
    fn given_a_file_with_two_hunks_when_only_the_first_is_settled_then_nothing_is_staged_yet() {
        // Given — « hunk par hunk » : trancher l'un ne met pas le fichier dans l'index,
        // parce qu'il porte encore des marqueurs que git refuserait
        let desk = Desk::rebase();

        // When
        desk.surface
            .resolve(&desk.tab_id, "src/probe.rs", 0, "main")
            .expect("le hunk existe");

        // Then
        assert!(desk.git.ran().is_empty());
        let shown = desk.shown();
        let probe = shown
            .files
            .iter()
            .find(|file| file.path == "src/probe.rs")
            .expect("le fichier est listé");
        assert_eq!(probe.hunks.len(), 1);
        assert!(!probe.resolved);
    }

    #[test]
    fn given_a_merge_tab_closed_after_a_resolution_when_it_is_opened_again_then_the_work_is_still_there(
    ) {
        // Given — le critère : « fermer l'onglet ne perd rien : l'état vit dans l'index git ».
        // La preuve est que la relecture ne passe par aucun champ d'Ash.
        let desk = Desk::rebase();
        desk.surface
            .resolve(&desk.tab_id, "src/probe.rs", 0, "main")
            .expect("le hunk existe");

        // When
        desk.surface.close(&desk.tab_id);
        let reopened = desk
            .surface
            .open(Path::new(ROOT), "01AGAIN".to_owned())
            .expect("l'opération est toujours arrêtée");

        // Then
        let shown = desk
            .surface
            .view(&reopened)
            .expect("l'onglet existe")
            .stopped
            .expect("l'opération est arrêtée");
        let probe = shown
            .files
            .iter()
            .find(|file| file.path == "src/probe.rs")
            .expect("le fichier est listé");
        assert_eq!(probe.hunks.len(), 1);
        assert_eq!(probe.hunks[0].ours, "un\n");
    }

    #[test]
    fn given_a_tab_where_a_conflict_remains_when_continue_is_asked_anyway_then_git_is_not_run() {
        // Given — le bouton éteint est une politesse ; la garde est la garantie. Un
        // `rebase --continue` lancé sur un fichier à marqueurs commiterait des `<<<<<<<`.
        let desk = Desk::rebase();

        // When
        let outcome = desk.surface.resume(&desk.tab_id).expect("l'onglet existe");

        // Then
        assert!(!outcome.success);
        assert!(desk.git.ran().is_empty());
    }

    #[test]
    fn given_every_conflict_settled_when_continue_is_asked_then_git_runs_and_says_what_it_did() {
        // Given
        let desk = Desk::on(
            FakeWorktree::rebase().conflicting(&["src/main.ts"]),
            FakeFiles::new().with("/dev/ash/src/main.ts", ONE_HUNK),
        );
        desk.surface
            .resolve(&desk.tab_id, "src/main.ts", 0, "both")
            .expect("le hunk existe");

        // When
        let outcome = desk.surface.resume(&desk.tab_id).expect("l'onglet existe");

        // Then
        assert!(outcome.success);
        assert_eq!(outcome.output, "Successfully rebased");
        assert!(desk.git.ran().contains(&"continue Rebase".to_owned()));
    }

    #[test]
    fn given_more_conflicts_than_git_listed_when_the_tab_is_shown_then_continue_stays_dark() {
        // Given — la liste des chemins est bornée à cent, le compte ne l'est pas. Allumer
        // `continue` sur les cent qu'on voit dirait « plus rien à résoudre » à un dépôt qui
        // en a trois mille.
        let desk = Desk::on(
            FakeWorktree::rebase()
                .conflicting(&["src/main.ts"])
                .counting(3_000),
            FakeFiles::new().with("/dev/ash/src/main.ts", ONE_HUNK),
        );
        desk.surface
            .resolve(&desk.tab_id, "src/main.ts", 0, "both")
            .expect("le hunk existe");

        // When
        let shown = desk.shown();

        // Then
        assert_eq!(shown.unresolved, 0);
        assert_eq!(shown.hidden, 2_999);
        assert!(!shown.can_continue);
    }

    #[test]
    fn given_a_path_git_had_to_quote_when_the_tab_shows_it_then_it_is_listed_and_never_written() {
        // Given — `core.quotePath=true` rend un nom exotique entre guillemets. Le
        // dé-échapper ici, c'est réécrire l'analyseur de chemins de git juste avant un
        // `write` dans un dépôt qu'on n'a pas choisi.
        let desk = Desk::on(
            FakeWorktree::rebase().conflicting(&["\"src/\\303\\251.rs\""]),
            FakeFiles::new(),
        );

        // When
        let shown = desk.shown();
        let written = desk
            .surface
            .resolve(&desk.tab_id, "\"src/\\303\\251.rs\"", 0, "anything");

        // Then — listé, compté comme non résolu, et refusé à l'écriture
        assert_eq!(shown.files.len(), 1);
        assert!(shown.files[0].unreadable);
        assert!(!shown.files[0].resolved);
        assert!(!shown.can_continue);
        assert!(matches!(written, Err(MergeError::Unreadable(_))));
    }

    #[test]
    fn given_a_path_that_climbs_out_of_the_worktree_when_resolving_it_then_nothing_is_written() {
        // Given — le chemin vient de la sortie de git sur un dépôt qu'Ash n'a pas choisi
        let desk = Desk::on(
            FakeWorktree::rebase().conflicting(&["../../.ssh/authorized_keys"]),
            FakeFiles::new().with("/dev/ash/../../.ssh/authorized_keys", ONE_HUNK),
        );

        // When
        let written = desk
            .surface
            .resolve(&desk.tab_id, "../../.ssh/authorized_keys", 0, "owned");

        // Then
        assert!(matches!(written, Err(MergeError::Unreadable(_))));
        assert_eq!(
            desk.files
                .content("/dev/ash/../../.ssh/authorized_keys")
                .as_deref(),
            Some(ONE_HUNK)
        );
    }

    #[test]
    fn given_a_tab_where_one_file_is_settled_when_handing_the_rest_over_then_the_prompt_names_only_what_is_left(
    ) {
        // Given — « passer le reste à claude » réutilise le compositeur de #29 sur un
        // **sous-ensemble** de chemins : c'est ce pour quoi `PromptSubject` est plus pauvre
        // que `StoppedOperation`
        let desk = Desk::rebase();
        desk.surface
            .resolve(&desk.tab_id, "src/main.ts", 0, "both")
            .expect("le hunk existe");

        // When
        let prompt = desk
            .surface
            .rest_prompt(&desk.tab_id)
            .expect("l'onglet existe")
            .expect("il reste un conflit");

        // Then
        assert!(prompt.contains("src/probe.rs"));
        assert!(!prompt.contains("src/main.ts"));
        // L'invariant de `compose_conflict_prompt` traverse avec lui : dans un PTY, un saut
        // de ligne *est* la touche `⏎`.
        assert!(!prompt.contains('\n'));
    }

    #[test]
    fn given_a_tab_where_nothing_is_left_when_handing_the_rest_over_then_there_is_no_prompt() {
        // Given — un prompt qui ne nomme aucun fichier demanderait à un agent de résoudre
        // un conflit qui n'existe plus
        let desk = Desk::on(
            FakeWorktree::rebase().conflicting(&["src/main.ts"]),
            FakeFiles::new().with("/dev/ash/src/main.ts", ONE_HUNK),
        );
        desk.surface
            .resolve(&desk.tab_id, "src/main.ts", 0, "both")
            .expect("le hunk existe");

        // When
        let prompt = desk
            .surface
            .rest_prompt(&desk.tab_id)
            .expect("l'onglet existe");

        // Then
        assert_eq!(prompt, None);
    }

    #[test]
    fn given_a_worktree_with_nothing_stopped_when_a_merge_tab_is_asked_for_then_it_is_refused() {
        // Given — c'est aussi la réponse dont #32 a besoin : `⌘⌃M` n'est actif que pendant
        // un rebase ou un merge arrêté
        let surface = MergeSurface::new(
            Arc::new(FakeWorktree::none()),
            Arc::new(FakeFiles::new()),
            Arc::new(FakeGit::new()),
        );

        // When
        let opened = surface.open(Path::new(ROOT), "01TAB".to_owned());

        // Then
        assert_eq!(opened, Err(MergeError::NothingStopped(ROOT.to_owned())));
    }

    #[test]
    fn given_a_stopped_merge_when_the_tab_is_shown_then_the_sides_are_not_the_ones_a_rebase_would_show(
    ) {
        // Given — les deux opérations, sur les deux mêmes branches
        let rebase = Desk::rebase().shown();
        let merge = Desk::on(
            FakeWorktree::merge(),
            FakeFiles::new().with("/dev/ash/src/probe.rs", ONE_HUNK),
        )
        .shown();

        // When
        let escapes = merge.escapes.clone();

        // Then — `main` reste à gauche, mais son **rôle** change, et un merge n'a pas de
        // `--skip` : il n'a qu'un pas
        assert_eq!(rebase.sides.left.name, "main");
        assert_eq!(merge.sides.left.name, "main");
        assert_ne!(rebase.sides.left.role, merge.sides.left.role);
        assert_eq!(escapes, vec!["git merge --abort".to_owned()]);
        assert_eq!(merge.continue_command, "git merge --continue");
    }
}
