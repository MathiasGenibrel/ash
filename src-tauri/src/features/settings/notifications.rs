//! Ce que la fenêtre de réglages dit des notifications macOS (spec §8, dernière puce).
//!
//! La spec demande que « l'état *permission macOS non accordée* » soit visible, **avec le
//! chemin pour l'accorder**. Ce module produit la ligne entière — l'état, sa conséquence en
//! prose, et le chemin — parce que c'est du texte qui décrit une règle de produit, et qu'une
//! règle de produit ne se recopie pas dans une vue
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! Il est dans `settings` et non dans `agents` pour une raison de frontière : `agents`
//! n'expose **rien** au frontend et n'a pas de `commands.rs` — c'est écrit dans son mod-doc
//! et ça reste vrai après cette tranche. La fenêtre de réglages, elle, a déjà sa surface,
//! ses onze commandes et sa capacité. Ce qu'`agents` possède et que ce module lui demande
//! est la seule chose qui lui appartienne vraiment : **quels états interrompent**
//! ([`NOTIFIED_STATES`]).
//!
//! ## Pourquoi Ash ne sait pas si la permission est accordée
//!
//! `tauri-plugin-notification` 2.3.3 expose bien `permission_state()`, mais son
//! implémentation de bureau rend une **constante** — `Ok(PermissionState::Granted)`,
//! quoi que macOS en pense (`src/desktop.rs` du plugin). La brancher ferait afficher
//! « accordée » à un utilisateur qui a refusé, c'est-à-dire exactement le contraire de ce
//! que la puce de la spec demande. Lire la vraie autorisation demanderait
//! `UNUserNotificationCenter` — donc `objc`, donc de l'`unsafe` hors de `features/probe/`,
//! et une application empaquetée.
//!
//! Ash dit donc ce qu'il sait, et pas plus : [`NotificationPermission::Undisclosed`]. Les
//! deux autres valeurs ne sont pas décoratives — ce sont elles que la spec nomme, et la
//! ligne qu'elles produisent est prouvée ici. Le jour où une source existera, c'est
//! [`observed`] qui changera, et rien d'autre.

use crate::features::agents::{AgentState, NOTIFIED_STATES};

/// Ce que macOS laisse savoir à Ash de sa propre autorisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum NotificationPermission {
    Granted,
    /// Refusée : rien n'arrivera jamais, et c'est le cas que la spec §8 veut voir affiché.
    Denied,
    /// macOS ne le dit pas à Ash. **La seule valeur produite aujourd'hui** — voir le
    /// mod-doc.
    Undisclosed,
}

/// Où l'autorisation se donne, mot pour mot.
///
/// Une constante, et pas une phrase écrite dans la vue : c'est le seul élément *actionnable*
/// de la ligne, et le voir diverger du nom du panneau de macOS le rendrait inutile.
pub const GRANT_PATH: &str = "System Settings ▸ Notifications ▸ ash";

/// La section `notifications` de la fenêtre, en entier.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NotificationsReport {
    pub permission: NotificationPermission,
    /// La phrase de la ligne d'état.
    pub summary: String,
    /// Sa conséquence, en prose : ce que l'état coûte à l'utilisateur.
    pub note: String,
    /// Le chemin où l'accorder. Toujours présent — il vaut aussi pour vérifier un « oui ».
    pub path: String,
    /// Les états qui interrompent, tels qu'`agents` en décide (spec §8).
    ///
    /// Ils voyagent depuis le backend plutôt que d'être écrits dans la vue : ce sont eux qui
    /// décident d'une bannière, et une liste recopiée finirait par annoncer un état qu'Ash
    /// ne notifie plus.
    pub notified: Vec<AgentState>,
}

/// Ce qu'Ash observe aujourd'hui de son autorisation.
///
/// Une fonction, et pas une constante, pour que le jour où une source existe il n'y ait
/// qu'un corps à écrire — et un seul endroit à relire pour savoir d'où vient la réponse.
pub fn observed() -> NotificationPermission {
    NotificationPermission::Undisclosed
}

/// La ligne que la fenêtre affiche pour cette autorisation.
pub fn report(permission: NotificationPermission) -> NotificationsReport {
    let (summary, note) = match permission {
        NotificationPermission::Granted => (
            "macOS notifications are allowed",
            "ash interrupts you for a waiting agent and for a failed one, and only while it is in the background.",
        ),
        NotificationPermission::Denied => (
            "macOS notifications are not allowed",
            "an agent waiting for an answer while ash is behind another window will go unnoticed until you come back. grant them here:",
        ),
        NotificationPermission::Undisclosed => (
            "macOS doesn't tell ash whether notifications are allowed",
            "if nothing appears while ash is in the background and an agent is waiting, the permission is the first thing to check:",
        ),
    };

    NotificationsReport {
        permission,
        summary: summary.to_owned(),
        note: note.to_owned(),
        path: GRANT_PATH.to_owned(),
        notified: NOTIFIED_STATES.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_user_who_refused_the_permission_when_the_section_is_composed_then_it_says_what_it_costs_and_where_to_grant_it(
    ) {
        // Given — la dernière puce de la spec §8. Une notification qui n'arrive jamais est
        // indiscernable d'un agent qui n'attend pas : sans cette ligne, l'utilisateur
        // conclut qu'Ash ne marche pas, et le produit perd son critère de sortie.
        let permission = NotificationPermission::Denied;

        // When
        let shown = report(permission);

        // Then
        assert_eq!(shown.summary, "macOS notifications are not allowed");
        assert_eq!(shown.path, "System Settings ▸ Notifications ▸ ash");
        assert!(shown.note.contains("will go unnoticed"));
    }

    #[test]
    fn given_a_permission_macos_does_not_disclose_when_the_section_is_composed_then_ash_claims_nothing(
    ) {
        // Given — le cas **réel** aujourd'hui : le plugin rend une constante `Granted` sur
        // le bureau. Afficher « accordée » sur cette foi-là serait affirmer à l'utilisateur
        // l'exact contraire de ce qu'il constate, et c'est la seule faute que cette ligne
        // pourrait commettre.
        let permission = observed();

        // When
        let shown = report(permission);

        // Then
        assert_eq!(permission, NotificationPermission::Undisclosed);
        assert_ne!(
            shown.summary,
            report(NotificationPermission::Granted).summary
        );
        assert_eq!(shown.path, GRANT_PATH);
    }

    #[test]
    fn given_the_section_when_it_names_what_interrupts_then_it_names_exactly_what_agents_notifies()
    {
        // Given — les deux états de la spec §8, et pas trois. La fenêtre les **rend** ; les
        // écrire une seconde fois ici ferait promettre `done` le jour où quelqu'un
        // l'ajouterait à `agents`, ou l'inverse.
        let shown = report(observed());

        // When
        let named = shown.notified;

        // Then
        assert_eq!(named, vec![AgentState::Waiting, AgentState::Error]);
    }
}
