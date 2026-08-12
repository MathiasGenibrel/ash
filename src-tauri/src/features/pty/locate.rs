//! Où un onglet se situe : le worktree qui le porte, et le dépôt qui le groupe.
//!
//! La hiérarchie d'[ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md) est
//! ce que la sidebar dessine, mais c'est le backend qui la détient
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : le frontend reçoit
//! un onglet **déjà situé**, il ne lit aucun fichier de contrôle git.
//!
//! La résolution elle-même appartient à `features::git`. Cette feature-ci n'en connaît que
//! le **port** ci-dessous, qu'elle possède : `pty` n'importe pas `git`, et `git` ne sait
//! rien des onglets. C'est le composition root qui les relie, exactement comme il relie la
//! sonde d'ADR-0005.

use std::path::Path;

/// Le dépôt commun sous lequel un worktree se range.
///
/// L'`id` est le dossier git commun : deux worktrees du même projet rendent la même
/// chaîne, et c'est par elle — pas par le nom, qui peut se répéter d'un disque à l'autre —
/// que la sidebar les groupe.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RepoRef {
    pub id: String,
    pub name: String,
}

/// La localisation d'un onglet, telle qu'elle traverse la frontière.
///
/// `repo` à `None` **est** la forme à plat d'ADR-0012 : un dépôt sans worktree lié, ou un
/// répertoire hors de tout dépôt. Le frontend n'a rien à en déduire — il rend un niveau ou
/// deux selon que ce champ est là.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TabLocation {
    /// La racine du worktree. C'est la clé de groupement des onglets.
    pub worktree_root: String,
    /// Le nom **brut** du dossier du worktree — la matière du suffixe `·sidebar`.
    pub worktree_name: String,
    pub repo: Option<RepoRef>,
}

/// Résout un répertoire en localisation. Le port que `pty` possède.
///
/// `None` veut dire « je ne sais pas situer ce répertoire » — un chemin illisible, un
/// fichier `.git` cassé, un worktree dont le dépôt a disparu. Ce n'est pas la même chose
/// qu'un répertoire **hors** dépôt, qui est un cas nominal et rend un `TabLocation` sans
/// `repo` : se taire sur un worktree cassé le ferait passer pour un dossier ordinaire.
pub trait WorktreeLocator: Send + Sync {
    fn locate(&self, cwd: &Path) -> Option<TabLocation>;
}
