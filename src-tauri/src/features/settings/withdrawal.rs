//! « Retirer Ash de tous les fichiers » : ce que le geste annonce, et ce qu'il rapporte.
//!
//! Deux formes et non une seule, parce que ce sont deux moments et que les confondre
//! reviendrait à annoncer le passé. [`RemovalPlan`] se lit **avant** — quels fichiers,
//! combien d'entrées, ce que le retrait emporterait, et ce qu'une main a touché —, et rien
//! n'est écrit tant qu'il est à l'écran. [`RemovalReport`] se lit après, et ne dit que ce
//! qui a réellement eu lieu.
//!
//! **Les deux disent aussi ce que le geste ne fait pas** ([`KEPT`]), et c'est la partie de
//! la spec §10 qu'on oublierait le plus facilement : les `.bak` restent — la désinstallation
//! n'est pas un nettoyage, et les effacer retirerait le filet à l'instant où l'on saute — et
//! le reste de l'empreinte tient dans `~/.ash/`, dont la suppression appartient à
//! l'utilisateur.
//!
//! Ce module ne lit rien et n'écrit rien : il met en forme ce que le registre a rassemblé.
//! C'est ce qui rend ses phrases vérifiables sans un seul fichier.

use super::values::Command;
use crate::features::hooks::Withdrawal;

/// Ce que le retrait laisse derrière lui, dit avant **et** après.
///
/// Ce n'est pas une précaution de rédaction : les deux phrases sont la réponse aux deux
/// questions que se pose celui qui désinstalle — « ai-je perdu ma configuration ? » et
/// « en reste-t-il quelque part ? ».
pub const KEPT: [&str; 2] = [
    "the .bak copies stay where they are. removing ash is not a clean-up, \
     and they are what gives your file back if this went wrong.",
    "everything else ash keeps — config, state, attribution journal — is under ~/.ash. \
     deleting that folder takes the rest.",
];

/// Ce qu'un retrait complet ferait, fichier par fichier — **avant** de le faire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RemovalPlan {
    /// Les fichiers qui portent des entrées d'Ash, dans l'ordre des entrées déclarées.
    ///
    /// Un fichier où il n'y a rien à retirer **n'y figure pas** : nommer un fichier pour
    /// dire qu'on n'y touchera pas ferait craindre l'inverse.
    pub files: Vec<PlannedRemoval>,
    /// La phrase du bouton — `5 entries in 1 file`, `nothing to remove`.
    pub summary: String,
    /// Une main est passée sur une entrée d'Ash, quelque part.
    ///
    /// Remonté au plan entier parce que c'est ce qui change la nature du geste : il ne
    /// reprend plus seulement ce qu'Ash a écrit, il emporte ce que quelqu'un en a fait.
    pub hand_edited: bool,
    /// Ce que le geste ne touchera pas — voir [`KEPT`].
    pub kept: Vec<String>,
}

/// Ce qu'un retrait emporterait dans un fichier.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PlannedRemoval {
    pub file: String,
    /// Les entrées déclarées qui visent ce fichier — deux comptes peuvent le partager.
    pub commands: Vec<String>,
    /// Le nombre d'entrées marquées qui partiraient.
    pub entries: usize,
    /// Le fichier ne portait que ça : il s'en va avec elles (spec §10).
    pub deletes_the_file: bool,
    /// Une main est passée sur une entrée d'Ash dans ce fichier.
    pub hand_edited: bool,
    /// Le fichier tel qu'il est, face à ce qu'Ash laisserait.
    pub diff: String,
}

impl PlannedRemoval {
    /// Ce que `features::hooks` a vu dans ce fichier, mis dans la forme qui va à l'écran.
    ///
    /// La traduction est **ici et pas dans le registre** : c'est le seul module qui décide
    /// de ce que l'annonce contient, donc le seul à changer le jour où elle dit une chose de
    /// plus. Le registre, lui, sait quels fichiers sont visés — il n'a pas à savoir comment
    /// un chemin devient une ligne d'écran.
    pub fn foreseen(found: Withdrawal, command: &Command) -> Self {
        Self {
            file: found.file.display().to_string(),
            commands: vec![command.to_string()],
            entries: found.entries,
            deletes_the_file: found.deletes_the_file,
            hand_edited: found.hand_edited,
            diff: found.diff,
        }
    }

