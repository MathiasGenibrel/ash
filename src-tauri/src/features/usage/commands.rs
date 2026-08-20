//! La surface de la feature vers le frontend : **une lecture, un event, et rien d'autre**.
//!
//! Le frontend ne connaît des quotas que ces deux noms et la forme d'[`AccountUsage`]. Il
//! **rend** deux couples ; il n'en détient aucun, ne sait pas d'où ils viennent, et n'a
//! aucun moyen de déclencher un appel
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! Le couple commande + event est celui du thème : la webview **lit une fois** en
//! s'affichant, puis c'est l'event qui la tient à jour. Elle ne redemande jamais — un rendu
//! qui redemanderait serait le chemin par lequel la condition 1 d'ADR-0016 se perdrait.
//!
//! ## L'interrupteur n'a pas de commande ici, et c'est voulu
//!
//! Il en avait deux — un `usage_polling` et un `usage_set_polling` — que personne n'appelait :
//! la fenêtre de réglages passe par `settings_usage` et `settings_set_usage_polling`, qui
//! rendent la **section recomposée**. Deux écritures pour un seul réglage, ce sont deux
//! chemins à tenir d'accord, et le second aurait laissé la section afficher une position que
//! le poller n'a plus — exactement la divergence que `token.rs` évite en tenant sa conduite et
//! sa lisibilité sous un seul verrou. Il y a donc **une seule écriture**, et elle rend ce que
//! le backend détient après coup.

use std::sync::Arc;

use super::poller::UsagePoller;
use super::quota::AccountUsage;

/// Nom de l'event qui porte les deux quotas. Contrat avec la ligne de statut.
pub const ACCOUNT_USAGE_EVENT: &str = "ash://account-usage";

/// Ce qu'Ash sait de l'usage du compte, lu par la webview en s'affichant.
///
/// **Une lecture, jamais un appel** : elle rend ce que le fil de fond a déjà trouvé, et
/// n'attend rien.
#[tauri::command]
pub fn usage_snapshot(state: tauri::State<'_, Arc<UsagePoller>>) -> AccountUsage {
    state.snapshot()
}
