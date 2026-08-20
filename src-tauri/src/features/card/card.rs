//! La fiche telle que le panneau la reçoit, et l'assemblage qui la produit.
//!
//! Tout ce qui se décide se décide **ici**, en Rust : où vit la fiche, ce que le bloc porte,
//! ce qu'Ash y écrirait, s'il a le droit. L'écran reçoit un état et le rend — il ne relit
//! pas le fichier, ne cherche pas les marqueurs, et ne compose pas la table
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! **La source est envoyée telle quelle**, en revanche, et c'est le seul endroit où la
//! frontière laisse passer du texte non interprété : le rendu est une mise en forme de
//! markdown standard, et ADR-0013 exige précisément qu'il n'invente aucune syntaxe. Ce que
//! l'écran en fait — deux volets, rendu à gauche, source à droite — est un fait d'affichage.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::shared::time::Clock;

use super::error::CardError;
use super::log;
use super::modes::ModeStore;
use super::place::{self, CardMode, Place};
use super::ports::{AgentWork, CardFiles};
use super::write::{self, LogState, LogWrite};

/// Ce que l'écran de la fiche reçoit, et tout ce qu'il reçoit.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BranchCard {
    /// La racine du worktree — la même clé que celle des onglets (`TabLocation`).
    pub worktree_root: String,
    /// Où la fiche est, en toutes lettres : c'est ce qui rend l'interrupteur compréhensible.
    pub path: String,
    /// Où elle irait dans l'autre mode.
    pub other_path: String,
    pub mode: CardMode,
    /// Vrai quand c'est le `.gitignore` du dépôt qui a placé la fiche hors du dépôt.
    pub ignored_by_the_repo: bool,
    pub exists: bool,
    /// Le markdown, **tel quel**. Vide quand la fiche n'existe pas encore.
    pub source: String,
    pub log: CardLog,
}

/// L'état de la zone d'Ash, et ce qu'il y écrirait.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CardLog {
    pub state: LogState,
    /// La table telle qu'elle irait dans le bloc — ce que le bouton pose.
    pub table: String,
    /// Le fichier tel qu'il est face au fichier tel qu'Ash le laisserait (spec §10).
    pub diff: String,
    /// Ce qui se passe, ou ce qui ne se passera pas, en une phrase.
    pub note: String,
    /// Le bouton est-il proposé ? Une seule lecture pour agir **et** pour afficher.
    pub writable: bool,
}

/// La fiche de branche, assemblée.
///
/// Elle ne connaît ni le journal, ni git, ni les onglets : elle pose trois questions à trois
/// ports, et c'est le composition root qui les branche.
pub struct Cards {
    files: Arc<dyn CardFiles>,
    modes: Arc<dyn ModeStore>,
    work: Arc<dyn AgentWork>,
    clock: Arc<dyn Clock>,
    home: PathBuf,
}

impl Cards {
    pub fn new(
        files: Arc<dyn CardFiles>,
        modes: Arc<dyn ModeStore>,
        work: Arc<dyn AgentWork>,
        clock: Arc<dyn Clock>,
        home: PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            files,
            modes,
            work,
            clock,
            home,
        })
    }

    /// La fiche de ce worktree, sans rien écrire.
    pub fn read(&self, worktree_root: &Path) -> Result<BranchCard, CardError> {
        let place = self.place(worktree_root);
        let source = self
            .files
            .read(&place.path)
            .map_err(|why| CardError::Io {
                path: place.path.clone(),
                why,
            })?
            .unwrap_or_default();
        let table = self.table(worktree_root);
        let plan = write::plan(self.files.as_ref(), &place.path, &table)?;

        Ok(BranchCard {
            worktree_root: worktree_root.to_string_lossy().into_owned(),
            path: place.path.to_string_lossy().into_owned(),
            other_path: place.other.to_string_lossy().into_owned(),
            mode: place.mode,
            ignored_by_the_repo: place.ignored_by_the_repo,
            exists: !source.is_empty(),
            source,
            log: CardLog {
                writable: plan.state.lets_ash_write(),
                state: plan.state,
                table: plan.table,
                diff: plan.diff,
                note: plan.note,
            },
        })
    }

    /// Pose le journal dans la fiche — ou refuse, et le dit.
    ///
    /// Rend la fiche **relue après coup**, jamais une fiche déduite de ce qu'on voulait
    /// écrire : c'est le même parti que la purge du journal, et c'est ce qui fait qu'un refus
    /// et une panne de disque se racontent pareil à l'écran — par ce que le fichier dit
    /// maintenant.
    pub fn write_log(&self, worktree_root: &Path) -> Result<BranchCard, CardError> {
        let place = self.place(worktree_root);
        let table = self.table(worktree_root);
        let written = write::write_log(self.files.as_ref(), &place.path, &table)?;
        let mut card = self.read(worktree_root)?;
        card.log.note = said(&written).unwrap_or(card.log.note);
        Ok(card)
    }

    /// Change l'emplacement de la fiche — **sans déplacer aucun fichier**.
    ///
    /// Rien n'est copié, rien n'est effacé, et surtout aucun `.gitignore` n'est touché
    /// (ADR-0013). Le choix dit où Ash regarde ; la fiche de l'autre emplacement reste où
    /// elle est, et le chemin qu'elle occupe est rendu dans `otherPath` pour que l'écran
    /// puisse le nommer.
    pub fn choose(
        &self,
        worktree_root: &Path,
        mode: Option<CardMode>,
    ) -> Result<BranchCard, CardError> {
        self.modes.choose(worktree_root, mode);
        self.read(worktree_root)
    }

    fn place(&self, worktree_root: &Path) -> Place {
        place::locate(
            self.files.as_ref(),
            worktree_root,
            &self.home,
            self.modes.chosen(worktree_root),
        )
    }

    /// La table du journal pour ce worktree, telle qu'elle irait dans le bloc.
    fn table(&self, worktree_root: &Path) -> String {
        log::table(
            &log::tally(&self.work.in_worktree(worktree_root)),
            self.clock.wall(),
        )
    }
}

