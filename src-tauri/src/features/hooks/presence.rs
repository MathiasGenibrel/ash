//! Où en sont les entrées d'Ash dans un fichier — **la question qu'on pose avant d'écrire**.
//!
//! C'est le classement, et il n'y en a qu'un : [`install`](super::install) décide quoi faire
//! à partir de lui, et l'écran de réglages annonce le même verdict sans rien écrire (#16).
//! Deux classements séparés — un pour agir, un pour afficher — diraient forcément deux
//! choses différentes un jour, et ce jour-là l'écran promettrait une installation que la
//! feature refuserait.
//!
//! Aucune fonction de ce fichier n'écrit quoi que ce soit : [`presence`] est pure, et
//! [`inspect`] ne fait que lire. C'est ce qui permet à la fenêtre de montrer l'état d'une
//! ligne `hooks` — **et le diff de ce qu'Ash écrirait** — sans qu'un seul octet ne parte sur
//! le disque.
//!
//! **`ForeignHooks` a disparu, et c'est le cœur de l'amendement du 2026-08-12 d'ADR-0007.**
//! Un fichier qui portait déjà des hooks à lui n'est plus un refus : c'est un conflit, qui
//! se montre et se tranche. Le sens d'ADR-0007 est « jamais silencieux », pas « jamais ».

use super::merge::{self, Plan, Standing};
use super::ports::ConfigFiles;
use crate::features::agents::Instrumentation;
use crate::shared::text_diff as diff;

/// Ce que le fichier porte, face à ce qu'Ash y écrirait.
///
/// Les six cas sont **les six issues d'`install`**, dites avant de l'appeler : quatre
/// laissent écrire — dont deux après que l'utilisateur a regardé le diff — et deux font
/// refuser, parce qu'on ne devine pas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    /// Aucune entrée d'Ash. `others` dit combien de hooks le fichier porte quand même : zéro
    /// se lit « il n'y a rien », le reste se lit « il y a quelque chose que je n'ai pas
    /// mis ».
    Missing { others: usize, diff: String },
    /// Les entrées en place sont exactement celles qu'on écrirait. Installer ne toucherait
    /// à rien.
    Current { version: u32 },
    /// Des entrées d'Ash, mais pas celles-ci — écrites par une version antérieure. Une
    /// installation les réécrit sans rien demander.
    Superseded {
        installed: u32,
        available: u32,
        diff: String,
    },
    /// Quelqu'un a édité une entrée d'Ash, ou en a retiré une. **Ash montre le diff et
    /// laisse choisir** (spec §10).
    HandEdited { diff: String },
    /// Pas d'objet JSON où écrire, ou un chemin occupé par autre chose : Ash ne devine pas.
    NotAnObject,
    /// Le disque a dit non. Seul [`inspect`] peut produire ce cas.
    Unreadable { why: String },
}

impl Presence {
    /// Le diff de ce qu'Ash écrirait, sur le fichier tel qu'il est — vide quand il n'y a
    /// rien à écrire.
    pub fn diff(&self) -> Option<&str> {
        match self {
            Presence::Missing { diff, .. }
            | Presence::Superseded { diff, .. }
            | Presence::HandEdited { diff } => Some(diff),
            _ => None,
        }
    }
}

/// Le classement, à partir du texte du fichier. **Pure.**
pub fn presence(content: &str, instrumentation: &Instrumentation) -> Presence {
    // Un fichier vide est un fichier sans entrées : c'est le cas nominal d'une première
    // installation, et le distinguer de l'absence de fichier n'apporterait rien ici.
    if content.trim().is_empty() {
        return Presence::Missing {
            others: 0,
            diff: merge::fresh(instrumentation)
                .map(|document| diff::preview("", document.as_str()))
                .unwrap_or_default(),
        };
    }

    match merge::plan(content, instrumentation) {
        Plan::Unusable => Presence::NotAnObject,
        Plan::Current { version } => Presence::Current { version },
        Plan::Write {
            document,
            standing,
            others,
        } => {
            let diff = diff::preview(content, document.as_str());
            match standing {
                Standing::Absent => Presence::Missing { others, diff },
                Standing::Older { version } => Presence::Superseded {
                    installed: version,
                    available: instrumentation.version,
                    diff,
                },
                Standing::Changed => Presence::HandEdited { diff },
            }
        }
    }
}

