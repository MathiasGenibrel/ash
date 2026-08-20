//! Poser le journal dans la fiche — et, le plus souvent, ne pas le poser.
//!
//! L'ordre des gestes est la règle, et il ne se réarrange pas. C'est celui de
//! `hooks::install`, parce que c'est le même régime (ADR-0013 : « même régime que pour les
//! `settings.json` ») :
//!
//! 1. **lire** le fichier tel qu'il est ;
//! 2. **classer** — rien à faire, écrire, ajouter le bloc, créer la fiche, ou refuser ;
//! 3. **sauvegarder**, si et seulement si on va écrire ;
//! 4. **écrire**, d'un seul remplacement de document.
//!
//! Rien ne se décide après l'étape 3 : une sauvegarde prise « au passage », pendant qu'on
//! écrit, ne sauvegarde plus rien.
//!
//! **Le refus n'est pas un échec, c'est une réponse** — et elle porte le diff. La spec §10
//! l'exige dans ces termes : « Ash ne réécrit pas silencieusement — il signale, propose le
//! diff, et demande. » Ici, « demander » s'arrête au diff : la fiche est un document que
//! l'utilisateur et les agents tiennent, et un bouton « écrase quand même » sur un bloc parti
//! en conflit serait précisément la résolution silencieuse qu'ADR-0013 interdit.

use std::path::{Path, PathBuf};

use crate::shared::text_diff;

use super::block::{self, Zone};
use super::document::CardDocument;
use super::error::CardError;
use super::log;
use super::ports::CardFiles;

/// L'état de la zone d'Ash dans une fiche — ce que l'écran affiche, et ce sur quoi
/// l'écriture se décide. **Une seule lecture pour les deux**, comme `hooks::presence`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum LogState {
    /// Le bloc porte déjà exactement ce qu'Ash y écrirait. **Rien à toucher.**
    Current,
    /// Le bloc est celui d'Ash, et son contenu a changé.
    Stale,
    /// La fiche n'existe pas encore.
    NoCard,
    /// La fiche existe, sans zone pour le journal.
    NoBlock,
    /// Le bloc porte autre chose que ce qu'Ash y laisse.
    EditedByHand,
    /// Le bloc porte des marqueurs de conflit git.
    Conflicted,
    /// Une ouverture sans fermeture : Ash ne sait pas où sa zone s'arrête.
    Unterminated,
    /// Deux zones dans la même fiche.
    Duplicated,
}

impl LogState {
    /// Ash a-t-il le droit d'écrire dans cet état ?
    pub fn lets_ash_write(self) -> bool {
        matches!(self, LogState::Stale | LogState::NoCard | LogState::NoBlock)
    }
}

/// Ce qu'Ash ferait de cette fiche, dit **avant** de le faire.
///
/// C'est le plan qui porte les octets, et l'écriture ne fait que les poser : `diff` est
/// l'aperçu de `document`, et rien d'autre ne se décide entre les deux. Un plan qui ne
/// rendrait que l'état laisserait l'écriture reclasser le fichier pour son propre compte —
/// deux lectures, deux classements, et le jour où l'un des deux dérive, **le diff montré à
/// l'utilisateur n'est plus celui qui s'applique**. C'est exactement ce que la spec §10
/// interdit. Même forme que `hooks::merge::Plan::Write`, qui porte déjà son `Document`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub state: LogState,
    /// La table telle qu'elle irait dans le bloc.
    pub table: String,
    /// Le fichier tel qu'il est, face au fichier tel qu'Ash le laisserait. Vide quand il n'y
    /// a rien à changer.
    pub diff: String,
    /// Ce qui se passe, en une phrase — y compris quand rien ne se passe.
    pub note: String,
    /// La fiche telle qu'Ash la laisserait, **aux octets près**. `None` quand il n'y a rien
    /// à écrire : le bloc est déjà à jour, ou Ash refuse.
    pub(super) document: Option<CardDocument>,
}

