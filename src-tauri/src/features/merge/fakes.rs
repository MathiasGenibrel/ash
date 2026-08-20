//! Les doubles des trois ports — ce qui rend l'onglet vérifiable sans dépôt ni processus.
//!
//! Aucun `git` n'est lancé ici, aucun fichier n'est écrit sur le disque. Ce qui *doit*
//! passer par un vrai dépôt — que git écrive bien les marqueurs là où Ash les cherche, et
//! que les côtés ne s'échangent pas entre un rebase et un merge — est dans
//! `src-tauri/tests/merge_real_repository.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::features::git::{Head, Operation, OperationKind, Progress, StoppedOperation};

use super::ports::{ConflictFiles, MergeOutcome, StoppedWorktree, TreeGit};

/// Test Data Builder : un worktree, et ce qu'il dit de son opération.
pub struct FakeWorktree {
    stopped: Option<StoppedOperation>,
    head: Option<Head>,
}

impl FakeWorktree {
    /// Rien en cours — le cas courant d'un worktree tranquille.
    pub fn none() -> Self {
        Self {
            stopped: None,
            head: Some(Head::Branch {
                name: "main".to_owned(),
            }),
        }
    }

    /// Un rebase de `feat` sur `main`, arrêté au pas 2/5, sur deux fichiers.
    pub fn rebase() -> Self {
        Self {
            stopped: Some(StoppedOperation {
                operation: Operation {
                    kind: OperationKind::Rebase,
                    branch: Some("feat".to_owned()),
                    onto: Some("main".to_owned()),
                    progress: Some(Progress { step: 2, total: 5 }),
                },
                conflicts: vec!["src/probe.rs".to_owned(), "src/main.ts".to_owned()],
                conflicted_total: Some(2),
                stopped_at: None,
                orig_head: Some("80eca44".to_owned()),
                test_command: Some("cargo test".to_owned()),
                escapes: vec![
                    "git rebase --abort".to_owned(),
                    "git rebase --skip".to_owned(),
                ],
            }),
            // Pendant un rebase, git détache `HEAD`.
            head: Some(Head::Detached {
                commit: "1a2b3c4".to_owned(),
            }),
        }
    }

    /// Un merge de `feat` dans `main`, arrêté sur un fichier.
    pub fn merge() -> Self {
        Self {
            stopped: Some(StoppedOperation {
                operation: Operation {
                    kind: OperationKind::Merge,
                    branch: None,
                    onto: Some("feat".to_owned()),
                    progress: None,
                },
                conflicts: vec!["src/probe.rs".to_owned()],
                conflicted_total: Some(1),
                stopped_at: None,
                orig_head: Some("80eca44".to_owned()),
                test_command: None,
                escapes: vec!["git merge --abort".to_owned()],
            }),
            head: Some(Head::Branch {
                name: "main".to_owned(),
            }),
        }
    }

    /// Les chemins que git nomme, remplacés.
    pub fn conflicting(mut self, paths: &[&str]) -> Self {
        if let Some(stopped) = self.stopped.as_mut() {
            stopped.conflicts = paths.iter().map(|path| (*path).to_owned()).collect();
            stopped.conflicted_total = Some(paths.len() as u32);
        }
        self
    }

    /// git compte plus de conflits que la liste n'en porte — elle est bornée à cent.
    pub fn counting(mut self, total: u32) -> Self {
        if let Some(stopped) = self.stopped.as_mut() {
            stopped.conflicted_total = Some(total);
        }
        self
    }
}

impl StoppedWorktree for FakeWorktree {
    fn stopped(&self, _worktree_root: &Path) -> Option<StoppedOperation> {
        self.stopped.clone()
    }

    fn head(&self, _worktree_root: &Path) -> Option<Head> {
        self.head.clone()
    }
}

/// Les fichiers du worktree, en mémoire.
#[derive(Default)]
pub struct FakeFiles {
    contents: Mutex<HashMap<PathBuf, String>>,
}

impl FakeFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(self, path: &str, text: &str) -> Self {
        if let Ok(mut contents) = self.contents.lock() {
            contents.insert(PathBuf::from(path), text.to_owned());
        }
        self
    }

    pub fn content(&self, path: &str) -> Option<String> {
        self.contents
            .lock()
            .ok()?
            .get(&PathBuf::from(path))
            .cloned()
    }
}

impl ConflictFiles for FakeFiles {
    fn read(&self, path: &Path) -> Option<String> {
        self.contents.lock().ok()?.get(path).cloned()
    }

    fn write(&self, path: &Path, text: &str) -> Result<(), String> {
        let mut contents = self.contents.lock().map_err(|_| "verrou".to_owned())?;
        contents.insert(path.to_path_buf(), text.to_owned());
        Ok(())
    }
}

/// Le git qui écrit, remplacé par un carnet.
#[derive(Default)]
pub struct FakeGit {
    pub calls: Mutex<Vec<String>>,
}

impl FakeGit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ran(&self) -> Vec<String> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .unwrap_or_default()
    }
}

impl TreeGit for FakeGit {
    fn stage(&self, _worktree_root: &Path, path: &str) -> MergeOutcome {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(format!("add {path}"));
        }
        MergeOutcome {
            label: format!("Stage {path}"),
            success: true,
            output: String::new(),
        }
    }

    fn resume(&self, _worktree_root: &Path, kind: OperationKind) -> MergeOutcome {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(format!("continue {kind:?}"));
        }
        MergeOutcome {
            label: super::sides::continuation(kind),
            success: true,
            output: "Successfully rebased".to_owned(),
        }
    }
}