/// La phrase d'une écriture qui vient d'avoir lieu. `None` quand le plan relu la dit déjà
/// mieux — c'est le cas d'un refus, dont la raison ne change pas d'un appel à l'autre.
fn said(written: &LogWrite) -> Option<String> {
    match written {
        LogWrite::Written {
            created_the_card: true,
            ..
        } => Some("card written, with its log block.".to_owned()),
        LogWrite::Written {
            backup: Some(backup),
            ..
        } => Some(format!(
            "log written. the card as it was is kept in {}.",
            backup.display()
        )),
        LogWrite::Written { .. } => Some("log written.".to_owned()),
        LogWrite::AlreadyCurrent { .. } | LogWrite::Refused { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::card::fakes::{CardBuilder, FakeWork, MemoryCardFiles, MemoryModes};
    use crate::features::card::ports::WorkRecord;
    use crate::shared::time::UnixMillis;

    const WORKTREE: &str = "/dev/ash";
    const HOME: &str = "/Users/moi";
    const CARD: &str = "/dev/ash/.ash/worktree.md";
    const NOON: UnixMillis = 1_755_000_000_000;

    struct FrozenClock;
    impl Clock for FrozenClock {
        fn wall(&self) -> UnixMillis {
            NOON
        }
        fn now(&self) -> std::time::Instant {
            std::time::Instant::now()
        }
    }

    fn cards(files: MemoryCardFiles, work: Vec<WorkRecord>) -> (Arc<Cards>, Arc<MemoryCardFiles>) {
        let files = Arc::new(files);
        (
            Cards::new(
                Arc::clone(&files) as Arc<dyn CardFiles>,
                Arc::new(MemoryModes::default()),
                Arc::new(FakeWork(work)),
                Arc::new(FrozenClock),
                PathBuf::from(HOME),
            ),
            files,
        )
    }

    fn wrote(agent: &str, at: u64) -> WorkRecord {
        WorkRecord {
            agent: agent.to_owned(),
            authored_at: at,
        }
    }

    #[test]
    fn given_a_worktree_where_an_agent_committed_when_the_log_is_written_then_the_card_carries_the_adr_table(
    ) {
        // Given — la tranche entière, d'un commit observé jusqu'aux octets du fichier : c'est
        // le seul test qui prouve que les trois ports se rejoignent.
        let (cards, files) = cards(
            MemoryCardFiles::new().file(CARD, &CardBuilder::new().build()),
            vec![
                wrote("claude", NOON / 1_000 - 922),
                wrote("claude", NOON / 1_000),
            ],
        );

        // When
        let card = cards.write_log(Path::new(WORKTREE));

        // Then
        let card = card.unwrap_or_else(|why| panic!("{why}"));
        assert_eq!(card.mode, CardMode::Repo);
        assert!(
            card.source
                .contains("| claude | 2 commits · 15m22s | now |"),
            "{}",
            card.source
        );
        assert!(
            card.log.note.starts_with("log written"),
            "{}",
            card.log.note
        );
        // …et la sauvegarde est là, avec la fiche telle qu'elle était.
        assert!(files.contents(&format!("{CARD}.bak")).is_some());
    }

    #[test]
    fn given_a_worktree_no_agent_ever_committed_in_when_the_card_is_read_then_it_says_so_without_inventing_a_row(
    ) {
        // Given — le journal ne sait rien de cette branche : Ash a démarré après, ou tout a
        // été commité à la main (ADR-0014). La fiche ne doit pas inventer une ligne.
        let (cards, _) = cards(
            MemoryCardFiles::new().file(CARD, &CardBuilder::new().build()),
            Vec::new(),
        );

        // When
        let card = cards
            .read(Path::new(WORKTREE))
            .unwrap_or_else(|why| panic!("{why}"));

        // Then
        assert_eq!(card.log.state, LogState::Current);
        assert!(card.log.table.is_empty());
        assert!(!card.log.writable);
    }

    #[test]
    fn given_a_user_who_moves_the_card_out_of_the_repository_when_he_chooses_local_then_nothing_is_moved_and_nothing_is_deleted(
    ) {
        // Given — « Ash ne doit ni forcer, ni imposer un `.gitignore` ». L'interrupteur
        // change où Ash regarde, et rien d'autre : la fiche versionnée reste versionnée, et
        // c'est git qui décidera de son sort, pas Ash.
        let versioned = CardBuilder::new().build();
        let (cards, files) = cards(
            MemoryCardFiles::new().file(CARD, &versioned),
            vec![wrote("claude", NOON / 1_000)],
        );

        // When
        let card = cards
            .choose(Path::new(WORKTREE), Some(CardMode::Local))
            .unwrap_or_else(|why| panic!("{why}"));

        // Then
        assert_eq!(card.mode, CardMode::Local);
        assert!(
            card.path.starts_with("/Users/moi/.ash/worktrees/"),
            "{}",
            card.path
        );
        assert_eq!(card.other_path, CARD);
        assert!(!card.exists, "la fiche locale n'existe pas encore");
        assert_eq!(files.contents(CARD), Some(versioned));
        assert_eq!(files.writes(), 0);
    }
}