/// Ce qu'une écriture a fait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogWrite {
    Written {
        path: PathBuf,
        /// La sauvegarde, si c'est cette écriture qui l'a créée.
        backup: Option<PathBuf>,
        /// La fiche n'existait pas : Ash l'a créée.
        created_the_card: bool,
    },
    /// Le bloc dit déjà ce qu'il doit dire. **Rien n'a été touché.**
    AlreadyCurrent { path: PathBuf },
    /// Ash n'écrit pas, et dit pourquoi.
    Refused {
        path: PathBuf,
        state: LogState,
        note: String,
        diff: String,
    },
}

/// Ce qu'Ash ferait — sans rien écrire.
///
/// **Une lecture, un classement.** C'est ici, et nulle part ailleurs, que le fichier est lu
/// et que sa zone est classée ; [`write_log`] ne fait que poser ce que ce plan porte.
pub fn plan(files: &dyn CardFiles, path: &Path, table: &str) -> Result<Plan, CardError> {
    Ok(plan_of(&read(files, path)?.unwrap_or_default(), table))
}

/// Le même plan, pour une fiche **déjà lue**.
///
/// Tout ce qui se décide se décide ici, et sans toucher au disque : l'écran, qui a besoin de
/// la source *et* de l'état du bloc, les obtient donc d'une seule lecture — deux lectures
/// pourraient décrire deux fichiers différents, et le rendu ne dirait plus la même chose que
/// le diff. C'est aussi ce qui rend la décision interrogeable sans aucun double.
///
/// Une fiche vide et une fiche absente sont le même cas : il n'y a rien de l'utilisateur.
pub(super) fn plan_of(source: &str, table: &str) -> Plan {
    let content = Some(source).filter(|content| !content.trim().is_empty());
    let table = table.to_owned();

    let (state, note, document) = match content {
        None => (
            LogState::NoCard,
            "there is no card yet — ash would write one, with its log block.".to_owned(),
            Some(CardDocument::fresh(scaffold(&table))),
        ),
        Some(content) => classify(content, &table),
    };

    Plan {
        state,
        diff: document
            .as_ref()
            .map(|would| text_diff::preview(content.unwrap_or_default(), would.as_str()))
            .unwrap_or_default(),
        table,
        note,
        document,
    }
}

/// Dans quel état est la zone d'Ash, ce qu'on en dit, et la fiche telle qu'il la laisserait.
///
/// Les huit branches sont ici en entier, et c'est voulu : lire de haut en bas doit suffire à
/// savoir dans quels cas Ash écrit et dans quels cas il s'abstient. Trois seulement portent
/// une écriture possible — [`LogState::lets_ash_write`] en est la seule lecture ; les autres
/// composent quand même le document, parce que **c'est lui que le diff du refus montre**
/// (spec §10 : « il signale, propose le diff, et demande »).
fn classify(content: &str, table: &str) -> (LogState, String, Option<CardDocument>) {
    match block::locate(content) {
        Zone::Absent => (
            LogState::NoBlock,
            "the card has no ash:log block — ash would append one at the end, and touch \
             nothing else."
                .to_owned(),
            Some(CardDocument::with_block(content, &block::block(table))),
        ),
        // La reconnaissance passe par la **grammaire** de la table, et sa limite est écrite
        // en tête de [`log`] : une ligne écrite à la main *dans* cette grammaire passe pour
        // une ligne d'Ash, et sera remplacée. Ce qui reste garanti quoi qu'il arrive : rien
        // hors du bloc, une sauvegarde avant, un diff montré.
        Zone::Present { inner, body } if !log::is_ours(&body) => (
            LogState::EditedByHand,
            "the ash:log block carries something ash did not write. it is left alone — \
             ash never rewrites a hand-edited block."
                .to_owned(),
            Some(CardDocument::with_log(content, inner, table)),
        ),
        Zone::Present { body, .. } if body == table => (
            LogState::Current,
            "the log is up to date. nothing to write.".to_owned(),
            None,
        ),
        Zone::Present { inner, .. } => (
            LogState::Stale,
            "ash would refresh the ash:log block, and touch nothing else.".to_owned(),
            Some(CardDocument::with_log(content, inner, table)),
        ),
        // Les trois derniers ne composent rien : Ash ne sait pas où est sa zone, ou saurait
        // mais choisirait pour l'utilisateur. Il n'y a donc pas de diff à montrer — la
        // phrase dit ce qu'il y a à faire, et c'est l'utilisateur qui le fait.
        Zone::Conflicted { .. } => (
            LogState::Conflicted,
            "the ash:log block is in conflict. ash never resolves it — settle it like any \
             other conflict, then come back."
                .to_owned(),
            None,
        ),
        Zone::Unterminated => (
            LogState::Unterminated,
            "the ash:log block is opened and never closed. ash does not know where its \
             zone ends, so it writes nothing."
                .to_owned(),
            None,
        ),
        Zone::Duplicated => (
            LogState::Duplicated,
            "the card carries two ash:log blocks. picking one would be resolving a merge, \
             and ash does not do that."
                .to_owned(),
            None,
        ),
    }
}

