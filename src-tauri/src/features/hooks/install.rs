//! Poser le bloc, et le retirer. Le seul endroit du produit qui écrit chez l'utilisateur.
//!
//! L'ordre des gestes est la règle, et il ne se réarrange pas :
//!
//! 1. **lire** le fichier tel qu'il est ;
//! 2. **décider** — absent, à jour, périmé, édité à la main, occupé par d'autres hooks ;
//! 3. **sauvegarder**, si et seulement si on va écrire ;
//! 4. **écrire**, d'un seul remplacement de plage.
//!
//! Rien ne se décide après l'étape 3. Une sauvegarde prise « au passage », pendant qu'on
//! écrit, ne sauvegarde plus rien.

use std::ops::Range;
use std::path::{Path, PathBuf};

use super::block::{self, Located};
use super::diff;
use super::error::HookError;
use super::ports::ConfigFiles;
use crate::features::agents::Instrumentation;

/// Ce qu'une installation a fait, pour que l'écran de réglages le dise (#16).
#[derive(Debug, PartialEq, Eq)]
pub enum Installation {
    /// Le bloc a été écrit — première pose, ou mise à jour d'un bloc périmé.
    Written {
        file: PathBuf,
        /// La sauvegarde, si c'est cette installation qui l'a créée.
        backup: Option<PathBuf>,
        /// Le fichier n'existait pas : Ash l'a créé, et la désinstallation l'effacera.
        created_the_file: bool,
    },
    /// Le bloc en place est déjà exactement celui qu'on écrirait. **Rien n'a été touché.**
    ///
    /// C'est le cas de tous les démarrages d'Ash après le premier, et c'est pour lui que
    /// [`Instrumentation`] doit être déterministe : réécrire un fichier identique
    /// réveillerait les surveillances de l'utilisateur pour rien.
    AlreadyCurrent { file: PathBuf },
}

/// Ce qu'une désinstallation a retiré.
#[derive(Debug, PartialEq, Eq)]
pub enum Removal {
    Removed {
        file: PathBuf,
        /// Le fichier ne contenait plus que le bloc : il a été effacé (spec §10).
        deleted_the_file: bool,
    },
    /// Aucun bloc : Ash n'était pas passé par là, ou en est déjà parti.
    NothingToRemove { file: PathBuf },
}

/// Pose ou met à jour le bloc décrit par une [`Instrumentation`].
///
/// Le fichier cible vient de l'instrumentation, donc de l'adaptateur, donc du dossier de
/// configuration qu'on lui a donné : deux comptes Claude sont deux appels, deux fichiers,
/// deux blocs, et cette fonction n'a rien à savoir de leur existence mutuelle (ADR-0007).
pub fn install(
    files: &dyn ConfigFiles,
    instrumentation: &Instrumentation,
) -> Result<Installation, HookError> {
    let file = instrumentation.file.as_path();
    let existing = read(files, file)?;

    let Some(content) = existing.filter(|content| !content.trim().is_empty()) else {
        // Pas de fichier, ou un fichier vide : Ash écrit le document entier. Il n'y a rien
        // à sauvegarder, et rien de l'utilisateur à préserver.
        let created_the_file = !files.exists(file);
        write(files, file, &block::fresh_document(instrumentation))?;
        return Ok(Installation::Written {
            file: file.to_owned(),
            backup: None,
            created_the_file,
        });
    };

    let placement = decide(&content, instrumentation, file)?;
    let Some((span, replacement)) = placement else {
        return Ok(Installation::AlreadyCurrent {
            file: file.to_owned(),
        });
    };

    let backup = back_up(files, file)?;
    let mut written = content;
    written.replace_range(span, &replacement);
    write(files, file, &written)?;

    Ok(Installation::Written {
        file: file.to_owned(),
        backup,
        created_the_file: false,
    })
}

