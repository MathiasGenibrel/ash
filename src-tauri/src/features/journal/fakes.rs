//! Les doubles des effets dont le journal dépend : le disque, `git log`, les onglets.
//!
//! Ils vivent ici plutôt que dans un module de tests parce qu'ils doublent des **ports**, et
//! qu'ils servent aux tests du journal comme à ceux de sa résolution.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::shared::time::{Clock, UnixMillis};

use super::commits::{CommitLog, CommitRecord};

use super::entry::Entry;
use super::error::JournalError;
use super::store::JournalStore;
use super::tabs::{TabAgent, Tabs};

/// Le journal en mémoire : le magasin, sans disque.
#[derive(Default)]
pub struct MemoryJournal(Mutex<BTreeMap<String, String>>);

impl MemoryJournal {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

impl JournalStore for MemoryJournal {
    fn append(&self, file: &str, line: &str) -> Result<(), JournalError> {
        if let Ok(mut files) = self.0.lock() {
            files.entry(file.to_owned()).or_default().push_str(line);
        }
        Ok(())
    }

    fn read(&self, file: &str) -> String {
        self.0
            .lock()
            .ok()
            .and_then(|files| files.get(file).cloned())
            .unwrap_or_default()
    }

    fn files(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|files| files.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn purge(&self) -> Result<(), JournalError> {
        if let Ok(mut files) = self.0.lock() {
            files.clear();
        }
        Ok(())
    }
}

/// Ce que `git log` répondrait — sans lancer de processus.
#[derive(Default)]
pub struct FakeCommits(Mutex<Vec<CommitRecord>>);

impl FakeCommits {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Les commits de `HEAD`, **du plus récent au plus ancien** — l'ordre de `git log`.
    pub fn set(&self, commits: Vec<CommitRecord>) {
        if let Ok(mut held) = self.0.lock() {
            *held = commits;
        }
    }
}

impl CommitLog for FakeCommits {
    fn recent(&self, _worktree_root: &Path) -> Vec<CommitRecord> {
        self.0.lock().map(|held| held.clone()).unwrap_or_default()
    }
}

/// Ce que les onglets portent, tel que le composition root le rendrait.
#[derive(Default)]
pub struct FakeTabs(Mutex<Vec<TabAgent>>);

impl FakeTabs {
    pub fn with_agent(worktree_root: &str, agent: &str, tab_id: &str) -> Arc<Self> {
        let tabs = Arc::new(Self::default());
        tabs.set_agent(worktree_root, agent, tab_id);
        tabs
    }

    pub fn set_agent(&self, worktree_root: &str, agent: &str, tab_id: &str) {
        self.replace(TabAgent {
            tab_id: tab_id.to_owned(),
            worktree_root: worktree_root.to_owned(),
            agent: Some(agent.to_owned()),
            since: 1_000,
        });
    }

    /// Un onglet à son invite : aucun outil reconnu.
    pub fn set_shell(&self, worktree_root: &str) {
        self.replace(TabAgent {
            tab_id: "01J0SHELL".to_owned(),
            worktree_root: worktree_root.to_owned(),
            agent: None,
            since: 1_000,
        });
    }

    fn replace(&self, tab: TabAgent) {
        if let Ok(mut tabs) = self.0.lock() {
            *tabs = vec![tab];
        }
    }
}

impl Tabs for FakeTabs {
    fn snapshot(&self) -> Vec<TabAgent> {
        self.0.lock().map(|tabs| tabs.clone()).unwrap_or_default()
    }
}

/// L'heure murale, arrêtée : le journal ne lit l'horloge qu'une fois, à sa naissance.
pub struct FrozenClock(pub UnixMillis);

impl Clock for FrozenClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wall(&self) -> UnixMillis {
        self.0
    }
}

/// Test Data Builder : une entrée valide, dont on ne surcharge que ce qui compte.
pub struct EntryBuilder {
    entry: Entry,
}

impl EntryBuilder {
    pub fn new() -> Self {
        Self {
            entry: Entry {
                repo: "/dev/ash/.git".to_owned(),
                sha: "8f3a1c2".to_owned(),
                author_date: "2026-08-12T14:03:21+02:00".to_owned(),
                subject: "feat(sidebar): group tabs by worktree".to_owned(),
                agent: "claude".to_owned(),
                tab_id: "01J0TAB".to_owned(),
                worktree: Some("/dev/ash".to_owned()),
                authored_at: Some(1_755_000_201_000),
                session_started: None,
                prompt: None,
            },
        }
    }

    pub fn sha(mut self, sha: &str) -> Self {
        self.entry.sha = sha.to_owned();
        self
    }

    pub fn agent(mut self, agent: &str) -> Self {
        self.entry.agent = agent.to_owned();
        self
    }

    pub fn build(self) -> Entry {
        self.entry
    }
}
