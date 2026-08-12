//! Où en est le bloc dans un fichier — **la question qu'on pose avant d'écrire**.
//!
//! C'est le classement, et il n'y en a qu'un : [`install`](super::install) décide quoi
//! faire à partir de lui, et l'écran de réglages annonce le même verdict sans rien écrire
//! (#16). Deux classements séparés — un pour agir, un pour afficher — diraient forcément
//! deux choses différentes un jour, et ce jour-là l'écran promettrait une installation que
//! la feature refuserait.
//!
//! Aucune fonction de ce fichier n'écrit quoi que ce soit : [`presence`] est pure, et
//! [`inspect`] ne fait que lire. C'est ce qui permet à la fenêtre de montrer les cinq états
//! d'une ligne `hooks` sans qu'un seul octet ne parte sur le disque.

use std::ops::Range;

use super::block::{self, Located};
use super::diff;
use super::ports::ConfigFiles;
use crate::features::agents::Instrumentation;

/// Ce que le fichier porte, face à ce qu'Ash y écrirait.
///
/// Les six cas sont **les six issues d'`install`**, dites avant de l'appeler : trois
/// laissent écrire (rien, un bloc périmé), trois font refuser (une main est passée, des
/// hooks qui ne sont pas les nôtres, un fichier qu'on ne sait pas lire).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    /// Aucun bloc d'Ash : il n'est jamais passé par ici. Une installation l'y poserait.
    Missing,
    /// Le bloc en place est exactement celui qu'on écrirait. Installer ne toucherait à rien.
    Current { version: u32 },
    /// Un bloc d'Ash, mais pas celui-ci — écrit par une version antérieure, ou dont le
    /// contenu a changé de forme depuis. Une installation le réécrit sans rien demander.
    Superseded { installed: u32, available: u32 },
    /// Quelqu'un a édité le bloc. **Ash n'écrit pas**, et porte le diff (spec §10).
    HandEdited { diff: String },
    /// Le fichier a déjà une clé `hooks` qui n'est pas la nôtre.
    ForeignHooks,
    /// Pas d'accolade ouvrante : Ash ne saurait pas où poser son bloc.
    NotAnObject,
    /// Le disque a dit non. Seul [`inspect`] peut produire ce cas.
    Unreadable { why: String },
}

/// Le classement, à partir du texte du fichier. **Pure.**
pub fn presence(content: &str, instrumentation: &Instrumentation) -> Presence {
    // Un fichier vide est un fichier sans bloc : c'est le cas nominal d'une première
    // installation, et le distinguer de l'absence de fichier n'apporterait rien ici.
    if content.trim().is_empty() {
        return Presence::Missing;
    }

    match block::locate(content) {
        // Des marqueurs qu'on ne sait plus lire ne sortent pas de `render` : personne
        // d'autre qu'un humain ne produit ça.
        Located::Damaged => Presence::HandEdited {
            diff: diff::compare(&instrumentation.block, content),
        },

        Located::Present(block) if !block.intact => Presence::HandEdited {
            diff: diff::compare(&instrumentation.block, &block.payload),
        },

        Located::Present(block) => match foreign(content, Some(block.span.clone())) {
            Some(refusal) => refusal,
            // Un bloc intact, de la version courante, au contenu identique : c'est le
            // démarrage ordinaire d'Ash, et le fichier de l'utilisateur ne doit pas bouger.
            None if block.version == instrumentation.version
                && block.payload == instrumentation.block =>
            {
                Presence::Current {
                    version: block.version,
                }
            }
            None => Presence::Superseded {
                installed: block.version,
                available: instrumentation.version,
            },
        },

        Located::Absent => match foreign(content, None) {
            Some(refusal) => refusal,
            None => match block::insertion_point(content) {
                Some(_) => Presence::Missing,
                None => Presence::NotAnObject,
            },
        },
    }
}

/// Le même classement, en lisant le fichier — la question que pose l'écran de réglages.
pub fn inspect(files: &dyn ConfigFiles, instrumentation: &Instrumentation) -> Presence {
    match files.read(&instrumentation.file) {
        Ok(Some(content)) => presence(&content, instrumentation),
        // Pas de fichier du tout : rien n'y est écrit, et une installation le créerait.
        Ok(None) => Presence::Missing,
        Err(why) => Presence::Unreadable { why },
    }
}