    /// Une seconde entrée déclarée vise le même fichier : les deux noms sur la même ligne.
    ///
    /// Le fichier ne s'annonce qu'une fois — l'annoncer deux fois promettrait deux fois les
    /// mêmes entrées, puis rapporterait un second passage qui n'a rien trouvé.
    pub fn also_aimed_by(&mut self, command: &Command) {
        self.commands.push(command.to_string());
    }
}

/// Ce que le retrait a réellement fait, fichier par fichier.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RemovalReport {
    pub files: Vec<RemovedFile>,
    /// `removed 5 entries from 1 file`, et ce qui a résisté.
    pub summary: String,
    pub kept: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RemovedFile {
    pub file: String,
    /// Ce que l'annonce promettait pour ce fichier.
    pub entries: usize,
    pub outcome: Outcome,
}

/// Ce qu'un fichier est devenu.
///
/// Quatre issues et non un booléen : « rien à retirer » n'est pas « retiré », et un fichier
/// que le disque a refusé n'est ni l'un ni l'autre. Les confondre ferait rapporter une
/// désinstallation complète là où un fichier porte encore le marqueur d'Ash.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Outcome {
    /// Les entrées sont parties, le fichier reste : il portait aussi celles de
    /// l'utilisateur.
    Removed,
    /// Les entrées sont parties, et le fichier avec elles : il ne portait que ça, donc Ash
    /// l'avait créé pour lui seul (spec §10).
    RemovedTheFile,
    /// Il n'y avait plus rien d'Ash à cet endroit — le fichier a changé depuis l'annonce,
    /// ou quelqu'un l'a devancé. **Rien n'a été écrit.**
    NothingLeft,
    /// Le geste a été refusé, et le fichier est resté tel quel.
    Refused { why: String },
}

/// La mise en forme de l'annonce.
pub fn plan(files: Vec<PlannedRemoval>) -> RemovalPlan {
    let entries: usize = files.iter().map(|planned| planned.entries).sum();
    RemovalPlan {
        summary: if files.is_empty() {
            "nothing to remove — no file carries ash's marker".to_owned()
        } else {
            format!(
                "{} in {}",
                count(entries, "entry", "entries"),
                places(&files)
            )
        },
        hand_edited: files.iter().any(|planned| planned.hand_edited),
        files,
        kept: KEPT.iter().map(|line| (*line).to_owned()).collect(),
    }
}

/// La mise en forme du compte rendu.
///
/// Elle **compte ce qui a eu lieu**, pas ce qui avait été annoncé : c'est la même règle que
/// partout ailleurs, un écran ne dit jamais d'un fichier plus que ce que le disque en a dit.
pub fn report(files: Vec<RemovedFile>) -> RemovalReport {
    let removed: Vec<&RemovedFile> = files
        .iter()
        .filter(|done| matches!(done.outcome, Outcome::Removed | Outcome::RemovedTheFile))
        .collect();
    let entries: usize = removed.iter().map(|done| done.entries).sum();
    let refused = files
        .iter()
        .filter(|done| matches!(done.outcome, Outcome::Refused { .. }))
        .count();

    let mut summary = if removed.is_empty() {
        "nothing was removed".to_owned()
    } else {
        format!(
            "removed {} from {}",
            count(entries, "entry", "entries"),
            count(removed.len(), "file", "files")
        )
    };
    if refused > 0 {
        summary.push_str(&format!(
            " · {} left untouched",
            count(refused, "file", "files")
        ));
    }

    RemovalReport {
        files,
        summary,
        kept: KEPT.iter().map(|line| (*line).to_owned()).collect(),
    }
}

/// `1 file` ou `2 files` — l'accord, écrit une fois.
fn count(many: usize, one: &str, several: &str) -> String {
    format!("{many} {}", if many == 1 { one } else { several })
}