/// Retire le bloc, et le fichier avec lui s'il ne portait rien d'autre.
///
/// La sauvegarde, elle, **reste**. Elle est la copie du `settings.json` d'avant Ash, et
/// l'effacer au moment précis où l'on désinstalle serait retirer le filet juste avant de
/// sauter. C'est à l'écran de réglages de proposer de s'en défaire (#16), une fois que
/// l'utilisateur a constaté que sa configuration est intacte.
pub fn uninstall(files: &dyn ConfigFiles, file: &Path) -> Result<Removal, HookError> {
    let Some(content) = read(files, file)? else {
        return Ok(Removal::NothingToRemove {
            file: file.to_owned(),
        });
    };

    let span = match block::locate(&content) {
        Located::Absent => {
            return Ok(Removal::NothingToRemove {
                file: file.to_owned(),
            })
        }
        // Le bloc est retiré qu'il ait été édité ou non : laisser derrière soi des marqueurs
        // qui annoncent « écrit par Ash » alors qu'Ash s'en va serait exactement la trace
        // que la spec §10 refuse. Ce qui protège l'édition de l'utilisateur, c'est le `.bak`.
        Located::Present(block) => block.span,
        // Des marqueurs qu'on ne sait plus lire : on ne devine pas où le bloc commence, et
        // découper au jugé abîmerait le fichier. C'est le seul cas où la désinstallation
        // demande une main humaine.
        Located::Damaged => {
            return Err(HookError::HandEdited {
                file: file.to_owned(),
                diff: diff::compare("", &content),
            })
        }
    };

    let mut remaining = content;
    remaining.replace_range(span, "");

    if block::is_an_empty_object(&remaining) {
        files.remove(file).map_err(|why| HookError::Io {
            path: file.to_owned(),
            why,
        })?;
        return Ok(Removal::Removed {
            file: file.to_owned(),
            deleted_the_file: true,
        });
    }

    write(files, file, &remaining)?;
    Ok(Removal::Removed {
        file: file.to_owned(),
        deleted_the_file: false,
    })
}