/// Le même classement, en lisant le fichier — la question que pose l'écran de réglages.
pub fn inspect(files: &dyn ConfigFiles, instrumentation: &Instrumentation) -> Presence {
    match files.read(&instrumentation.file) {
        Ok(Some(content)) => presence(&content, instrumentation),
        // Pas de fichier du tout : rien n'y est écrit, et une installation le créerait.
        Ok(None) => presence("", instrumentation),
        Err(why) => Presence::Unreadable { why },
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::features::agents::{hook_mark, Adapter, ClaudeCodeAdapter};
    use crate::features::hooks::document::Document;
    use crate::features::hooks::fakes::FakeConfigFiles;
    use crate::features::hooks::install;

    fn instrumentation(config_dir: &str) -> Instrumentation {
        ClaudeCodeAdapter::new(PathBuf::from(
            "/Applications/Ash.app/Contents/MacOS/ash-event",
        ))
        .instrumentation(Path::new(config_dir))
        .unwrap_or_else(|| panic!("claude-code instrumente toujours"))
    }

    /// Ce que l'Ash **d'avant les sous-agents** avait écrit : les cinq mêmes entrées, sans
    /// `SubagentStop`, et marquées de la version qui les portait.
    ///
    /// C'est le seul moyen honnête de jouer le parcours de réinstallation du sixième hook :
    /// il ne se rejoue pas en changeant un nombre, il se rejoue en écrivant ce que la version
    /// précédente écrivait.
    fn five_hook_version(config_dir: &str) -> Instrumentation {
        let current = instrumentation(config_dir);
        let previous = current.version - 1;
        Instrumentation {
            entries: current
                .entries
                .iter()
                .filter(|entry| entry.path.last().map(String::as_str) != Some("SubagentStop"))
                .map(|entry| crate::features::agents::HookEntry {
                    path: entry.path.clone(),
                    item: entry
                        .item
                        .replace(&hook_mark(current.version), &hook_mark(previous)),
                })
                .collect(),
            version: previous,
            ..current
        }
    }

    #[test]
    fn given_a_file_instrumented_before_the_subagent_hook_when_ash_looks_at_it_then_it_offers_the_missing_entry_in_a_diff(
    ) {
        // Given — l'utilisateur avait installé les hooks avec un Ash d'avant les lignes
        // filles. Le sixième hook change la **forme** du bloc, donc la version : sans elle,
        // ses cinq entrées se liraient comme une édition à la main, et Ash refuserait de
        // toucher au fichier au lieu de le mettre à jour.
        let config_dir = "/home/someone/.claude";
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            "{\n  \"model\": \"opus\"\n}\n",
        );
        install(&files, &five_hook_version(config_dir)).unwrap_or_else(|why| panic!("{why}"));

        // When — l'Ash d'aujourd'hui regarde, sans rien écrire
        let seen = inspect(&files, &instrumentation(config_dir));

        // Then — c'est une version dépassée, et le diff montre l'entrée qui manque
        let current = instrumentation(config_dir).version;
        assert!(
            matches!(
                seen,
                Presence::Superseded { installed, available, .. }
                    if installed == current - 1 && available == current
            ),
            "{seen:?}"
        );
        let diff = seen.diff().unwrap_or_default();
        assert!(
            diff.contains("SubagentStop"),
            "le diff n'annonce pas le sixième hook :\n{diff}"
        );
        assert!(diff.contains("subagent-stop"), "{diff}");
    }

    #[test]
    fn given_a_configuration_file_ash_never_touched_when_it_is_inspected_then_the_absence_is_said_in_full_and_nothing_was_written(
    ) {
        // Given — l'écran de réglages pose la question à chaque affichage ; y répondre ne
        // doit rien écrire, sans quoi ouvrir la fenêtre modifierait le fichier de
        // l'utilisateur. Et l'absence doit se distinguer d'un refus : c'est la demande la
        // plus concrète de l'utilisateur — « on ne comprend pas »
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            "{\n  \"model\": \"opus\"\n}\n",
        );
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let found = inspect(&files, &instrumentation);

        // Then
        let Presence::Missing { others, diff } = found else {
            panic!("rien d'Ash n'est dans ce fichier : {found:?}");
        };
        assert_eq!(others, 0, "et rien de l'utilisateur non plus");
        assert!(
            diff.lines().any(|line| line.starts_with("+ ")),
            "le diff montre ce qu'Ash écrirait :\n{diff}"
        );
        assert_eq!(
            files.journal(),
            vec!["read /home/someone/.claude/settings.json"]
        );
    }

    #[test]
    fn given_a_file_that_already_carries_hooks_of_its_own_when_it_is_inspected_then_it_is_a_conflict_that_carries_the_merge_to_come(
    ) {
        // Given — c'est le fichier que les vrais utilisateurs ont : quelqu'un qui outille
        // déjà Claude Code. Il rendait la fonction centrale d'Ash inatteignable, et la seule
        // issue proposée était « déplace-les toi-même ». L'issue est maintenant de regarder
        // et de choisir
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            "{\n  \"hooks\": { \"PreToolUse\": [ { \"matcher\": \"Bash\",\n    \"hooks\": [ { \"type\": \"command\", \"command\": \"rtk hook claude\", \"timeout\": 5 } ] } ] }\n}\n",
        );

        // When
        let found = inspect(&files, &instrumentation("/home/someone/.claude"));

        // Then
        let Presence::Missing { others, diff } = found else {
            panic!("Ash n'a rien écrit dans ce fichier : {found:?}");
        };
        assert_eq!(others, 1, "le hook de l'utilisateur est compté, pas ignoré");
        assert!(
            diff.contains("ash-event"),
            "le diff montre ce qu'Ash ajouterait :\n{diff}"
        );
        assert!(
            diff.lines()
                .filter(|line| line.starts_with("- "))
                .all(|line| diff.contains(&format!("+{}", &line[1..]))
                    || diff.contains("rtk hook claude")),
            "ce que le diff retire doit se retrouver dans ce qu'il ajoute :\n{diff}"
        );
        assert!(
            diff.contains("rtk hook claude"),
            "le hook de l'utilisateur survit à ce qu'Ash écrirait :\n{diff}"
        );
    }

    #[test]
    fn given_entries_ash_just_installed_when_they_are_inspected_then_they_are_the_current_ones() {
        // Given — c'est l'état `installed · v1` de la ligne hooks, et il ne se déduit pas
        // d'un souvenir : Ash relit le fichier, parce que l'utilisateur a pu le vider entre
        // deux ouvertures de la fenêtre
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", "{}\n");
        let instrumentation = instrumentation("/home/someone/.claude");
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));

        // When
        let found = inspect(&files, &instrumentation);

        // Then
        assert_eq!(
            found,
            Presence::Current {
                version: instrumentation.version
            }
        );
    }

    #[test]
    fn given_entries_written_by_an_older_ash_when_they_are_inspected_then_it_names_both_versions() {
        // Given — l'état `v1 · v2 available` : l'écran doit pouvoir dire de quoi vers quoi,
        // sinon « mettre à jour » ne dit pas ce qu'il changerait
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", "{}\n");
        let older = instrumentation("/home/someone/.claude");
        install(&files, &older).unwrap_or_else(|why| panic!("{why}"));
        let newer = Instrumentation {
            entries: older
                .entries
                .iter()
                .map(|entry| crate::features::agents::HookEntry {
                    path: entry.path.clone(),
                    item: entry.item.replace("--tab", "--onglet").replace(
                        &crate::features::agents::hook_mark(older.version),
                        &crate::features::agents::hook_mark(older.version + 1),
                    ),
                })
                .collect(),
            version: older.version + 1,
            ..older.clone()
        };

        // When
        let found = inspect(&files, &newer);

        // Then
        let Presence::Superseded {
            installed,
            available,
            ..
        } = found
        else {
            panic!("un bloc périmé se réécrit : {found:?}");
        };
        assert_eq!((installed, available), (older.version, older.version + 1));
    }

    #[test]
    fn given_an_entry_someone_edited_by_hand_when_it_is_inspected_then_it_carries_the_diverging_lines(
    ) {
        // Given — refuser d'écrire sans montrer ce qui diffère ne laisse que le choix de
        // tout effacer (spec §10). Le diff est une partie du conflit, pas un agrément
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", "{}\n");
        let instrumentation = instrumentation("/home/someone/.claude");
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));
        let written = files
            .content_of(&instrumentation.file)
            .unwrap_or_default()
            .replace("waiting --tab", "mon-script --tab");
        files.replace(&instrumentation.file, &written);

        // When
        let found = inspect(&files, &instrumentation);

        // Then
        let Presence::HandEdited { diff } = found else {
            panic!("une main est passée : {found:?}");
        };
        assert!(
            diff.lines()
                .any(|line| line.starts_with('-') && line.contains("mon-script")),
            "le diff montre la ligne de l'utilisateur :\n{diff}"
        );
    }

    #[test]
    fn given_a_file_the_system_refuses_to_read_when_it_is_inspected_then_the_reason_travels_with_the_verdict(
    ) {
        // Given — un dossier verrouillé n'est pas « pas de hooks » : l'écran doit dire
        // qu'Ash n'a pas pu regarder, pas qu'il n'y a rien
        struct Locked;
        impl ConfigFiles for Locked {
            fn read(&self, _: &Path) -> Result<Option<String>, String> {
                Err("permission denied".to_owned())
            }
            fn exists(&self, _: &Path) -> bool {
                true
            }
            fn write(&self, _: &Path, _: &Document) -> Result<(), String> {
                Err("permission denied".to_owned())
            }
            fn copy(&self, _: &Path, _: &Path) -> Result<(), String> {
                Err("permission denied".to_owned())
            }
            fn remove(&self, _: &Path) -> Result<(), String> {
                Err("permission denied".to_owned())
            }
        }

        // When
        let found = inspect(&Locked, &instrumentation("/home/someone/.claude"));

        // Then
        assert_eq!(
            found,
            Presence::Unreadable {
                why: "permission denied".to_owned()
            }
        );
    }
}
