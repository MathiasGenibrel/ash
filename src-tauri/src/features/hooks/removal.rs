//! Ce qu'un retrait emporterait, **dit avant de l'emporter**.
//!
//! La spec §10 promet une « désinstallation en un geste qui rend le fichier à l'octet
//! près ». [`super::install::uninstall`] tient la seconde moitié de cette phrase ; ce
//! module tient la première — *un geste*, donc un geste que l'on comprend avant de le
//! poser. Sans lui, « retirer Ash de tous les fichiers » serait un bouton qui écrit dans
//! plusieurs fichiers de l'utilisateur sans jamais avoir dit lesquels.
//!
//! Il ne fait que **lire** : la même règle que partout ailleurs dans la feature — rien ne
//! s'écrit sans un geste explicite, et le geste vient après ce que ce module rend.

use std::path::PathBuf;

use super::document::is_an_empty_object;
use super::merge;
use super::ports::ConfigFiles;
use super::presence::presence;
use super::Presence;
use crate::features::agents::Instrumentation;
use crate::shared::text_diff as diff;

/// Ce qu'un retrait ferait à un fichier, sans l'avoir fait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withdrawal {
    pub file: PathBuf,
    /// Le nombre d'entrées marquées que le fichier porte — celles qui partiraient.
    pub entries: usize,
    /// Le fichier ne portait que ça : il s'en va avec elles (spec §10).
    pub deletes_the_file: bool,
    /// Une main est passée sur une entrée d'Ash.
    ///
    /// Le retrait les emporte quand même — elles portent son marqueur — et c'est justement
    /// pour ça que le cas se **signale** avant : « Ash ne réécrit pas silencieusement, il
    /// signale, propose le diff, et demande » (spec §10).
    pub hand_edited: bool,
    /// Le fichier tel qu'il est, face au fichier tel qu'Ash le laisserait.
    pub diff: String,
}

