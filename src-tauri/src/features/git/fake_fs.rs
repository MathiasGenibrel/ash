//! L'arbre de fichiers en mémoire — le second adaptateur du port [`FileSystem`].
//!
//! Il vit à côté de [`super::system_fs`] plutôt que dans le module de tests de la
//! résolution : c'est un adaptateur du port, pas une règle de résolution, et les
//! prochains modules de la feature (refs, graphe, rebase — [ADR-0011](../../../../docs/adr/0011-git-domaine-de-premier-plan.md))
//! s'en serviront aussi. C'est lui qui fait de `FileSystem` une vraie couture : deux
//! adaptateurs, pas un.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use super::ports::{Entry, FileSystem};

/// Test Data Builder : un arbre de fichiers monté par cas d'usage git plutôt que dossier
/// par dossier — c'est ce qui rend le `Given` lisible.
///
/// Défaut valide et déterministe : un arbre vide, où rien n'existe.
#[derive(Debug, Default)]
pub struct FakeFs {
    dirs: BTreeSet<PathBuf>,
    files: BTreeMap<PathBuf, String>,
}

impl FakeFs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn dir(mut self, path: &str) -> Self {
        self.add_dir(Path::new(path));
        self
    }

    pub fn file(mut self, path: &str, content: &str) -> Self {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            self.add_dir(parent);
        }
        self.files.insert(path, content.to_owned());
        self
    }

    /// Un dépôt sans le moindre worktree lié : `.git` est un dossier.
    pub fn plain_repo(self, root: &str) -> Self {
        self.dir(&format!("{root}/.git/refs"))
    }

    /// Un dépôt qui héberge des worktrees liés : le `.git/worktrees/<nom>` de chacun,
    /// avec le `commondir` que git y écrit.
    pub fn repo_hosting(self, root: &str, worktrees: &[&str]) -> Self {
        worktrees.iter().fold(self.plain_repo(root), |fs, name| {
            fs.dir(&format!("{root}/.git/worktrees/{name}")).file(
                &format!("{root}/.git/worktrees/{name}/commondir"),
                "../..\n",
            )
        })
    }

    /// Un worktree lié : `.git` y est un **fichier**.
    pub fn linked_worktree(self, root: &str, git_file: &str) -> Self {
        self.dir(root).file(&format!("{root}/.git"), git_file)
    }

    fn add_dir(&mut self, path: &Path) {
        for ancestor in path.ancestors() {
            self.dirs.insert(ancestor.to_owned());
        }
    }

    /// Le `canonicalize` d'un arbre sans lien symbolique : `.` et `..` réduits.
    fn normalize(path: &Path) -> PathBuf {
        path.components()
            .fold(PathBuf::new(), |mut out, component| {
                match component {
                    Component::ParentDir => {
                        out.pop();
                    }
                    Component::CurDir => {}
                    other => out.push(other.as_os_str()),
                }
                out
            })
    }
}

impl FileSystem for FakeFs {
    fn entry(&self, path: &Path) -> Option<Entry> {
        let path = Self::normalize(path);
        if self.files.contains_key(&path) {
            Some(Entry::File)
        } else if self.dirs.contains(&path) {
            Some(Entry::Directory)
        } else {
            None
        }
    }

    fn read_to_string(&self, path: &Path) -> Result<String, String> {
        self.files
            .get(&Self::normalize(path))
            .cloned()
            .ok_or_else(|| "aucun fichier".to_owned())
    }

    fn has_entries(&self, path: &Path) -> bool {
        let path = Self::normalize(path);
        let child_of = |candidate: &PathBuf| candidate.parent() == Some(path.as_path());
        self.dirs.contains(&path)
            && (self.dirs.iter().any(child_of) || self.files.keys().any(child_of))
    }

    fn canonicalize(&self, path: &Path) -> Option<PathBuf> {
        let path = Self::normalize(path);
        self.entry(&path).map(|_| path)
    }
}