fn places(files: &[PlannedRemoval]) -> String {
    match files {
        // Un seul fichier se nomme : c'est la question qu'on se pose en premier, et la
        // réponse tient sur la ligne.
        [only] => only.file.clone(),
        _ => count(files.len(), "file", "files"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : un fichier à retirer, dont on ne dit que ce qui compte.
    fn planned(file: &str, entries: usize) -> PlannedRemoval {
        PlannedRemoval {
            file: file.to_owned(),
            commands: vec!["claude".to_owned()],
            entries,
            deletes_the_file: false,
            hand_edited: false,
            diff: String::new(),
        }
    }

    fn done(file: &str, entries: usize, outcome: Outcome) -> RemovedFile {
        RemovedFile {
            file: file.to_owned(),
            entries,
            outcome,
        }
    }

    #[test]
    fn given_no_file_carrying_ashs_marker_when_the_removal_is_announced_then_it_says_there_is_nothing_to_do(
    ) {
        // Given — le cas de quelqu'un qui n'a jamais installé les hooks. Un bouton qui
        // annonce « je vais retirer » sans rien à retirer ferait douter de tout le reste
        let nothing: Vec<PlannedRemoval> = Vec::new();

        // When
        let announced = plan(nothing);

        // Then
        assert_eq!(
            announced.summary,
            "nothing to remove — no file carries ash's marker"
        );
        assert!(!announced.hand_edited);
    }

    #[test]
    fn given_entries_in_several_files_when_the_removal_is_announced_then_it_counts_both_before_anything_is_written(
    ) {
        // Given — « dire ce qu'il va faire avant de le faire : quels fichiers, quelles
        // entrées » (spec §10). Deux comptes Claude, c'est deux fichiers
        let files = vec![
            planned("/home/.claude/settings.json", 5),
            planned("/home/.claude-perso/settings.json", 5),
        ];

        // When
        let announced = plan(files);

        // Then
        assert_eq!(announced.summary, "10 entries in 2 files");
        assert_eq!(announced.files.len(), 2);
    }

    #[test]
    fn given_a_single_file_when_the_removal_is_announced_then_the_summary_names_it() {
        // Given — un seul fichier : « 1 file » n'apprend rien, son chemin apprend tout
        let files = vec![planned("/home/.claude/settings.json", 5)];

        // When
        let announced = plan(files);

        // Then
        assert_eq!(
            announced.summary,
            "5 entries in /home/.claude/settings.json"
        );
    }

    #[test]
    fn given_one_file_someone_edited_by_hand_when_the_removal_is_announced_then_the_whole_plan_says_so(
    ) {
        // Given — spec §10 : Ash ne réécrit pas silencieusement, il signale. Le signal doit
        // remonter au geste entier, sinon il se lit sous le pli d'un seul fichier
        let files = vec![
            planned("/home/.claude/settings.json", 5),
            PlannedRemoval {
                hand_edited: true,
                ..planned("/home/.claude-perso/settings.json", 5)
            },
        ];

        // When
        let announced = plan(files);

        // Then
        assert!(announced.hand_edited);
    }

    #[test]
    fn given_a_file_that_refused_the_write_when_the_removal_is_reported_then_it_is_not_counted_as_removed(
    ) {
        // Given — un compte rendu qui arrondit est pire que pas de compte rendu : celui-là
        // ferait croire qu'Ash a quitté un fichier qui porte encore son marqueur
        let files = vec![
            done("/home/.claude/settings.json", 5, Outcome::Removed),
            done(
                "/home/.claude-perso/settings.json",
                5,
                Outcome::Refused {
                    why: "read-only file system".to_owned(),
                },
            ),
        ];

        // When
        let told = report(files);

        // Then
        assert_eq!(
            told.summary,
            "removed 5 entries from 1 file · 1 file left untouched"
        );
    }

    #[test]
    fn given_a_removal_that_took_place_when_it_is_reported_then_it_still_says_the_backups_stay() {
        // Given — « les .bak sont conservés » est une promesse du produit, et c'est après
        // coup qu'on a besoin de l'entendre : la question qui suit une désinstallation est
        // « qu'est-ce qui reste ? »
        let files = vec![done(
            "/home/.claude/settings.json",
            5,
            Outcome::RemovedTheFile,
        )];

        // When
        let told = report(files);

        // Then
        assert_eq!(told.kept, KEPT.to_vec());
    }
}