/// Où écrire, et quoi — ou `None` s'il n'y a rien à faire.
///
/// Toute la prudence de la feature tient dans cette fonction, et elle n'écrit rien : c'est
/// ce qui permet de prouver chaque refus sans qu'un seul octet ne parte sur le disque.
fn decide(
    content: &str,
    instrumentation: &Instrumentation,
    file: &Path,
) -> Result<Option<(Range<usize>, String)>, HookError> {
    let hand_edited = |carried: &str| HookError::HandEdited {
        file: file.to_owned(),
        diff: diff::compare(&instrumentation.block, carried),
    };

    match block::locate(content) {
        Located::Damaged => Err(hand_edited(content)),

        Located::Present(block) if !block.intact => Err(hand_edited(&block.payload)),

        Located::Present(block) => {
            refuse_foreign_hooks(content, Some(block.span.clone()), file)?;
            // Un bloc intact, de la version courante, au contenu identique : c'est le
            // démarrage ordinaire d'Ash, et le fichier de l'utilisateur ne doit pas bouger.
            if block.version == instrumentation.version && block.payload == instrumentation.block {
                return Ok(None);
            }
            let rest = content.get(block.span.end..).unwrap_or("");
            let comma = block::is_followed_by_an_entry(rest);
            Ok(Some((block.span, block::render(instrumentation, comma))))
        }

        Located::Absent => {
            refuse_foreign_hooks(content, None, file)?;
            let at = block::insertion_point(content).ok_or(HookError::NotAnObject {
                file: file.to_owned(),
            })?;
            let rest = content.get(at..).unwrap_or("");
            let comma = block::is_followed_by_an_entry(rest);
            Ok(Some((at..at, block::render(instrumentation, comma))))
        }
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
fn refuse_foreign_hooks(
    content: &str,
    ours: Option<Range<usize>>,
    file: &Path,
) -> Result<(), HookError> {
    let ours = ours.unwrap_or(0..0);
    let foreign = content
        .match_indices("\"hooks\"")
        .any(|(at, _)| !ours.contains(&at));

    if foreign {
        return Err(HookError::ForeignHooks {
            file: file.to_owned(),
        });
    }
    Ok(())
}

/// La sauvegarde, **avant** toute écriture, et une seule fois.
///
/// `settings.json.bak` est la copie d'**avant Ash**, et c'est la seule qui vaille : elle
/// est la seule dont on sait qu'aucun bloc n'y traîne. La réécrire à chaque installation
/// remplacerait cette copie saine par une copie déjà instrumentée — donc détruirait le
/// filet au moment même où on prétend le tendre. Elle n'est donc jamais écrasée.
fn back_up(files: &dyn ConfigFiles, file: &Path) -> Result<Option<PathBuf>, HookError> {
    let backup = backup_of(file);
    if !files.exists(file) || files.exists(&backup) {
        return Ok(None);
    }

    files.copy(file, &backup).map_err(|why| HookError::Io {
        path: backup.clone(),
        why,
    })?;
    Ok(Some(backup))
}

fn backup_of(file: &Path) -> PathBuf {
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "settings.json".to_owned());
    file.with_file_name(format!("{name}.bak"))
}

fn read(files: &dyn ConfigFiles, file: &Path) -> Result<Option<String>, HookError> {
    files.read(file).map_err(|why| HookError::Io {
        path: file.to_owned(),
        why,
    })
}

fn write(files: &dyn ConfigFiles, file: &Path, content: &str) -> Result<(), HookError> {
    files.write(file, content).map_err(|why| HookError::Io {
        path: file.to_owned(),
        why,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::{Adapter, ClaudeCodeAdapter};
    use crate::features::hooks::fakes::FakeConfigFiles;

    fn claude_code() -> ClaudeCodeAdapter {
        ClaudeCodeAdapter::new(PathBuf::from(
            "/Applications/Ash.app/Contents/MacOS/ash-event",
        ))
    }

    /// Ce que l'adaptateur veut faire écrire dans ce dossier de configuration.
    fn instrumentation(config_dir: &str) -> Instrumentation {
        claude_code()
            .instrumentation(Path::new(config_dir))
            .unwrap_or_else(|| panic!("claude-code instrumente toujours"))
    }

    /// Ce que l'adaptateur voudra écrire après une mise à jour d'Ash : un bloc de forme
    /// différente, et la version qui va avec.
    ///
    /// C'est le seul moyen honnête de jouer « le bloc en place a été écrit par un Ash plus
    /// ancien » tant que la version courante est la première.
    fn next_version(config_dir: &str) -> Instrumentation {
        let current = instrumentation(config_dir);
        Instrumentation {
            block: current.block.replace("--tab", "--onglet"),
            version: current.version + 1,
            ..current
        }
    }

    #[test]
    fn given_a_settings_file_the_user_wrote_when_the_hooks_are_installed_then_only_the_block_appeared(
    ) {
        // Given — le fichier est le sien : son ordre de clés, son indentation, sa mise en
        // forme. C'est la promesse la plus lourde du projet, et la seule façon de la vérifier
        // est de comparer le fichier à lui-même une fois le bloc retiré.
        let theirs = "{\n    \"model\": \"opus\",\n    \"env\": {\"FOO\": \"bar\"}\n}\n";
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", theirs);
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let installed = install(&files, &instrumentation);
        let after = files.content_of(&instrumentation.file).unwrap_or_default();

        // Then
        assert!(matches!(installed, Ok(Installation::Written { .. })));
        assert!(after.contains("ash:begin"));
        assert!(
            after.contains("    \"model\": \"opus\",\n    \"env\": {\"FOO\": \"bar\"}"),
            "les lignes de l'utilisateur ont bougé :\n{after}"
        );
        assert_eq!(
            uninstall(&files, &instrumentation.file),
            Ok(Removal::Removed {
                file: instrumentation.file.clone(),
                deleted_the_file: false,
            })
        );
        assert_eq!(
            files.content_of(&instrumentation.file).as_deref(),
            Some(theirs)
        );
    }

    #[test]
    fn given_the_real_claude_code_block_when_it_is_installed_then_the_file_is_json_that_declares_the_hooks(
    ) {
        // Given — c'est ici que les deux moitiés de la tranche se rencontrent : l'adaptateur
        // compose, la feature écrit. Chacune est vérifiée de son côté, mais seul le fichier
        // final dit si Claude Code y trouvera quelque chose. S'il n'était pas du JSON
        // valide, l'utilisateur perdrait toute sa configuration, et personne ne le saurait
        // avant d'avoir lancé un agent.
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            "{\n  \"model\": \"opus\",\n  \"env\": {\"FOO\": \"bar\"}\n}\n",
        );
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));
        let after = files.content_of(&instrumentation.file).unwrap_or_default();

        // Then
        let parsed: serde_json::Value = serde_json::from_str(&after)
            .unwrap_or_else(|why| panic!("le fichier n'est plus du JSON ({why}) :\n{after}"));
        assert_eq!(parsed["model"], "opus", "les réglages sont intacts");
        assert_eq!(
            parsed["hooks"]["Stop"][0]["hooks"][0]["command"],
            "'/Applications/Ash.app/Contents/MacOS/ash-event' waiting --tab \"$ASH_TAB_ID\"",
            "la forme canonique de la spec §6.3, telle que le shell la lira"
        );
        assert!(
            parsed["//ash:begin"]
                .as_str()
                .unwrap_or_default()
                .contains("ash block v1"),
            "le marqueur porte sa version :\n{after}"
        );
    }

    #[test]
    fn given_a_settings_file_the_user_wrote_when_the_hooks_are_installed_then_the_backup_was_taken_first(
    ) {
        // Given — la sauvegarde n'a de valeur que si elle précède l'écriture. Le double
        // retient l'ordre des gestes, parce que c'est le seul moyen de distinguer « a
        // sauvegardé » de « a sauvegardé au bon moment ».
        let theirs = "{\n  \"model\": \"opus\"\n}\n";
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", theirs);
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let installed = install(&files, &instrumentation);

        // Then
        assert_eq!(
            installed,
            Ok(Installation::Written {
                file: instrumentation.file.clone(),
                backup: Some(PathBuf::from("/home/someone/.claude/settings.json.bak")),
                created_the_file: false,
            })
        );
        assert_eq!(
            files.journal(),
            [
                "read /home/someone/.claude/settings.json",
                "copy /home/someone/.claude/settings.json -> /home/someone/.claude/settings.json.bak",
                "write /home/someone/.claude/settings.json",
            ]
        );
        assert_eq!(
            files
                .content_of(Path::new("/home/someone/.claude/settings.json.bak"))
                .as_deref(),
            Some(theirs)
        );
    }

    #[test]
    fn given_a_backup_from_before_ash_when_a_later_install_runs_then_that_first_copy_is_kept() {
        // Given — la seule copie saine du `settings.json` de l'utilisateur est celle d'avant
        // le premier bloc. L'écraser à la mise à jour suivante remplacerait la copie sans
        // bloc par une copie avec bloc : on croirait avoir un filet, on n'aurait plus rien.
        let before_ash = "{\n  \"model\": \"opus\"\n}\n";
        let config_dir = "/home/someone/.claude";
        let files =
            FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", before_ash);
        install(&files, &instrumentation(config_dir)).unwrap_or_else(|why| panic!("{why}"));

        // When — Ash a changé de version, le bloc doit être réécrit
        let updated = install(&files, &next_version(config_dir));

        // Then
        assert!(matches!(
            updated,
            Ok(Installation::Written { backup: None, .. })
        ));
        assert_eq!(
            files
                .content_of(Path::new("/home/someone/.claude/settings.json.bak"))
                .as_deref(),
            Some(before_ash)
        );
    }

    #[test]
    fn given_a_block_the_user_edited_by_hand_when_ash_installs_again_then_it_refuses_and_shows_what_changed(
    ) {
        // Given — la règle de la spec §10 : Ash ne réécrit pas silencieusement, il signale,
        // propose le diff, et demande. Le fichier appartient à l'utilisateur ; son édition
        // est peut-être exactement ce qu'il voulait.
        let config_dir = "/home/someone/.claude";
        let instrumentation = instrumentation(config_dir);
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            "{\n  \"model\": \"opus\"\n}\n",
        );
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));
        let installed = files.content_of(&instrumentation.file).unwrap_or_default();
        files.replace(
            &instrumentation.file,
            &installed.replace("--tab \\\"$ASH_TAB_ID\\\"", "--tab moi"),
        );
        let edited = files.content_of(&instrumentation.file).unwrap_or_default();

        // When
        let refused = install(&files, &instrumentation);

        // Then — et surtout : le fichier n'a pas bougé d'un octet
        let HookError::HandEdited { diff, .. } = refused.unwrap_err() else {
            panic!("un bloc édité à la main doit être refusé comme tel");
        };
        assert!(
            diff.contains("+ "),
            "le diff doit montrer l'édition :\n{diff}"
        );
        assert_eq!(
            files.content_of(&instrumentation.file).as_deref(),
            Some(edited.as_str())
        );
    }

    #[test]
    fn given_a_block_written_by_an_older_ash_when_it_installs_again_then_it_is_rewritten_without_asking(
    ) {
        // Given — l'autre moitié de la même règle. Un bloc périmé et un bloc édité se
        // ressemblent — les deux diffèrent de ce qu'Ash écrirait — et c'est la version
        // inscrite dans le marqueur qui les sépare. Les confondre bloquerait toute mise à
        // jour du bloc, pour tout le monde.
        let config_dir = "/home/someone/.claude";
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            "{\n  \"model\": \"opus\"\n}\n",
        );
        install(&files, &instrumentation(config_dir)).unwrap_or_else(|why| panic!("{why}"));

        // When
        let updated = install(&files, &next_version(config_dir));
        let after = files
            .content_of(Path::new("/home/someone/.claude/settings.json"))
            .unwrap_or_default();

        // Then
        assert!(matches!(updated, Ok(Installation::Written { .. })));
        assert!(after.contains("ash block v2 "), "version à jour :\n{after}");
        assert!(
            !after.contains("--tab \\\"$ASH_TAB_ID\\\""),
            "l'ancien bloc a disparu :\n{after}"
        );
        assert!(after.contains("  \"model\": \"opus\""));
    }

    #[test]
    fn given_the_block_already_in_place_when_ash_starts_again_then_the_users_file_is_not_touched() {
        // Given — Ash démarre plusieurs fois par jour. Réécrire un fichier identique
        // réveillerait les surveillances de l'utilisateur, changerait la date de son
        // `settings.json`, et ferait grossir un diff git dans les dotfiles de ceux qui les
        // versionnent.
        let config_dir = "/home/someone/.claude";
        let instrumentation = instrumentation(config_dir);
        let files = FakeConfigFiles::new().carrying(
            "/home/someone/.claude/settings.json",
            "{\n  \"model\": \"opus\"\n}\n",
        );
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));
        files.forget_the_journal();

        // When
        let again = install(&files, &instrumentation);

        // Then
        assert_eq!(
            again,
            Ok(Installation::AlreadyCurrent {
                file: instrumentation.file.clone()
            })
        );
        assert_eq!(
            files.journal(),
            ["read /home/someone/.claude/settings.json"]
        );
    }

    #[test]
    fn given_a_settings_file_that_already_declares_its_own_hooks_when_ash_installs_then_it_refuses_rather_than_duplicate_the_key(
    ) {
        // Given — l'utilisateur a ses propres hooks. Ajouter les nôtres à côté écrirait une
        // seconde clé `"hooks"` dans le même objet : le dernier arrivé l'emporte, donc les
        // siens s'arrêteraient de fonctionner sans qu'aucun message ne le dise.
        let theirs = "{\n  \"hooks\": {\n    \"Stop\": []\n  }\n}\n";
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", theirs);

        // When
        let refused = install(&files, &instrumentation("/home/someone/.claude"));

        // Then
        assert_eq!(
            refused,
            Err(HookError::ForeignHooks {
                file: PathBuf::from("/home/someone/.claude/settings.json")
            })
        );
        assert_eq!(
            files
                .content_of(Path::new("/home/someone/.claude/settings.json"))
                .as_deref(),
            Some(theirs)
        );
    }

    #[test]
    fn given_a_config_dir_without_a_settings_file_when_ash_installs_then_uninstalling_leaves_no_file_behind(
    ) {
        // Given — le dossier d'un compte tout neuf. « La désinstallation ne laisse rien »
        // (spec §10) veut dire jusqu'au fichier lui-même : un `settings.json` vide qu'Ash
        // aurait créé serait une trace de plus, et une clé orpheline de moins.
        let files = FakeConfigFiles::new();
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let installed = install(&files, &instrumentation);
        let removed = uninstall(&files, &instrumentation.file);

        // Then
        assert_eq!(
            installed,
            Ok(Installation::Written {
                file: instrumentation.file.clone(),
                backup: None,
                created_the_file: true,
            })
        );
        assert_eq!(
            removed,
            Ok(Removal::Removed {
                file: instrumentation.file.clone(),
                deleted_the_file: true,
            })
        );
        assert_eq!(files.content_of(&instrumentation.file), None);
    }

    #[test]
    fn given_two_claude_accounts_when_both_are_instrumented_then_each_config_dir_gets_its_own_block(
    ) {
        // Given — `claude` et `claude-perso` (ADR-0007). Un chemin retenu quelque part entre
        // les deux appels, ou une sauvegarde partagée, ferait que le second compte écraserait
        // le premier — et personne ne le verrait avant d'avoir lancé les deux.
        let files = FakeConfigFiles::new()
            .carrying(
                "/home/someone/.claude/settings.json",
                "{\n  \"model\": \"opus\"\n}\n",
            )
            .carrying(
                "/home/someone/.claude-perso/settings.json",
                "{\n  \"model\": \"haiku\"\n}\n",
            );

        // When
        let pro = install(&files, &instrumentation("/home/someone/.claude"));
        let perso = install(&files, &instrumentation("/home/someone/.claude-perso"));

        // Then
        assert!(matches!(pro, Ok(Installation::Written { .. })));
        assert!(matches!(perso, Ok(Installation::Written { .. })));
        for (file, model) in [
            ("/home/someone/.claude/settings.json", "opus"),
            ("/home/someone/.claude-perso/settings.json", "haiku"),
        ] {
            let content = files.content_of(Path::new(file)).unwrap_or_default();
            assert!(content.contains("ash:begin"), "{file} n'a pas de bloc");
            assert!(content.contains(model), "{file} a perdu son réglage");
        }
        // Puis on en retire un : l'autre ne bouge pas.
        uninstall(&files, Path::new("/home/someone/.claude/settings.json"))
            .unwrap_or_else(|why| panic!("{why}"));
        assert!(files
            .content_of(Path::new("/home/someone/.claude-perso/settings.json"))
            .unwrap_or_default()
            .contains("ash:begin"));
    }

    #[test]
    fn given_a_file_that_is_not_a_json_object_when_ash_installs_then_it_refuses_to_guess_where_to_write(
    ) {
        // Given — un `settings.json` remplacé par une liste, ou par un fichier de notes.
        // Poser le bloc « quelque part » produirait un fichier que l'outil ne lit plus.
        let files =
            FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", "[1, 2, 3]\n");

        // When
        let refused = install(&files, &instrumentation("/home/someone/.claude"));

        // Then
        assert_eq!(
            refused,
            Err(HookError::NotAnObject {
                file: PathBuf::from("/home/someone/.claude/settings.json")
            })
        );
    }

    #[test]
    fn given_a_settings_file_ash_never_touched_when_it_is_uninstalled_then_nothing_happens() {
        // Given — la désinstallation se lance sur tous les dossiers connus, y compris ceux
        // où l'installation avait échoué. Y écrire « pour normaliser » serait une écriture
        // chez l'utilisateur sans aucune raison.
        let theirs = "{\n  \"model\": \"opus\"\n}\n";
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", theirs);
        let file = PathBuf::from("/home/someone/.claude/settings.json");

        // When
        let removed = uninstall(&files, &file);

        // Then
        assert_eq!(removed, Ok(Removal::NothingToRemove { file: file.clone() }));
        assert_eq!(
            files.journal(),
            ["read /home/someone/.claude/settings.json"]
        );
    }
}
