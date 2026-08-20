//! La surface de la feature vers le frontend : trois commandes, aucun event.
//!
//! Aucune n'écrit sans qu'on le lui demande. La fiche se **lit** en ouvrant le panneau ;
//! elle ne s'écrit que sur le bouton, et le diff de ce qui changerait est sous les yeux de
//! qui clique (spec §10). C'est le même parti que l'installation des hooks : Ash n'écrit
//! chez l'utilisateur que sur un geste, jamais au fil d'une lecture.
//!
//! Les trois sont **`async`**, comme `git_metadata` et `journal_summary` : Tauri exécute une
//! commande synchrone sur le fil de l'interface, et chacune lit — parfois écrit — un fichier.

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};

use super::card::{BranchCard, Cards};
use super::place::CardMode;

/// La fiche de ce worktree — ce que le panneau montre en s'ouvrant.
///
/// `None` pour un worktree dont la fiche ne se lit pas : un disque en erreur, une
/// permission. L'écran affiche alors la même chose que pour un worktree sans fiche, ce qui
/// est vrai des deux points de vue — il n'y a rien à montrer.
#[tauri::command]
pub async fn branch_card<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
) -> Option<BranchCard> {
    let cards = app.state::<Arc<Cards>>();
    cards.read(Path::new(&worktree_root)).ok()
}

/// Pose le journal dans le bloc `ash:log`, ou refuse en le disant.
///
/// Rend la fiche **relue après coup**, refus compris : c'est elle qui porte l'état du bloc,
/// la phrase, et le diff. Un `None` est une panne de lecture, pas un refus — les refus se
/// racontent dans `log.state`.
#[tauri::command]
pub async fn branch_card_write_log<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
) -> Option<BranchCard> {
    let cards = app.state::<Arc<Cards>>();
    cards.write_log(Path::new(&worktree_root)).ok()
}

/// Choisit où vit la fiche de ce worktree (ADR-0013, mode local).
///
/// `local: null` efface le choix et rend la main à la détection. **Rien n'est déplacé,
/// rien n'est effacé, et aucun `.gitignore` n'est écrit.**
#[tauri::command]
pub async fn branch_card_place<R: Runtime>(
    app: AppHandle<R>,
    worktree_root: String,
    local: Option<bool>,
) -> Option<BranchCard> {
    let cards = app.state::<Arc<Cards>>();
    let mode = local.map(|local| {
        if local {
            CardMode::Local
        } else {
            CardMode::Repo
        }
    });
    cards.choose(Path::new(&worktree_root), mode).ok()
}
