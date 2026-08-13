//! Poser les entrées, et les reprendre. Le seul endroit du produit qui écrit chez
//! l'utilisateur.
//!
//! L'ordre des gestes est la règle, et il ne se réarrange pas :
//!
//! 1. **lire** le fichier tel qu'il est ;
//! 2. **décider** — rien à faire, fusionner, réécrire, ou refuser ;
//! 3. **sauvegarder**, si et seulement si on va écrire ;
//! 4. **écrire**, d'un seul remplacement de document.
//!
//! Rien ne se décide après l'étape 3. Une sauvegarde prise « au passage », pendant qu'on
//! écrit, ne sauvegarde plus rien.
//!
//! **Ce qui a changé le 2026-08-12** ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md),
//! amendement) : `install` ne refuse plus devant des hooks qui ne sont pas les siens, ni
//! devant une entrée éditée à la main. Il fusionne, ou il réécrit ses propres entrées. Ce
//! n'est pas un affaiblissement de « jamais silencieux » : l'appel vient d'un clic que
//! l'écran n'allume qu'après avoir montré le conflit et le diff (#16), et la copie `.bak`
//! précède toujours l'écriture.

use std::path::{Path, PathBuf};

use super::document::{is_an_empty_object, Document};
use super::error::HookError;
use super::merge::{self, Plan};
use super::ports::ConfigFiles;
use crate::features::agents::Instrumentation;

/// Ce qu'une installation a fait, pour que l'écran de réglages le dise (#16).
#[derive(Debug, PartialEq, Eq)]
pub enum Installation {
    /// Les entrées ont été écrites — première pose, fusion, ou mise à jour.
    Written {
        file: PathBuf,
        /// La sauvegarde, si c'est cette installation qui l'a créée.
        backup: Option<PathBuf>,
        /// Le fichier n'existait pas : Ash l'a créé, et la désinstallation l'effacera.
        created_the_file: bool,
    },
    /// Ce qui est en place est déjà exactement ce qu'on écrirait. **Rien n'a été touché.**
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
        /// Le fichier ne contenait plus rien : il a été effacé (spec §10).
        deleted_the_file: bool,
    },
    /// Aucune entrée d'Ash : il n'était pas passé par là, ou en est déjà parti.
    NothingToRemove { file: PathBuf },
}

/// Pose, fusionne ou met à jour les entrées décrites par une [`Instrumentation`].
///
/// Le fichier cible vient de l'instrumentation, donc de l'adaptateur, donc du dossier de
/// configuration qu'on lui a donné : deux comptes Claude sont deux appels, deux fichiers,
/// deux jeux d'entrées, et cette fonction n'a rien à savoir de leur existence mutuelle
/// (ADR-0007).
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
        let document = merge::fresh(instrumentation).ok_or(HookError::NotAnObject {
            file: file.to_owned(),
        })?;
        write(files, file, &document)?;
        return Ok(Installation::Written {
            file: file.to_owned(),
            backup: None,
            created_the_file,
        });
    };

    let document = match merge::plan(&content, instrumentation) {
        Plan::Current { .. } => {
            return Ok(Installation::AlreadyCurrent {
                file: file.to_owned(),
            })
        }
        Plan::Unusable => {
            return Err(HookError::NotAnObject {
                file: file.to_owned(),
            })
        }
        Plan::Write { document, .. } => document,
    };

    let backup = back_up(files, file)?;
    write(files, file, &document)?;

    Ok(Installation::Written {
        file: file.to_owned(),
        backup,
        created_the_file: false,
    })
}

