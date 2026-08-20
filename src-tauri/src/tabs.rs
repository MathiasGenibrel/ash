//! La **réunion** des deux genres d'onglet — au composition root, et nulle part ailleurs.
//!
//! [ADR-0003](../../docs/adr/0003-zone-terminal-unique.md), reformulation du 2026-08-10 :
//! « Un onglet est soit un terminal, soit une **surface d'outil** (merge). » Les deux
//! genres vivent dans deux features qui ne se connaissent pas — `pty` tient des PTY,
//! `merge` tient des surfaces —, et la somme n'appartient donc à aucune des deux.
//!
//! Elle vit ici, dans le seul module qui a le droit de connaître les deux features. C'est
//! le prix du choix défendu dans `features::merge` : le registre de PTY garde son invariant
//! « un onglet **est** un PTY » littéralement vrai, et c'est ce fichier-ci qui porte
//! l'unique endroit où les deux listes se rencontrent.
//!
//! # L'ordre, qui n'est pas un détail
//!
//! C'est l'ordre que `⌘1..9` numérote et que `⌃⇥` parcourt (spec §4.4), et le frontend ne
//! le fabrique pas ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)). La règle
//! est : **les shells dans leur ordre, puis les surfaces de merge dans le leur**. Elle est
//! stable — ouvrir un onglet de merge ne renumérote aucun shell — et elle se dit en une
//! phrase, ce qu'un entrelacement par date d'ouverture ne ferait pas sans qu'on tienne un
//! troisième registre juste pour l'ordre.
//!
//! # Ce que le `kind` change côté frontend
//!
//! Un onglet de merge n'a ni `cwd`, ni processus en avant-plan, ni état d'agent, ni
//! `stateSince`, ni pause : ces champs **ne sont pas** dans sa variante. C'est délibéré et
//! c'est le cœur du ticket — les remplir de valeurs neutres ferait apparaître une ligne
//! `idle · 12m` sous un onglet où aucun processus ne tourne, et la ligne de statut
//! afficherait la durée d'un état qui n'existe pas.

use std::path::Path;
use std::sync::Arc;

use crate::features::merge::{MergeSurface, MergeTabInfo};
use crate::features::pty::{PtyError, PtyRegistry, TabInfo, TabLocation, WorktreeLocator};

/// Un onglet, quel que soit son genre.
///
/// Étiquetée **à l'intérieur** (`kind`) : le frontend reçoit un objet plat qu'il discrimine
/// sur un champ, comme il le fait déjà pour `Head` et pour `PtyFrame`.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Tab {
    /// Un terminal — ce qu'un onglet a toujours été jusqu'ici.
    Shell(TabInfo),
    /// Une surface de merge : **pas de PTY du tout** (ADR-0003).
    Merge(MergeTab),
}

/// Un onglet de merge, situé.
///
/// La localisation est ajoutée **ici** et non par la feature : résoudre un répertoire en
/// worktree et en dépôt est le port de `pty` (`WorktreeLocator`), et `merge` n'a pas à le
/// connaître pour montrer trois panneaux. C'est le composition root qui relie les deux,
/// exactement comme il relie la sonde et la surveillance git.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MergeTab {
    pub tab_id: String,
    /// La racine du worktree dont on résout le conflit — la clé de rangement de la sidebar.
    pub worktree_root: String,
    /// Ce que la ligne affiche — `rebase feat onto main`, composé par la feature.
    pub title: String,
    /// L'opération est toujours arrêtée dans ce worktree.
    pub live: bool,
    /// Où cet onglet se range dans la hiérarchie d'ADR-0012, comme un onglet de shell.
    ///
    /// `None` quand le worktree n'a pas pu être situé — un `.git` cassé, un dossier
    /// disparu. La sidebar le montre quand même, à plat : perdre un onglet vivant serait
    /// pire.
    pub location: Option<TabLocation>,
}

/// Les onglets vivants, dans l'ordre que le backend détient — **les deux genres**.
///
/// Le frontend les relit après chaque ouverture et chaque fermeture plutôt que d'en tenir
/// une copie qu'il ferait évoluer de son côté (ADR-0009).
#[tauri::command]
pub fn tabs(
    registry: tauri::State<'_, Arc<PtyRegistry>>,
    merges: tauri::State<'_, Arc<MergeSurface>>,
    locator: tauri::State<'_, Arc<dyn WorktreeLocator>>,
) -> Result<Vec<Tab>, PtyError> {
    let shells = registry.tabs()?;
    Ok(join(shells, merges.list(), locator.inner().as_ref()))
}