/// Écrit le journal dans la fiche, ou refuse en le disant.
///
/// **La sauvegarde précède l'écriture**, et n'est jamais écrasée : c'est la copie de la
/// fiche d'avant Ash, et une seconde écriture ne doit pas la remplacer par la fiche d'après.
/// Même règle que `hooks::install`, et pour la même raison — le filet ne sert que s'il porte
/// l'état auquel on veut revenir.
///
/// **Elle ne décide rien** : elle demande le plan, refuse ou pose ses octets. Les octets
/// écrits sont donc, par construction, ceux dont le diff a été montré.
pub fn write_log(files: &dyn CardFiles, path: &Path, table: &str) -> Result<LogWrite, CardError> {
    let plan = plan(files, path, table)?;
    let path = path.to_owned();

    if plan.state == LogState::Current {
        return Ok(LogWrite::AlreadyCurrent { path });
    }
    // La même lecture que celle dont l'écran allume son bouton — il n'y en a qu'une.
    let Some(document) = plan.document.filter(|_| plan.state.lets_ash_write()) else {
        return Ok(LogWrite::Refused {
            path,
            state: plan.state,
            note: plan.note,
            diff: plan.diff,
        });
    };

    // Une fiche absente ou vide n'a rien de l'utilisateur à préserver : rien à sauvegarder,
    // et c'est le seul cas où Ash compose autre chose que sa zone.
    let from_scratch = plan.state == LogState::NoCard;
    // Avant l'écriture, sinon le fichier existe déjà quand on le demande.
    let created_the_card = from_scratch && !files.exists(&path);
    let backup = if from_scratch {
        None
    } else {
        back_up(files, &path)?
    };
    write(files, &path, &document)?;
    Ok(LogWrite::Written {
        path,
        backup,
        created_the_card,
    })
}

/// La fiche qu'Ash pose quand il n'y en a pas — du markdown standard, et rien d'autre.
///
/// Elle est **volontairement pauvre** : le front matter d'ADR-0013 avec les champs qu'Ash
/// ne peut pas remplir laissés vides, les titres du design, et le bloc du journal. Le
/// contenu appartient à l'utilisateur et aux agents ; Ash pose la structure, pas l'intention.
fn scaffold(table: &str) -> String {
    format!(
        "---\ntype:\nissue:\nbranch:\nbase:\nstatus:\n---\n\n# why\n\n## tasks\n\n- [ ] \n\n## decided\n\n## out of scope\n\n{}",
        block::block(table)
    )
}

fn back_up(files: &dyn CardFiles, path: &Path) -> Result<Option<PathBuf>, CardError> {
    let backup = backup_of(path);
    if !files.exists(path) || files.exists(&backup) {
        return Ok(None);
    }
    files.copy(path, &backup).map_err(|why| CardError::Io {
        path: backup.clone(),
        why,
    })?;
    Ok(Some(backup))
}

fn backup_of(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "worktree.md".to_owned());
    path.with_file_name(format!("{name}.bak"))
}