/// Retire les entrées d'Ash, et le fichier avec elles s'il ne portait rien d'autre.
///
/// La sauvegarde, elle, **reste**. Elle est la copie du `settings.json` d'avant Ash, et
/// l'effacer au moment précis où l'on désinstalle serait retirer le filet juste avant de
/// sauter. C'est à l'écran de réglages de proposer de s'en défaire (#16), une fois que
/// l'utilisateur a constaté que sa configuration est intacte.
pub fn uninstall(
    files: &dyn ConfigFiles,
    instrumentation: &Instrumentation,
) -> Result<Removal, HookError> {
    let file = instrumentation.file.as_path();
    let Some(content) = read(files, file)? else {
        return Ok(Removal::NothingToRemove {
            file: file.to_owned(),
        });
    };

    let Some(remaining) = merge::removal(&content, instrumentation) else {
        return Ok(Removal::NothingToRemove {
            file: file.to_owned(),
        });
    };

    if is_an_empty_object(&remaining) {
        // Le fichier ne portait que les entrées d'Ash : il l'avait créé pour lui seul. Il
        // n'y a rien de l'utilisateur à sauvegarder, et un `.bak` laissé derrière serait
        // exactement la trace que la spec §10 refuse.
        files.remove(file).map_err(|why| HookError::Io {
            path: file.to_owned(),
            why,
        })?;
        return Ok(Removal::Removed {
            file: file.to_owned(),
            deleted_the_file: true,
        });
    }

    // « Toute écriture est précédée d'une copie » (spec §10) vaut aussi pour celle-ci, et
    // c'est même ici qu'elle compte le plus : les entrées sont retirées **qu'elles aient été
    // éditées ou non**, donc ce geste peut emporter des lignes que l'utilisateur y avait
    // ajoutées.
    back_up(files, file)?;
    write(files, file, &remaining)?;
    Ok(Removal::Removed {
        file: file.to_owned(),
        deleted_the_file: false,
    })
}

/// La sauvegarde, **avant** toute écriture, et une seule fois.
///
/// `settings.json.bak` est la copie d'**avant Ash**, et c'est la seule qui vaille : elle
/// est la seule dont on sait qu'aucune entrée d'Ash n'y traîne. La réécrire à chaque
/// installation remplacerait cette copie saine par une copie déjà instrumentée — donc
/// détruirait le filet au moment même où on prétend le tendre. Elle n'est donc jamais
/// écrasée.
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

