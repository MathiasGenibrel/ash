//! Les onglets de merge, et **rien d'autre** — le premier onglet sans PTY.
//!
//! [ADR-0003](../../../../docs/adr/0003-zone-terminal-unique.md), reformulation du
//! 2026-08-10 : « Un onglet est soit un terminal, soit une **surface d'outil** (merge). »
//! Ce registre est la moitié « surface d'outil » de cette phrase, et il vit à côté de
//! `features::pty::registry` plutôt que dedans. Le choix est défendu dans [`super`].
//!
//! # Ce que ce registre ne contient pas
//!
//! Il ne contient **aucun état de résolution**. Pas de brouillon, pas de hunk retenu, pas
//! de fichier en mémoire : un onglet de merge, c'est un identifiant et une racine de
//! worktree. Tout le reste est relu à la demande dans le worktree et dans l'index
//! (spec §7.4 : « l'onglet de merge se ferme sans rien perdre : l'état vit dans l'index
//! git, pas dans Ash »).
//!
//! Cette pauvreté est la fonctionnalité. Un champ de plus ici, et fermer l'onglet en
//! perdrait le contenu.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::features::git::{Head, Operation, OperationKind};

use super::error::MergeError;
use super::ports::StoppedWorktree;
use super::sides::sides;

/// Identifiant d'onglet — le même genre d'ulid que celui d'un onglet de shell.
///
/// La même **forme** et le même espace de noms, volontairement : `⌘1..9`, `⌃⇥` et la
/// sidebar désignent un onglet par cette chaîne, et deux espaces d'identifiants
/// obligeraient chaque appelant à savoir de quel genre d'onglet il parle avant de pouvoir
/// le nommer. Ce qui les sépare est le **type de l'onglet**, pas la forme de sa clé.
pub type TabId = String;

/// Un onglet de merge ouvert, tel qu'il traverse la frontière.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MergeTabInfo {
    pub tab_id: TabId,
    /// La racine du worktree dont on résout le conflit. C'est la clé de rangement de la
    /// sidebar, la même que celle d'un onglet de shell (`TabLocation.worktreeRoot`).
    pub worktree_root: String,
    /// Ce que la ligne affiche — `rebase feat onto main`.
    ///
    /// Composé ici et non dans la webview, comme les libellés d'action de branche : un
    /// onglet nomme ses deux côtés partout, et deux compositions séparées finiraient par
    /// nommer deux choses différentes pour la même opération.
    pub title: String,
    /// L'opération est toujours arrêtée dans ce worktree.
    ///
    /// Faux quand le rebase a été terminé ou abandonné **ailleurs** — dans un terminal, par
    /// un agent. L'onglet reste alors ouvert et le dit : rien ne se ferme sans un geste de
    /// l'utilisateur ([ADR-0010](../../../../docs/adr/0010-la-sidebar-informe-l-ecran-agit.md)).
    pub live: bool,
}

/// Les onglets de merge vivants, **dans l'ordre**.
///
/// Le même `Vec` que le registre de PTY, et pour la même raison : l'ordre est ce que
/// `⌘1..9` numérote, et une table de hachage n'en a pas
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[derive(Default)]
pub struct MergeTabs {
    tabs: Mutex<Vec<OpenTab>>,
}

struct OpenTab {
    id: TabId,
    worktree_root: PathBuf,
}

impl MergeTabs {
    /// Ouvre un onglet de merge sur ce worktree, ou **retrouve celui qui existe déjà**.
    ///
    /// Un worktree n'a qu'une opération arrêtée à la fois : deux onglets de merge dessus
    /// montreraient le même index sous deux compteurs qui se contrediraient dès le premier
    /// hunk tranché. Rendre l'onglet existant est aussi ce qui rend `⌘⌃M` idempotent, sans
    /// que #32 ait à s'en soucier.
    pub fn open(&self, worktree_root: &Path, tab_id: TabId) -> TabId {
        let Ok(mut tabs) = self.tabs.lock() else {
            return tab_id;
        };
        if let Some(existing) = tabs.iter().find(|tab| tab.worktree_root == worktree_root) {
            return existing.id.clone();
        }
        tabs.push(OpenTab {
            id: tab_id.clone(),
            worktree_root: worktree_root.to_path_buf(),
        });
        tab_id
    }

    /// Ferme un onglet. **Rien n'est perdu** : il n'y avait rien à perdre.
    pub fn close(&self, tab_id: &str) {
        if let Ok(mut tabs) = self.tabs.lock() {
            tabs.retain(|tab| tab.id != tab_id);
        }
    }