/// La règle d'ordre, écrite une fois — et vérifiable sans Tauri.
fn join(
    shells: Vec<TabInfo>,
    merges: Vec<MergeTabInfo>,
    locator: &dyn WorktreeLocator,
) -> Vec<Tab> {
    shells
        .into_iter()
        .map(Tab::Shell)
        .chain(merges.into_iter().map(|tab| {
            let location = locator.locate(Path::new(&tab.worktree_root));
            Tab::Merge(MergeTab {
                tab_id: tab.tab_id,
                worktree_root: tab.worktree_root,
                title: tab.title,
                live: tab.live,
                location,
            })
        }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::AgentState;
    use crate::features::pty::RepoRef;

    /// Un localisateur qui situe tout dans le même dépôt — le décor, pas le sujet.
    struct HereLocator;

    impl WorktreeLocator for HereLocator {
        fn locate(&self, cwd: &Path) -> Option<TabLocation> {
            Some(TabLocation {
                worktree_root: cwd.display().to_string(),
                worktree_name: "ash".to_owned(),
                repo: Some(RepoRef {
                    id: "/dev/ash/.git".to_owned(),
                    name: "ash".to_owned(),
                }),
            })
        }
    }

    fn merge_tab(id: &str) -> MergeTabInfo {
        MergeTabInfo {
            tab_id: id.to_owned(),
            worktree_root: "/dev/ash".to_owned(),
            title: "rebase feat onto main".to_owned(),
            live: true,
        }
    }

    fn shell_tab(id: &str) -> TabInfo {
        TabInfo {
            tab_id: id.to_owned(),
            cwd: "/dev/ash".to_owned(),
            process: "zsh".to_owned(),
            agent: None,
            state: AgentState::Idle,
            state_since: 0,
            subagents: Vec::new(),
            location: None,
            paused: false,
        }
    }

    /// Le rang de chaque onglet dans la liste — ce que `⌘1..9` numérote (spec §4.4).
    fn numbering(listed: &[Tab]) -> Vec<&str> {
        listed
            .iter()
            .map(|tab| match tab {
                Tab::Shell(shell) => shell.tab_id.as_str(),
                Tab::Merge(merge) => merge.tab_id.as_str(),
            })
            .collect()
    }

    #[test]
    fn given_a_merge_tab_opened_while_shells_are_running_when_the_tabs_are_listed_then_no_shell_is_renumbered(
    ) {
        // Given — deux terminaux déjà ouverts, en `⌘1` et `⌘2`
        let shells = vec![shell_tab("01A"), shell_tab("01B")];
        let alone = join(shells.clone(), Vec::new(), &HereLocator);
        let before = numbering(&alone);

        // When — une surface d'outil s'ouvre
        let listed = join(shells, vec![merge_tab("01M")], &HereLocator);

        // Then — les shells gardent leur rang, et la surface prend le suivant
        assert_eq!(before, ["01A", "01B"]);
        assert_eq!(numbering(&listed), ["01A", "01B", "01M"]);
        assert!(matches!(
            listed.as_slice(),
            [Tab::Shell(_), Tab::Shell(_), Tab::Merge(_)]
        ));
    }

    #[test]
    fn given_a_merge_tab_when_the_tabs_are_listed_then_it_is_located_like_a_shell_tab() {
        // Given — situer un répertoire est le port de `pty`, et `merge` ne le connaît pas :
        // c'est ici que les deux se rencontrent, donc ici que ça se vérifie
        let merges = vec![merge_tab("01M")];

        // When
        let listed = join(Vec::new(), merges, &HereLocator);

        // Then
        let Some(Tab::Merge(merge)) = listed.first() else {
            panic!("l'onglet de merge doit être là");
        };
        assert_eq!(
            merge
                .location
                .as_ref()
                .map(|place| place.worktree_root.as_str()),
            Some("/dev/ash")
        );
    }

    #[test]
    fn given_a_merge_tab_when_it_crosses_the_boundary_then_it_carries_no_agent_state_at_all() {
        // Given — c'est l'invariant du ticket : `state`, `stateSince` et `paused` n'ont
        // aucun sens sans processus, et les remplir de valeurs neutres ferait apparaître un
        // `idle · 12m` sous un onglet où rien ne tourne
        let listed = join(Vec::new(), vec![merge_tab("01M")], &HereLocator);

        // When
        let json = serde_json::to_string(&listed).expect("la sérialisation doit tenir");

        // Then
        assert!(json.contains("\"kind\":\"merge\""));
        for absent in ["\"state\"", "\"stateSince\"", "\"paused\"", "\"cwd\""] {
            assert!(!json.contains(absent), "{absent} n'a rien à faire ici");
        }
    }
}
