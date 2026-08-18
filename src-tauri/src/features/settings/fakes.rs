//! Les doubles des deux ports, pour les tests de la feature.
//!
//! Même forme que `features/git/fakes.rs` : un arbre décrit à la main plutôt qu'un
//! `tempdir`, et un lanceur qui **enregistre ce qu'on lui a demandé de lancer sans rien
//! lancer**. Ce dernier point est ce qui rend la frontière de sécurité du test 4
//! démontrable — on peut affirmer qu'aucun processus n'a été créé, ce qu'un vrai
//! `Command` ne permettrait jamais.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::hooks::BlockAt;
use super::ports::{Answer, CommandRunner, ConfigFiles, Folder, HookBlocks, Launch};
use super::values::{Command, ConfigTarget};
use crate::features::hooks::Presence;
use crate::features::hooks::{Removal, Withdrawal};

/// Un système de fichiers en mémoire : ce qu'on trouve à chaque chemin, et le foyer.
pub struct FakeFolders {
    home: PathBuf,
    found: HashMap<PathBuf, Folder>,
}

impl FakeFolders {
    pub fn new(home: &str) -> Self {
        Self {
            home: PathBuf::from(home),
            found: HashMap::new(),
        }
    }

    /// Un dossier lisible et son contenu direct.
    #[must_use]
    pub fn folder(mut self, path: &str, entries: &[&str]) -> Self {
        self.found.insert(
            PathBuf::from(path),
            Folder::Readable(entries.iter().map(|e| (*e).to_owned()).collect()),
        );
        self
    }

    /// Autre chose qu'un dossier lisible — un fichier, un refus de lecture.
    #[must_use]
    pub fn at(mut self, path: &str, found: Folder) -> Self {
        self.found.insert(PathBuf::from(path), found);
        self
    }
}

impl ConfigFiles for FakeFolders {
    fn read_folder(&self, path: &Path) -> Folder {
        self.found.get(path).cloned().unwrap_or(Folder::Missing)
    }

    fn home(&self) -> Option<PathBuf> {
        Some(self.home.clone())
    }
}

/// Des blocs de hooks en mémoire — **et un journal**, parce que ce qui compte ici est ce
/// qu'Ash a écrit, pas seulement ce qu'il a répondu.
///
/// Un double qui ne retiendrait que l'état final laisserait passer la faute qu'on cherche :
/// une écriture faite sur une entrée que la séquence n'autorisait pas a exactement la même
/// trace qu'une écriture légitime.
pub struct FakeBlocks {
    /// Ce que chaque dossier porte. Absent = l'adaptateur n'instrumente pas ce dossier.
    found: HashMap<PathBuf, Presence>,
    /// Les adaptateurs qui n'instrumentent rien du tout — `generic`.
    silent: Vec<String>,
    written: Mutex<Vec<String>>,
}

impl FakeBlocks {
    pub fn new() -> Self {
        Self {
            found: HashMap::new(),
            silent: Vec::new(),
            written: Mutex::new(Vec::new()),
        }
    }

    /// Ce qu'on trouvera dans ce dossier de configuration.
    #[must_use]
    pub fn at(mut self, config_dir: &str, presence: Presence) -> Self {
        self.found.insert(PathBuf::from(config_dir), presence);
        self
    }

    /// Un adaptateur qui ne décrit aucune instrumentation.
    #[must_use]
    pub fn without_hooks(mut self, adapter: &str) -> Self {
        self.silent.push(adapter.to_owned());
        self
    }

    /// Ce qu'Ash a réellement écrit, dans l'ordre — vide veut dire « aucun fichier touché ».
    pub fn written(&self) -> Vec<String> {
        self.written
            .lock()
            .map(|seen| seen.clone())
            .unwrap_or_default()
    }
}