/// Ce que le retrait emporterait dans ce fichier, ou `None` s'il n'y a rien d'Ash.
///
/// `None` couvre les quatre façons de n'avoir rien à faire, et elles ne se distinguent pas
/// ici parce qu'elles ne se distinguent pas pour celui qui regarde : pas de fichier, un
/// fichier vide, un fichier qu'on ne sait pas lire, ou un fichier qui ne porte aucun
/// marqueur. Dans les quatre cas, **rien ne sera écrit**, et un plan de désinstallation qui
/// nommerait un fichier pour dire « je n'y toucherai pas » ferait craindre l'inverse.
pub fn foresee(files: &dyn ConfigFiles, instrumentation: &Instrumentation) -> Option<Withdrawal> {
    let content = files.read(&instrumentation.file).ok().flatten()?;
    let remaining = merge::removal(&content, instrumentation)?;
    Some(Withdrawal {
        file: instrumentation.file.clone(),
        entries: merge::ours(&content, instrumentation),
        deletes_the_file: is_an_empty_object(&remaining),
        hand_edited: matches!(
            presence(&content, instrumentation),
            Presence::HandEdited { .. }
        ),
        diff: diff::preview_removal(&content, remaining.as_str()),
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::features::agents::{Adapter, ClaudeCodeAdapter};
    use crate::features::hooks::fakes::FakeConfigFiles;
    use crate::features::hooks::{install, uninstall};

    fn instrumentation(config_dir: &str) -> Instrumentation {
        ClaudeCodeAdapter::new(PathBuf::from(
            "/Applications/Ash.app/Contents/MacOS/ash-event",
        ))
        .instrumentation(Path::new(config_dir))
        .unwrap_or_else(|| panic!("claude-code instrumente toujours"))
    }

    /// Un hook de l'utilisateur qui **ressemble** aux nôtres : il lance `ash-event`, sur le
    /// même événement, dans le même tableau. Il ne porte simplement pas le marqueur.
    const THEIRS_LOOKS_LIKE_OURS: &str = "{\n  \"hooks\": {\n    \"Stop\": [\n      { \"hooks\": [ { \"type\": \"command\", \"command\": \"ash-event --tab $ASH_TAB_ID done\" } ] }\n    ]\n  }\n}\n";

    #[test]
    fn given_a_hook_of_the_users_that_looks_like_ashs_when_the_removal_is_foreseen_then_it_is_not_counted_in(
    ) {
        // Given — le seul critère est le marqueur, et rien d'autre : ni le nom du binaire,
        // ni l'événement, ni la place dans le fichier. Un utilisateur qui a copié notre
        // ligne à la main a écrit *sa* ligne, et Ash n'y touche pas (ADR-0007)
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            THEIRS_LOOKS_LIKE_OURS,
        );
        let instrumentation = instrumentation("/home/someone/.claude");
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));
        let after_install = files.content_of(&instrumentation.file).unwrap_or_default();

        // When
        let foreseen = foresee(&files, &instrumentation);
        uninstall(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));

        // Then — les cinq entrées d'Ash s'annoncent, et la sienne ne s'annonce pas
        let foreseen = foreseen.unwrap_or_else(|| panic!("il y a des entrées d'ash à retirer"));
        assert_eq!(
            foreseen.entries,
            instrumentation.entries.len(),
            "seules les entrées marquées se comptent : {after_install}"
        );
        assert!(!foreseen.deletes_the_file);
        // Et le fichier redevient le sien, sa ligne comprise
        assert_eq!(
            files.content_of(&instrumentation.file).as_deref(),
            Some(THEIRS_LOOKS_LIKE_OURS)
        );
    }

    #[test]
    fn given_an_entry_someone_edited_by_hand_when_the_removal_is_foreseen_then_it_is_flagged_before_anything_is_written(
    ) {
        // Given — « si un bloc géré a été modifié à la main, Ash ne réécrit pas
        // silencieusement : il signale, propose le diff, et demande » (spec §10). Le retrait
        // emporte quand même l'entrée, marqueur oblige — donc il doit le dire avant
        let files = FakeConfigFiles::new();
        let instrumentation = instrumentation("/home/someone/.claude");
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));
        let edited = files
            .content_of(&instrumentation.file)
            .unwrap_or_default()
            .replace("waiting --tab", "mon-script --tab");
        files.replace(&instrumentation.file, &edited);

        // When
        let foreseen = foresee(&files, &instrumentation);

        // Then
        let foreseen = foreseen.unwrap_or_else(|| panic!("il y a des entrées d'ash à retirer"));
        assert!(foreseen.hand_edited);
        assert!(
            foreseen.diff.contains("mon-script"),
            "le diff montre ce que le retrait emporte : {}",
            foreseen.diff
        );
        // Rien n'a été écrit : prévoir est une lecture
        assert_eq!(
            files.content_of(&instrumentation.file).as_deref(),
            Some(edited.as_str())
        );
    }

    #[test]
    fn given_a_settings_file_ash_created_for_itself_when_the_removal_is_foreseen_then_it_says_the_file_goes_too(
    ) {
        // Given — un dossier de configuration tout neuf : le fichier est à Ash seul, et
        // « la désinstallation ne laisse rien » va jusqu'au fichier (spec §10). L'annoncer
        // change ce que l'utilisateur accepte
        let files = FakeConfigFiles::new();
        let instrumentation = instrumentation("/home/someone/.claude");
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));

        // When
        let foreseen = foresee(&files, &instrumentation);

        // Then
        let foreseen = foreseen.unwrap_or_else(|| panic!("il y a des entrées d'ash à retirer"));
        assert!(foreseen.deletes_the_file);
    }

    #[test]
    fn given_a_file_that_is_no_longer_json_when_the_removal_is_foreseen_then_it_announces_nothing_for_it(
    ) {
        // Given — le fichier a changé sous Ash depuis l'installation. On ne devine pas où
        // sont nos entrées dans un fichier qu'on ne sait plus lire, donc on n'annonce rien
        // et on n'écrira rien : le fichier de l'utilisateur reste intact
        let files =
            FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", "pas du json");
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let foreseen = foresee(&files, &instrumentation);

        // Then
        assert_eq!(foreseen, None);
        assert_eq!(
            files.journal(),
            ["read /home/someone/.claude/settings.json"]
        );
    }
}