fn write(files: &dyn ConfigFiles, file: &Path, content: &Document) -> Result<(), HookError> {
    files.write(file, content).map_err(|why| HookError::Io {
        path: file.to_owned(),
        why,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::{hook_mark, Adapter, ClaudeCodeAdapter, HookEntry};
    use crate::features::hooks::fakes::FakeConfigFiles;

    /// Le `settings.json` réel de l'utilisateur qui a signalé le défaut : un hook posé par
    /// un autre outil, et rien d'Ash.
    const THEIRS: &str = "{\n  \"hooks\": { \"PreToolUse\": [ { \"matcher\": \"Bash\",\n    \"hooks\": [ { \"type\": \"command\", \"command\": \"rtk hook claude\", \"timeout\": 5 } ] } ] }\n}\n";

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

    /// Ce que l'adaptateur voudra écrire après une mise à jour d'Ash : d'autres entrées, et
    /// la version qui va avec.
    ///
    /// C'est le seul moyen honnête de jouer « ce qui est en place a été écrit par un Ash
    /// plus ancien » tant que la version courante est la première.
    fn next_version(config_dir: &str) -> Instrumentation {
        let current = instrumentation(config_dir);
        Instrumentation {
            entries: current
                .entries
                .iter()
                .map(|entry| HookEntry {
                    path: entry.path.clone(),
                    item: entry
                        .item
                        .replace("--tab", "--onglet")
                        .replace(&hook_mark(current.version), &hook_mark(current.version + 1)),
                })
                .collect(),
            version: current.version + 1,
            ..current
        }
    }

    #[test]
    fn given_a_settings_file_that_already_carries_a_hook_of_its_own_when_ash_installs_then_it_merges_without_losing_it(
    ) {
        // Given — le refus que les vrais utilisateurs heurtaient en premier. Ash avait
        // raison de ne pas écrire une seconde clé `"hooks"` ; il avait tort d'en faire une
        // impasse. Il fusionne désormais, et le hook de l'utilisateur doit rester intact
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", THEIRS);
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let installed = install(&files, &instrumentation);
        let after = files.content_of(&instrumentation.file).unwrap_or_default();

        // Then
        assert!(
            matches!(installed, Ok(Installation::Written { .. })),
            "{installed:?}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&after)
            .unwrap_or_else(|why| panic!("le fichier n'est plus du JSON ({why}) :\n{after}"));
        let tools = parsed["hooks"]["PreToolUse"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            tools
                .iter()
                .any(|group| group["hooks"][0]["command"] == "rtk hook claude"),
            "le hook de l'utilisateur a disparu :\n{after}"
        );
        assert_eq!(tools.len(), 2, "et celui d'Ash est à côté :\n{after}");
        assert!(
            parsed["hooks"]["Stop"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap_or_default()
                .contains("ash-event"),
            "les quatre autres événements ont été créés :\n{after}"
        );
    }

    #[test]
    fn given_a_file_ash_merged_into_when_its_hooks_are_removed_then_the_file_is_back_to_the_byte() {
        // Given — le geste inverse, sur le même fichier réel. C'est la promesse la plus
        // lourde du projet : le fichier appartient à l'utilisateur, et le seul moyen de le
        // vérifier est de le comparer à lui-même
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", THEIRS);
        let instrumentation = instrumentation("/home/someone/.claude");
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));

        // When
        let removed = uninstall(&files, &instrumentation);

        // Then
        assert_eq!(
            removed,
            Ok(Removal::Removed {
                file: instrumentation.file.clone(),
                deleted_the_file: false,
            })
        );
        assert_eq!(
            files.content_of(&instrumentation.file).as_deref(),
            Some(THEIRS)
        );
    }

    #[test]
    fn given_a_settings_file_the_user_wrote_when_the_hooks_are_installed_then_only_ash_entries_appeared(
    ) {
        // Given — le fichier est le sien : son ordre de clés, son indentation, sa mise en
        // forme. C'est la promesse la plus lourde du projet, et la seule façon de la
        // vérifier est de comparer le fichier à lui-même une fois les entrées retirées.
        let theirs = "{\n    \"model\": \"opus\",\n    \"env\": {\"FOO\": \"bar\"}\n}\n";
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", theirs);
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let installed = install(&files, &instrumentation);
        let after = files.content_of(&instrumentation.file).unwrap_or_default();

        // Then
        assert!(matches!(installed, Ok(Installation::Written { .. })));
        assert!(after.contains(&hook_mark(instrumentation.version)));
        assert!(
            after.contains("    \"model\": \"opus\",\n    \"env\": {\"FOO\": \"bar\"}"),
            "les lignes de l'utilisateur ont bougé :\n{after}"
        );
        assert_eq!(
            uninstall(&files, &instrumentation),
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
    fn given_the_real_claude_code_entries_when_they_are_installed_then_the_file_is_json_that_declares_the_hooks(
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
            format!(
                "'/Applications/Ash.app/Contents/MacOS/ash-event' waiting --tab \"$ASH_TAB_ID\" {}",
                hook_mark(instrumentation.version)
            ),
            "la forme canonique de la spec §6.3, telle que le shell la lira, marqueur compris"
        );
        // Le sixième hook, celui des sous-agents : il écrit un verbe qui n'est **pas** un
        // état, et c'est ce qui l'empêche d'atteindre l'état de l'onglet (ADR-0007,
        // amendement du 2026-08-13).
        assert_eq!(
            parsed["hooks"]["SubagentStop"][0]["hooks"][0]["command"],
            format!(
                "'/Applications/Ash.app/Contents/MacOS/ash-event' subagent-stop --tab \"$ASH_TAB_ID\" {}",
                hook_mark(instrumentation.version)
            )
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
        // la première entrée. L'écraser à la mise à jour suivante remplacerait la copie sans
        // Ash par une copie avec Ash : on croirait avoir un filet, on n'aurait plus rien.
        let before_ash = "{\n  \"model\": \"opus\"\n}\n";
        let config_dir = "/home/someone/.claude";
        let files =
            FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", before_ash);
        install(&files, &instrumentation(config_dir)).unwrap_or_else(|why| panic!("{why}"));

        // When — Ash a changé de version, les entrées doivent être réécrites
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
    fn given_entries_written_by_an_older_ash_when_it_installs_again_then_they_are_rewritten_in_place(
    ) {
        // Given — une entrée périmée et une entrée éditée se ressemblent — les deux diffèrent
        // de ce qu'Ash écrirait — et c'est la version inscrite dans le marqueur qui les
        // sépare. Les confondre bloquerait toute mise à jour, pour tout le monde.
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
        let installed = instrumentation(config_dir).version;
        assert!(
            after.contains(&hook_mark(installed + 1)),
            "version à jour :\n{after}"
        );
        assert!(
            !after.contains(&hook_mark(installed)),
            "les anciennes entrées ont disparu :\n{after}"
        );
        assert!(after.contains("  \"model\": \"opus\""));
    }

    #[test]
    fn given_the_entries_already_in_place_when_ash_starts_again_then_the_users_file_is_not_touched()
    {
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
    fn given_a_config_dir_without_a_settings_file_when_ash_installs_then_uninstalling_leaves_no_file_behind(
    ) {
        // Given — le dossier d'un compte tout neuf. « La désinstallation ne laisse rien »
        // (spec §10) veut dire jusqu'au fichier lui-même : un `settings.json` vide qu'Ash
        // aurait créé serait une trace de plus, et une clé orpheline de moins.
        let files = FakeConfigFiles::new();
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let installed = install(&files, &instrumentation);
        let removed = uninstall(&files, &instrumentation);

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
    fn given_an_entry_someone_added_their_own_line_to_when_it_is_removed_then_a_copy_was_taken_first(
    ) {
        // Given — la désinstallation retire les entrées **même éditées**, donc ce geste peut
        // emporter des lignes que l'utilisateur y avait ajoutées. C'est exactement le cas où
        // « toute écriture est précédée d'une copie » (spec §10) n'est pas une formalité :
        // le `.bak` est la seule chose qui les lui rende.
        let theirs = "{\n  \"model\": \"opus\"\n}\n";
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", theirs);
        let instrumentation = instrumentation("/home/someone/.claude");
        install(&files, &instrumentation).unwrap_or_else(|why| panic!("{why}"));
        // Le `.bak` d'avant Ash est déjà là : on l'écarte pour observer celui de ce geste-ci.
        files
            .remove(Path::new("/home/someone/.claude/settings.json.bak"))
            .unwrap_or_else(|why| panic!("{why}"));
        let edited = files
            .content_of(&instrumentation.file)
            .unwrap_or_default()
            .replace("waiting --tab", "mon-script --tab");
        files.replace(&instrumentation.file, &edited);
        files.forget_the_journal();

        // When
        let removed = uninstall(&files, &instrumentation);

        // Then
        assert!(
            matches!(removed, Ok(Removal::Removed { .. })),
            "{removed:?}"
        );
        let journal = files.journal();
        let copied = journal.iter().position(|step| step.starts_with("copy"));
        let written = journal.iter().position(|step| step.starts_with("write"));
        assert!(
            copied < written && copied.is_some(),
            "la copie doit précéder l'écriture : {journal:?}"
        );
        assert!(files
            .content_of(Path::new("/home/someone/.claude/settings.json.bak"))
            .unwrap_or_default()
            .contains("mon-script"));
    }

    #[test]
    fn given_two_claude_accounts_when_both_are_instrumented_then_each_config_dir_gets_its_own_entries(
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
            assert!(
                content.contains(&hook_mark(instrumentation(".").version)),
                "{file} n'a pas d'entrée"
            );
            assert!(content.contains(model), "{file} a perdu son réglage");
        }
        // Puis on en retire un : l'autre ne bouge pas.
        uninstall(&files, &instrumentation("/home/someone/.claude"))
            .unwrap_or_else(|why| panic!("{why}"));
        assert!(files
            .content_of(Path::new("/home/someone/.claude-perso/settings.json"))
            .unwrap_or_default()
            .contains(&hook_mark(instrumentation(".").version)));
    }

    #[test]
    fn given_a_file_that_is_not_a_json_object_when_ash_installs_then_it_refuses_to_guess_where_to_write(
    ) {
        // Given — un `settings.json` remplacé par une liste, ou par un fichier de notes.
        // Poser les entrées « quelque part » produirait un fichier que l'outil ne lit plus.
        // C'est le refus qui reste, et il ne bouge pas.
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
        assert_eq!(
            files
                .content_of(Path::new("/home/someone/.claude/settings.json"))
                .as_deref(),
            Some("[1, 2, 3]\n")
        );
    }

    #[test]
    fn given_a_settings_file_ash_never_touched_when_it_is_uninstalled_then_nothing_happens() {
        // Given — la désinstallation se lance sur tous les dossiers connus, y compris ceux
        // où l'installation avait échoué. Y écrire « pour normaliser » serait une écriture
        // chez l'utilisateur sans aucune raison.
        let theirs = "{\n  \"model\": \"opus\"\n}\n";
        let files = FakeConfigFiles::new().carrying("/home/someone/.claude/settings.json", theirs);
        let instrumentation = instrumentation("/home/someone/.claude");

        // When
        let removed = uninstall(&files, &instrumentation);

        // Then
        assert_eq!(
            removed,
            Ok(Removal::NothingToRemove {
                file: instrumentation.file.clone()
            })
        );
        assert_eq!(
            files.journal(),
            ["read /home/someone/.claude/settings.json"]
        );
    }
}