impl HookBlocks for FakeBlocks {
    fn inspect(&self, adapter: &str, config_dir: &ConfigTarget) -> Option<BlockAt> {
        if self.silent.iter().any(|id| id == adapter) {
            return None;
        }
        Some(BlockAt {
            file: config_dir.resolved().join("settings.json"),
            presence: self
                .found
                .get(config_dir.resolved())
                .cloned()
                .unwrap_or(Presence::Missing {
                    others: 0,
                    diff: String::new(),
                }),
        })
    }

    fn install(&self, adapter: &str, config_dir: &ConfigTarget) -> Result<(), String> {
        if let Ok(mut seen) = self.written.lock() {
            seen.push(format!(
                "install {adapter} {}",
                config_dir.resolved().display()
            ));
        }
        Ok(())
    }

    fn remove(&self, adapter: &str, config_dir: &ConfigTarget) -> Result<Removal, String> {
        if let Ok(mut seen) = self.written.lock() {
            seen.push(format!(
                "remove {adapter} {}",
                config_dir.resolved().display()
            ));
        }
        Ok(Removal::Removed {
            file: config_dir.resolved().join("settings.json"),
            deleted_the_file: false,
        })
    }

    fn foresee_removal(&self, adapter: &str, config_dir: &ConfigTarget) -> Option<Withdrawal> {
        if self.silent.iter().any(|id| id == adapter) {
            return None;
        }
        // Le double ne rejoue pas le classement de `features::hooks` : il rend ce que les
        // deux états qui portent des entrées d'Ash rendraient, et rien pour les autres.
        match self.found.get(config_dir.resolved()) {
            Some(Presence::Current { .. }) | Some(Presence::Superseded { .. }) => {
                Some(withdrawal(config_dir, false))
            }
            Some(Presence::HandEdited { .. }) => Some(withdrawal(config_dir, true)),
            _ => None,
        }
    }
}

/// Ce qu'un retrait emporterait, tel que le double le raconte.
fn withdrawal(config_dir: &ConfigTarget, hand_edited: bool) -> Withdrawal {
    Withdrawal {
        file: config_dir.resolved().join("settings.json"),
        entries: 5,
        deletes_the_file: false,
        hand_edited,
        diff: String::new(),
    }
}

/// Un lanceur qui ne lance rien, et se souvient de tout ce qu'on lui a demandé.
pub struct FakeCommands {
    path: HashMap<String, PathBuf>,
    answer: Option<bool>,
    launched: Mutex<Vec<Launch>>,
}

impl FakeCommands {
    pub fn new() -> Self {
        Self {
            path: HashMap::new(),
            answer: None,
            launched: Mutex::new(Vec::new()),
        }
    }

    /// Une commande que le `PATH` résout.
    #[must_use]
    pub fn in_path(mut self, command: &str, program: &str) -> Self {
        self.path.insert(command.to_owned(), PathBuf::from(program));
        self
    }

    /// Ce que la commande répondra le jour où on la lancera.
    #[must_use]
    pub fn answering(mut self, succeeded: bool) -> Self {
        self.answer = Some(succeeded);
        self
    }

    /// Tout ce qu'Ash a demandé à lancer — vide veut dire « aucun processus ».
    pub fn launches(&self) -> Vec<Launch> {
        self.launched
            .lock()
            .map(|seen| seen.clone())
            .unwrap_or_default()
    }
}

impl CommandRunner for FakeCommands {
    fn locate(&self, command: &Command) -> Option<PathBuf> {
        self.path.get(command.as_str()).cloned()
    }

    fn run(&self, launch: &Launch) -> Result<Answer, String> {
        if let Ok(mut seen) = self.launched.lock() {
            seen.push(launch.clone());
        }
        match self.answer {
            Some(succeeded) => Ok(Answer {
                succeeded,
                output: if succeeded {
                    "1.0.0".to_owned()
                } else {
                    "unknown option".to_owned()
                },
            }),
            None => Err("le scénario n'a pas dit ce que la commande répond".to_owned()),
        }
    }
}
