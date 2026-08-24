use std::sync::Arc;

use tauri::{AppHandle, Runtime};

use super::question::QuitGate;

/// Nom de l'event qui porte la question, et les onglets qu'elle nomme.
///
/// Contrat avec `src/app/confirm-quit.ts` : une chaîne que rien ne vérifie à la
/// compilation, comme `ash://tab-changed` et `ash://select-tab`. Sa charge est un
/// `Vec<TabInfo>` — la fiche que le frontend connaît déjà —, et **pas** un type de plus :
/// la modale a besoin d'un chemin et d'un état par ligne, et les deux y sont.
///
/// Il ne porte aucun état d'agent nouveau et n'en est pas une seconde source (ADR-0009) :
/// c'est une **question**, posée à l'instant d'un geste, et le frontend la rend.
pub const CONFIRM_QUIT_EVENT: &str = "ash://confirm-quit";

/// L'utilisateur a répondu « quitter quand même ».
///
/// Elle ne ferme aucun onglet et ne signale aucun agent : quitter reste quitter (le hors
/// périmètre de l'issue #177). Elle ouvre le laissez-passer, puis demande à Tauri de sortir
/// — donc l'arrêt repasse par exactement le chemin d'avant cette tranche,
/// `RunEvent::ExitRequested` puis `RunEvent::Exit`, avec ses sondes éteintes, sa
/// surveillance `.git` arrêtée, son sondage de quotas arrêté et son fichier de socket retiré.
///
/// Il n'y a **pas** de commande symétrique pour « annuler » : annuler, c'est ne rien faire.
/// Le laissez-passer n'a pas été ouvert, l'application n'a rien perdu, et un second `⌘Q`
/// repose la question.
#[tauri::command]
pub fn quit_now<R: Runtime>(app: AppHandle<R>, gate: tauri::State<'_, Arc<QuitGate>>) {
    gate.open();
    app.exit(0);
}