    /// La racine du worktree d'un onglet.
    pub fn worktree_of(&self, tab_id: &str) -> Result<PathBuf, MergeError> {
        let tabs = self
            .tabs
            .lock()
            .map_err(|_| MergeError::UnknownTab(tab_id.to_owned()))?;
        tabs.iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.worktree_root.clone())
            .ok_or_else(|| MergeError::UnknownTab(tab_id.to_owned()))
    }

    /// Les onglets ouverts, dans l'ordre, avec leur titre **relu**.
    ///
    /// Le titre n'est pas retenu à l'ouverture : un rebase qui avance d'un pas change la
    /// phrase, et un titre figé mentirait sur ce que l'onglet montre.
    pub fn list(&self, worktrees: &dyn StoppedWorktree) -> Vec<MergeTabInfo> {
        let Ok(tabs) = self.tabs.lock() else {
            return Vec::new();
        };
        tabs.iter()
            .map(|tab| {
                let operation = worktrees
                    .stopped(&tab.worktree_root)
                    .map(|stopped| stopped.operation);
                let head = worktrees.head(&tab.worktree_root);
                MergeTabInfo {
                    tab_id: tab.id.clone(),
                    worktree_root: tab.worktree_root.display().to_string(),
                    title: title(operation.as_ref(), head.as_ref()),
                    live: operation.is_some(),
                }
            })
            .collect()
    }
}

/// Le nom de l'onglet — qui **nomme ses deux côtés**, comme partout ailleurs (spec §7.1).
///
/// Il est composé à partir de [`super::sides`], et non d'une seconde lecture de
/// l'opération : c'est ce qui interdit au titre de dire l'inverse des colonnes. Le jour où
/// l'un des deux se tromperait de sens, les deux se tromperaient ensemble — et le test qui
/// oppose un rebase à un merge des mêmes branches rougirait.
pub fn title(operation: Option<&Operation>, head: Option<&Head>) -> String {
    let (Some(operation), Some(head)) = (operation, head) else {
        // L'opération s'est terminée ailleurs. L'onglet ne ment pas là-dessus, et il ne se
        // referme pas non plus tout seul.
        return "nothing to merge".to_owned();
    };
    let named = sides(operation, head);
    let (verb, preposition) = match operation.kind {
        OperationKind::Rebase => ("rebase", "onto"),
        OperationKind::Am => ("am", "onto"),
        // « merge feat into main » : un merge amène l'autre branche **dans** la courante,
        // l'inverse d'un rebase. La même règle que `prompt.rs`, et la même que `sides.rs`.
        OperationKind::Merge => ("merge", "into"),
    };
    format!(
        "{verb} {} {preposition} {}",
        named.right.name, named.left.name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::merge::fakes::FakeWorktree;

    #[test]
    fn given_a_worktree_that_already_has_a_merge_tab_when_opening_another_then_the_same_tab_comes_back(
    ) {
        // Given — un worktree n'a qu'une opération arrêtée : deux onglets dessus
        // montreraient le même index sous deux compteurs qui se contrediraient
        let tabs = MergeTabs::default();
        let first = tabs.open(Path::new("/dev/ash"), "01AAA".to_owned());

        // When
        let second = tabs.open(Path::new("/dev/ash"), "01BBB".to_owned());

        // Then
        assert_eq!(second, first);
        assert_eq!(tabs.list(&FakeWorktree::none()).len(), 1);
    }

    #[test]
    fn given_a_merge_tab_when_it_is_closed_and_reopened_then_the_worktree_still_says_everything() {
        // Given — la preuve que fermer ne perd rien : le registre ne retient qu'un chemin,
        // donc il n'y a rien qui puisse ne pas revenir
        let tabs = MergeTabs::default();
        let worktrees = FakeWorktree::rebase();
        let opened = tabs.open(Path::new("/dev/ash"), "01AAA".to_owned());

        // When
        tabs.close(&opened);
        let reopened = tabs.open(Path::new("/dev/ash"), "01CCC".to_owned());

        // Then
        let listed = tabs.list(&worktrees);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].tab_id, reopened);
        assert_eq!(listed[0].title, "rebase feat onto main");
        assert!(listed[0].live);
    }

    #[test]
    fn given_a_rebase_finished_in_a_terminal_when_the_merge_tab_is_listed_then_it_says_so_and_stays_open(
    ) {
        // Given — l'utilisateur a fini le rebase ailleurs. Refermer l'onglet sous ses yeux
        // serait un geste qu'il n'a pas fait (ADR-0010).
        let tabs = MergeTabs::default();
        tabs.open(Path::new("/dev/ash"), "01AAA".to_owned());

        // When
        let listed = tabs.list(&FakeWorktree::none());

        // Then
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].live);
        assert_eq!(listed[0].title, "nothing to merge");
    }

    #[test]
    fn given_a_stopped_merge_when_the_tab_is_titled_then_the_incoming_branch_goes_into_the_current_one(
    ) {
        // Given — « merge feat into main », jamais « merge main into feat » : le sens du
        // merge est l'inverse de celui du rebase, et le titre le dit. Pour un merge, git
        // n'écrit aucun `head-name` : la branche courante ne se lit que dans `HEAD`.
        let merge = Operation {
            kind: OperationKind::Merge,
            branch: None,
            onto: Some("feat".to_owned()),
            progress: None,
        };
        let head = Head::Branch {
            name: "main".to_owned(),
        };

        // When
        let named = title(Some(&merge), Some(&head));

        // Then
        assert_eq!(named, "merge feat into main");
    }
}
