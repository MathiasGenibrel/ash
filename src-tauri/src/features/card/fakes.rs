//! Les doubles de la feature : un disque en mémoire, et une fiche à construire.
//!
//! Sans eux, prouver « Ash ne touche à rien hors de son bloc », « une sauvegarde précède
//! l'écriture » et « le `.gitignore` n'est jamais écrit » demanderait d'écrire dans un vrai
//! dépôt — donc de ne jamais lancer ces tests-là en série.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use super::block::{CLOSE, OPEN};
use super::document::CardDocument;
use super::modes::ModeStore;
use super::place::CardMode;
use super::ports::{AgentWork, CardFiles, WorkRecord};

/// Un disque en mémoire qui **retient tout ce qui y a été écrit**, dans l'ordre.
///
/// L'ordre n'est pas décoratif : c'est lui qui prouve que la sauvegarde précède l'écriture,
/// et non l'inverse.
#[derive(Default)]
pub struct MemoryCardFiles {
    tree: Mutex<BTreeMap<String, String>>,
    written: Mutex<Vec<String>>,
}

impl MemoryCardFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn file(self, path: &str, content: &str) -> Self {
        if let Ok(mut tree) = self.tree.lock() {
            tree.insert(path.to_owned(), content.to_owned());
        }
        self
    }

    pub fn contents(&self, path: &str) -> Option<String> {
        self.tree.lock().ok()?.get(path).cloned()
    }

    /// Tous les chemins touchés, dans l'ordre — copies comprises.
    pub fn written_paths(&self) -> Vec<String> {
        self.written
            .lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }

    pub fn writes(&self) -> usize {
        self.written_paths().len()
    }

    fn record(&self, path: &Path) {
        if let Ok(mut written) = self.written.lock() {
            written.push(path.to_string_lossy().into_owned());
        }
    }
}

impl CardFiles for MemoryCardFiles {
    fn read(&self, path: &Path) -> Result<Option<String>, String> {
        Ok(self
            .tree
            .lock()
            .map_err(|_| "verrou".to_owned())?
            .get(&path.to_string_lossy().into_owned())
            .cloned())
    }

    fn exists(&self, path: &Path) -> bool {
        self.read(path).ok().flatten().is_some()
    }

    fn write(&self, path: &Path, content: &CardDocument) -> Result<(), String> {
        self.record(path);
        self.tree.lock().map_err(|_| "verrou".to_owned())?.insert(
            path.to_string_lossy().into_owned(),
            content.as_str().to_owned(),
        );
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), String> {
        let content = self.read(from)?.ok_or_else(|| "absent".to_owned())?;
        self.record(to);
        self.tree
            .lock()
            .map_err(|_| "verrou".to_owned())?
            .insert(to.to_string_lossy().into_owned(), content);
        Ok(())
    }
}

/// Test Data Builder : une fiche telle qu'un utilisateur et des agents l'écrivent.
///
/// Les défauts sont **valides et déterministes**, et couvrent tout le markdown qu'ADR-0013
/// autorise : front matter, cases à cocher, tableau, clôture `mermaid`. C'est ce qui donne
/// leur valeur aux tests d'écriture — ils prouvent que rien de tout ça ne bouge.
pub struct CardBuilder {
    before: String,
    block: Option<String>,
    after: String,
}

impl Default for CardBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CardBuilder {
    pub fn new() -> Self {
        Self {
            before: "---\ntype: feat\nissue: 31\nbranch: feat/branch-card\nbase: main\nstatus: wip\n---\n\n# pourquoi\n\n- [x] fait\n- [ ] à faire\n\n| ce qui | est tranché |\n|---|---|\n| le format | markdown |\n\n```mermaid\nstateDiagram-v2\n  idle --> working\n```\n".to_owned(),
            block: Some(String::new()),
            after: "\n## hors périmètre\n\nrien.\n".to_owned(),
        }
    }

    pub fn without_a_block(mut self) -> Self {
        self.block = None;
        self
    }

    pub fn logging(mut self, body: &str) -> Self {
        self.block = Some(body.to_owned());
        self
    }

    /// Le format d'Ash **cité** dans la fiche, comme ADR-0013 le cite elle-même.
    pub fn quoting_the_format(mut self) -> Self {
        self.before
            .push_str("\n```markdown\n<!-- ash:log -->\n| agent |\n<!-- /ash:log -->\n```\n");
        self
    }

    pub fn build(self) -> String {
        match self.block {
            None => format!("{}{}", self.before, self.after),
            Some(body) => format!("{}\n{OPEN}\n{body}{CLOSE}\n{}", self.before, self.after),
        }
    }
}

/// Le journal d'attribution, doublé : ce que les agents ont écrit dans ce worktree.
#[derive(Default)]
pub struct FakeWork(pub Vec<WorkRecord>);

impl AgentWork for FakeWork {
    fn in_worktree(&self, _worktree_root: &Path) -> Vec<WorkRecord> {
        self.0.clone()
    }
}

/// Le magasin des choix, en mémoire.
#[derive(Default)]
pub struct MemoryModes(Mutex<BTreeMap<String, CardMode>>);

impl ModeStore for MemoryModes {
    fn chosen(&self, worktree_root: &Path) -> Option<CardMode> {
        self.0
            .lock()
            .ok()?
            .get(&worktree_root.to_string_lossy().into_owned())
            .copied()
    }

    fn choose(&self, worktree_root: &Path, mode: Option<CardMode>) {
        let Ok(mut chosen) = self.0.lock() else {
            return;
        };
        let key = worktree_root.to_string_lossy().into_owned();
        match mode {
            Some(mode) => {
                chosen.insert(key, mode);
            }
            None => {
                chosen.remove(&key);
            }
        }
    }
}
