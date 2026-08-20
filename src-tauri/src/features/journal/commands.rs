//! La surface de la feature vers le frontend : ce que le journal pèse, et sa purge.
//!
//! Deux commandes, aucun event. Le frontend ne connaît de `journal` que ces deux noms et la
//! fiche qui traverse ; l'attribution elle-même ne passe pas par ici — elle est lue en Rust
//! par [`super::CommitJournal::attribution`], que la colonne `by` du graphe consommera.
//!
//! **Ce que la fiche dit est composé ici**, pas dans l'écran : c'est la discipline du dépôt
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — la fenêtre rend une
//! phrase, elle ne la fabrique pas. Et la phrase qui compte, celle de la spec §10 sur les
//! prompts, ne doit pas pouvoir diverger d'un écran à l'autre.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};

use super::journal::{CommitJournal, JournalSummary};

/// Ce que la fenêtre de réglages montre du journal.
///
/// Elle ne montre **jamais** son contenu : le fichier contient des prompts, et un écran qui
/// les affiche est un écran de plus où ils passent. Elle en montre le poids, l'endroit, et
/// ce qu'il faut savoir avant de l'effacer.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct JournalReport {
    pub entries: usize,
    pub repos: usize,
    /// Ce que le journal pèse, en toutes lettres.
    pub summary: String,
    /// Pourquoi ce fichier existe, et ce qu'il ne fait pas (spec §10).
    pub note: String,
    /// Où il est. Un utilisateur qui lit « purgeable » sans savoir où regarder n'apprend que
    /// la moitié de ce qu'on lui promet.
    pub path: String,
}

impl JournalReport {
    fn of(summary: JournalSummary) -> Self {
        Self {
            entries: summary.entries,
            repos: summary.repos,
            summary: describe(summary),
            note: "the journal never leaves this machine. it is not synced, and nothing is \
                   sent anywhere."
                .to_owned(),
            path: "~/.ash/journal".to_owned(),
        }
    }
}

/// Le poids du journal, dit comme on le lirait.
///
/// L'état vide a sa propre phrase, et ce n'est pas de la coquetterie : « 0 commits » ferait
/// croire à une panne d'un dispositif qui, la plupart du temps, n'a simplement rien encore
/// observé.
fn describe(summary: JournalSummary) -> String {
    match (summary.entries, summary.repos) {
        (0, _) => "nothing recorded yet".to_owned(),
        (1, _) => "1 commit attributed".to_owned(),
        (entries, 1) => format!("{entries} commits attributed, in 1 repository"),
        (entries, repos) => format!("{entries} commits attributed, in {repos} repositories"),
    }
}

/// Ce que le journal a retenu — la fenêtre de réglages le demande en s'ouvrant.
///
/// **`async` volontairement**, comme `git_metadata` : Tauri exécute une commande synchrone
/// sur le fil de l'interface, et compter les entrées lit tous les fichiers du dossier.
#[tauri::command]
pub async fn journal_summary<R: Runtime>(app: AppHandle<R>) -> JournalReport {
    let journal = app.state::<Arc<CommitJournal>>();
    JournalReport::of(journal.summary())
}

/// Efface le journal — le geste explicite que la spec §10 exige.
///
/// Il n'a **pas** de premier temps annoncé, contrairement au retrait des hooks : celui-là
/// touche des fichiers de l'utilisateur, celui-ci n'emporte que ce qu'Ash a écrit dans son
/// propre dossier, et le compte de ce qui partira est sous les yeux de qui clique. Le
/// résultat est la fiche relue après coup, pas une fiche vide posée d'autorité — si un
/// fichier a résisté, l'écran doit le dire.
#[tauri::command]
pub async fn journal_purge<R: Runtime>(app: AppHandle<R>) -> JournalReport {
    let journal = app.state::<Arc<CommitJournal>>();
    JournalReport::of(journal.purge().unwrap_or_else(|_| journal.summary()))
}

/// De quoi lire les commits **sans retenir le fil de la surveillance**.
///
/// Le pendant exact de `git::commands::follow_worktrees`, et pour la même raison : le rappel
/// arrive sur le fil de FSEvents, et la lecture qui suit lance un processus `git`. Le tenir
/// là ferait attendre toutes les autres écritures observées du dépôt derrière lui.
///
/// Les demandes en attente sont **dédoublonnées**, jamais écrasées : deux worktrees peuvent
/// commiter dans la même rafale, et n'en garder qu'un perdrait une attribution — alors que
/// dix fois le même worktree ne rend que le même travail.
pub fn record_commits(
    journal: &Arc<CommitJournal>,
) -> impl Fn(&Path, &Path) + Send + Sync + 'static {
    let (sender, receiver) = std::sync::mpsc::channel::<(PathBuf, PathBuf)>();
    // Un `Weak` : ce fil observe le journal, il ne doit pas être ce qui le maintient en vie
    // après l'arrêt de l'application.
    let journal = Arc::downgrade(journal);

    std::thread::spawn(move || {
        while let Ok(first) = receiver.recv() {
            let mut pending = vec![first];
            while let Ok(more) = receiver.try_recv() {
                if !pending.contains(&more) {
                    pending.push(more);
                }
            }
            let Some(journal) = journal.upgrade() else {
                return;
            };
            for (worktree_root, common_dir) in pending {
                journal.on_head_moved(&worktree_root, &common_dir);
            }
        }
    });

    move |worktree_root: &Path, common_dir: &Path| {
        // Échouer à envoyer signifie que le fil est parti : il n'y a plus de journal à tenir.
        let _ = sender.send((worktree_root.to_owned(), common_dir.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_journal_that_has_seen_nothing_when_it_is_described_then_it_does_not_read_as_a_failure(
    ) {
        // Given — l'état de tout Ash au premier lancement, et de la plupart des sessions :
        // le journal n'a rien observé. « 0 commits » se lirait comme une panne.
        let empty = JournalSummary::default();

        // When
        let report = JournalReport::of(empty);

        // Then — et la promesse de la spec §10 est dite même quand il n'y a rien à effacer
        assert_eq!(report.summary, "nothing recorded yet");
        assert!(report.note.contains("not synced"));
        assert_eq!(report.path, "~/.ash/journal");
    }

    #[test]
    fn given_a_journal_of_several_repositories_when_it_is_described_then_the_count_says_what_a_purge_takes(
    ) {
        // Given — le compte est ce qui rend le clic explicite : il n'y a pas d'écran
        // d'annonce avant la purge, donc c'est cette phrase qui la précède.
        let summary = JournalSummary {
            entries: 12,
            repos: 3,
        };

        // When
        let report = JournalReport::of(summary);

        // Then
        assert_eq!(report.summary, "12 commits attributed, in 3 repositories");
        assert_eq!(report.entries, 12);
    }
}