fn read(files: &dyn CardFiles, path: &Path) -> Result<Option<String>, CardError> {
    files.read(path).map_err(|why| CardError::Io {
        path: path.to_owned(),
        why,
    })
}

fn write(files: &dyn CardFiles, path: &Path, document: &CardDocument) -> Result<(), CardError> {
    files.write(path, document).map_err(|why| CardError::Io {
        path: path.to_owned(),
        why,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::card::fakes::{CardBuilder, MemoryCardFiles};

    const CARD: &str = "/dev/ash/.ash/worktree.md";
    const LOG: &str =
        "| agent | work | when |\n|---|---|---|\n| claude | 4 commits · 15m22s | now |\n";

    #[test]
    fn given_a_card_the_user_wrote_around_the_block_when_the_log_is_written_then_nothing_else_moves(
    ) {
        // Given — la garantie centrale d'ADR-0013, et le seul test qui la prouve de bout en
        // bout : la fiche porte du front matter, des cases à cocher, un tableau, une clôture
        // `mermaid`, **et une citation des marqueurs d'Ash**.
        let before = CardBuilder::new().quoting_the_format().logging("").build();
        let files = MemoryCardFiles::new().file(CARD, &before);

        // When
        let written = write_log(&files, Path::new(CARD), LOG);

        // Then
        assert!(
            matches!(written, Ok(LogWrite::Written { .. })),
            "{written:?}"
        );
        let after = files.contents(CARD).unwrap_or_default();
        // La même fiche, dont **seul** le bloc a changé — front matter, cases, tableau,
        // clôture `mermaid` et citation des marqueurs compris.
        assert_eq!(
            after,
            CardBuilder::new().quoting_the_format().logging(LOG).build()
        );
        assert!(after.contains("```markdown\n<!-- ash:log -->\n| agent |\n"));
    }

    #[test]
    fn given_a_block_someone_annotated_when_the_log_is_written_then_ash_refuses_and_shows_what_differs(
    ) {
        // Given — spec §10 : « si un bloc géré a été modifié à la main, Ash ne réécrit pas
        // silencieusement — il signale, propose le diff, et demande. »
        let annotated = CardBuilder::new()
            .logging("| agent | work | when |\n|---|---|---|\n| claude | 4 commits · 15m22s | now |\n\nnote : gardé pour la PR.\n")
            .build();
        let files = MemoryCardFiles::new().file(CARD, &annotated);

        // When
        let written = write_log(&files, Path::new(CARD), LOG);

        // Then — le fichier est intact, et le refus dit ce qui changerait
        let Ok(LogWrite::Refused {
            state, note, diff, ..
        }) = written
        else {
            panic!("Ash a écrit sur un bloc édité à la main : {written:?}");
        };
        assert_eq!(state, LogState::EditedByHand);
        assert!(note.contains("hand-edited"), "{note}");
        assert!(diff.contains("- note : gardé pour la PR."), "{diff}");
        assert_eq!(files.contents(CARD), Some(annotated));
    }

    #[test]
    fn given_a_block_left_in_conflict_by_a_merge_when_the_log_is_written_then_ash_never_settles_it()
    {
        // Given — le cas nommé par ADR-0013 : « Ash ne résout jamais ce conflit tout seul ;
        // il le traite comme n'importe quel autre. »
        let conflicted = CardBuilder::new()
            .logging("<<<<<<< HEAD\n| claude | 4 commits · 15m22s | now |\n=======\n| codex | 1 commit · 2m00s | 3m ago |\n>>>>>>> autre\n")
            .build();
        let files = MemoryCardFiles::new().file(CARD, &conflicted);

        // When
        let written = write_log(&files, Path::new(CARD), LOG);

        // Then — rien n'est écrit, et aucune sauvegarde n'a été prise : on n'a pas commencé
        let Ok(LogWrite::Refused { state, note, .. }) = written else {
            panic!("Ash a touché à un conflit : {written:?}");
        };
        assert_eq!(state, LogState::Conflicted);
        assert!(note.contains("never resolves"), "{note}");
        assert_eq!(files.contents(CARD), Some(conflicted));
        assert!(files.contents(&format!("{CARD}.bak")).is_none());
    }

    #[test]
    fn given_a_block_whose_closing_marker_is_gone_when_the_log_is_written_then_ash_writes_nothing_at_all(
    ) {
        // Given — Ash ne sait plus où sa zone s'arrête. Écrire « jusqu'à la fin » effacerait
        // tout ce que l'utilisateur a mis dessous ; le refus doit donc aller jusqu'au bout,
        // sauvegarde comprise — on n'a pas commencé.
        let truncated = CardBuilder::new()
            .logging(LOG)
            .build()
            .replace("<!-- /ash:log -->", "");
        let files = MemoryCardFiles::new().file(CARD, &truncated);

        // When
        let written = write_log(&files, Path::new(CARD), LOG);

        // Then
        let Ok(LogWrite::Refused { state, note, .. }) = written else {
            panic!("Ash a écrit dans une zone dont il ignore la fin : {written:?}");
        };
        assert_eq!(state, LogState::Unterminated);
        assert!(note.contains("never closed"), "{note}");
        assert_eq!(files.contents(CARD), Some(truncated));
        assert_eq!(files.written_paths(), Vec::<String>::new());
    }

    #[test]
    fn given_two_blocks_left_by_a_merge_when_the_log_is_written_then_ash_refuses_rather_than_picking_one(
    ) {
        // Given — la fusion recollée à la main. Écrire dans « le » bloc reviendrait à choisir
        // lequel garder, donc à résoudre le conflit à la place de l'utilisateur (ADR-0013).
        let two = format!(
            "{}{}",
            CardBuilder::new().logging(LOG).build(),
            CardBuilder::new().logging(LOG).build()
        );
        let files = MemoryCardFiles::new().file(CARD, &two);

        // When
        let written = write_log(&files, Path::new(CARD), LOG);

        // Then
        let Ok(LogWrite::Refused { state, note, .. }) = written else {
            panic!("Ash a choisi un bloc : {written:?}");
        };
        assert_eq!(state, LogState::Duplicated);
        assert!(note.contains("resolving a merge"), "{note}");
        assert_eq!(files.contents(CARD), Some(two));
        assert_eq!(files.written_paths(), Vec::<String>::new());
    }

    #[test]
    fn given_a_card_about_to_be_written_when_the_log_lands_then_a_backup_of_the_previous_one_exists(
    ) {
        // Given — « toute écriture est précédée d'une copie » (spec §10, ADR-0013). C'est ce
        // qui rend le geste réversible sans git, y compris en mode local.
        let before = CardBuilder::new().logging("").build();
        let files = MemoryCardFiles::new().file(CARD, &before);

        // When
        let _ = write_log(&files, Path::new(CARD), LOG);

        // Then
        assert_eq!(files.contents(&format!("{CARD}.bak")), Some(before));
    }

    #[test]
    fn given_a_backup_taken_before_ash_ever_wrote_when_the_log_is_written_again_then_it_is_not_replaced(
    ) {
        // Given — la deuxième écriture. Écraser le `.bak` remplacerait la fiche d'avant Ash
        // par la fiche d'après : le filet porterait l'état auquel personne ne veut revenir.
        let files = MemoryCardFiles::new()
            .file(CARD, &CardBuilder::new().logging(LOG).build())
            .file(&format!("{CARD}.bak"), "la fiche d'avant ash\n");

        // When
        let _ = write_log(
            &files,
            Path::new(CARD),
            "| agent | work | when |\n|---|---|---|\n| claude | 9 commits · 1h02m | 3m ago |\n",
        );

        // Then
        assert_eq!(
            files.contents(&format!("{CARD}.bak")),
            Some("la fiche d'avant ash\n".to_owned())
        );
    }

    #[test]
    fn given_a_card_written_by_hand_without_a_block_when_the_log_is_written_then_the_block_is_appended_below(
    ) {
        // Given — le cas courant : la fiche est rédigée par l'utilisateur et les agents, et
        // n'a jamais eu de zone. Refuser ici ne journaliserait que les fiches d'Ash.
        let by_hand = CardBuilder::new().without_a_block().build();
        let files = MemoryCardFiles::new().file(CARD, &by_hand);

        // When
        let written = write_log(&files, Path::new(CARD), LOG);

        // Then — rien n'a été perdu : le fichier d'origine est un préfixe du nouveau
        assert!(
            matches!(written, Ok(LogWrite::Written { .. })),
            "{written:?}"
        );
        let after = files.contents(CARD).unwrap_or_default();
        assert!(after.starts_with(&by_hand), "{after}");
        assert!(after.ends_with(&block::block(LOG)), "{after}");
    }

    #[test]
    fn given_no_card_at_all_when_the_log_is_written_then_ash_writes_one_in_plain_markdown() {
        // Given — une branche neuve. La fiche qu'Ash pose ne doit porter aucune syntaxe qui
        // lui soit propre (ADR-0013) : elle s'ouvre dans n'importe quel éditeur.
        let files = MemoryCardFiles::new();

        // When
        let written = write_log(&files, Path::new(CARD), LOG);

        // Then
        assert!(
            matches!(
                written,
                Ok(LogWrite::Written {
                    created_the_card: true,
                    backup: None,
                    ..
                })
            ),
            "{written:?}"
        );
        let card = files.contents(CARD).unwrap_or_default();
        assert!(card.starts_with("---\ntype:\n"), "{card}");
        assert!(card.contains("- [ ] "), "{card}");
        assert!(card.contains(LOG), "{card}");
    }

    #[test]
    fn given_a_block_that_already_says_it_when_the_log_is_written_then_nothing_is_touched() {
        // Given — le cas de toutes les ouvertures du panneau après la première. Réécrire un
        // fichier identique salirait `git status` et réveillerait les surveillances de
        // l'utilisateur pour rien.
        let files = MemoryCardFiles::new().file(CARD, &CardBuilder::new().logging(LOG).build());

        // When
        let written = write_log(&files, Path::new(CARD), LOG);

        // Then
        assert!(
            matches!(written, Ok(LogWrite::AlreadyCurrent { .. })),
            "{written:?}"
        );
        assert_eq!(files.writes(), 0);
    }

    #[test]
    fn given_a_card_whose_front_matter_is_broken_when_the_log_is_written_then_ash_writes_anyway_without_touching_it(
    ) {
        // Given — un fichier de l'utilisateur peut être n'importe quoi. Le front matter est
        // à lui ; Ash ne le lit pas, ne le répare pas, et ne s'arrête pas dessus.
        let broken =
            "---\ntype: [feat\n  issue: : :\n\n# pourquoi\n\n<!-- ash:log -->\n<!-- /ash:log -->\n";
        let files = MemoryCardFiles::new().file(CARD, broken);

        // When
        let written = write_log(&files, Path::new(CARD), LOG);

        // Then
        assert!(
            matches!(written, Ok(LogWrite::Written { .. })),
            "{written:?}"
        );
        let after = files.contents(CARD).unwrap_or_default();
        assert!(
            after.starts_with("---\ntype: [feat\n  issue: : :\n"),
            "{after}"
        );
    }

    #[test]
    fn given_any_card_when_ash_writes_its_log_then_it_never_touches_the_gitignore() {
        // Given — l'interdiction explicite d'ADR-0013 : « Ash ne doit ni forcer, ni imposer
        // un `.gitignore` ». La preuve est ici plutôt que dans une relecture : le double
        // enregistre **tout** ce qui est écrit.
        let files = MemoryCardFiles::new()
            .file("/dev/ash/.gitignore", "target/\n")
            .file(CARD, &CardBuilder::new().logging("").build());

        // When
        let _ = write_log(&files, Path::new(CARD), LOG);

        // Then
        assert_eq!(
            files.written_paths(),
            vec![format!("{CARD}.bak"), CARD.to_owned()]
        );
    }
}