/// Ash n'écrit pas dans un fichier qui a déjà des hooks à lui.
///
/// **La détection est délibérément grossière** : toute occurrence de `"hooks"` hors du bloc
/// suffit à refuser, même dans une chaîne ou une clé imbriquée. Se tromper dans ce sens fait
/// perdre une installation, et l'utilisateur l'apprend ; se tromper dans l'autre écrit une
/// seconde clé `"hooks"` dans son objet racine, où le dernier arrivé l'emporte — donc
/// désactive silencieusement les hooks qu'il avait écrits lui-même.
///
/// Fusionner les deux configurations serait la vraie réponse, mais elle demande de modifier
/// du texte **hors** des marqueurs : c'est précisément ce que toute cette feature interdit,
/// et ça mérite sa propre décision.
fn foreign(content: &str, ours: Option<Range<usize>>) -> Option<Presence> {
    let ours = ours.unwrap_or(0..0);
    content
        .match_indices("\"hooks\"")
        .any(|(at, _)| !ours.contains(&at))
        .then_some(Presence::ForeignHooks)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::features::agents::{Adapter, ClaudeCodeAdapter};
    use crate::features::hooks::fakes::FakeConfigFiles;
    use crate::features::hooks::install;

    fn instrumentation(config_dir: &str) -> Instrumentation {
        ClaudeCodeAdapter::new(PathBuf::from(
            "/Applications/Ash.app/Contents/MacOS/ash-event",
        ))
        .instrumentation(Path::new(config_dir))
        .unwrap_or_else(|| panic!("claude-code instrumente toujours"))
    }

    #[test]
    fn given_a_configuration_file_ash_never_touched_when_it_is_inspected_then_the_block_is_missing_and_nothing_was_read_twice(
    ) {
        // Given — l'écran de réglages pose la question à chaque affichage ; y répondre ne
        // doit rien écrire, sans quoi ouvrir la fenêtre modifierait le fichier de
        // l'utilisateur
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            "{\n  \"model\": \"opus\"\n}\n",
        );
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let found = inspect(&files, &instrumentation);

        // Then
        assert_eq!(found, Presence::Missing);
        assert_eq!(
            files.journal(),
            vec!["read /home/someone/.claude/settings.json"]
        );
    }

    #[test]
    fn given_a_block_ash_just_installed_when_it_is_inspected_then_it_is_the_current_one() {
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
    fn given_a_block_written_by_an_older_ash_when_it_is_inspected_then_it_names_both_versions() {
        // Given — l'état `v1 · v2 available` : l'écran doit pouvoir dire de quoi vers quoi,
        // sinon « mettre à jour » ne dit pas ce qu'il changerait
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", "{}\n");
        let older = instrumentation("/home/someone/.claude");
        install(&files, &older).unwrap_or_else(|why| panic!("{why}"));
        let newer = Instrumentation {
            block: older.block.replace("--tab", "--onglet"),
            version: older.version + 1,
            ..older.clone()
        };

        // When
        let found = inspect(&files, &newer);

        // Then
        assert_eq!(
            found,
            Presence::Superseded {
                installed: older.version,
                available: older.version + 1,
            }
        );
    }

    #[test]
    fn given_a_block_someone_edited_by_hand_when_it_is_inspected_then_it_carries_the_diverging_lines(
    ) {
        // Given — refuser sans montrer ce qui diffère ne laisse que le choix de tout
        // effacer (spec §10). Le diff est une partie du refus, pas un agrément
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
                .any(|line| line.starts_with('+') && line.contains("mon-script")),
            "le diff montre la ligne de l'utilisateur :\n{diff}"
        );
    }

    #[test]
    fn given_a_file_that_already_carries_hooks_of_its_own_when_it_is_inspected_then_ash_says_it_is_blocked(
    ) {
        // Given — c'est le refus que les vrais utilisateurs heurteront en premier : écrire
        // une seconde clé `"hooks"` désactiverait la leur en silence
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            "{\n  \"hooks\": {\"Stop\": \"le mien\"}\n}\n",
        );

        // When
        let found = inspect(&files, &instrumentation("/home/someone/.claude"));

        // Then
        assert_eq!(found, Presence::ForeignHooks);
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
            fn write(&self, _: &Path, _: &block::Document) -> Result<(), String> {
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
