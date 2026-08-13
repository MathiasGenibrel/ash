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
//! ## D'où vient la permission, maintenant
//!
//! De `UNUserNotificationCenter`, par le port [`Banners`] de `features::notifications` —
//! c'est-à-dire du **même** centre que celui qui pose les bannières. C'était la condition
//! pour que cette ligne ne mente pas : `tauri-plugin-notification`, qui portait les
//! bannières jusqu'ici, rend `Granted` en dur sur le bureau, donc afficher « accordée » sur
//! sa foi aurait dit à un utilisateur ayant refusé l'exact contraire de ce qu'il constate.
//!
//! Il reste un cas où Ash ne sait rien, et il est franc : hors d'une application empaquetée
//! — `bun run tauri dev` —, macOS n'a pas de centre de notifications pour Ash. C'est ce que
//! [`NotificationPermission::Undisclosed`] dit, et c'est désormais tout ce qu'il dit.

use crate::features::agents::{AgentState, NOTIFIED_STATES};
use crate::features::notifications::Authorization;

/// Ce que macOS laisse savoir à Ash de sa propre autorisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum NotificationPermission {
    Granted,
    /// Refusée : rien n'arrivera jamais, et c'est le cas que la spec §8 veut voir affiché.
    Denied,
    /// macOS ne le dit pas à Ash — il n'a pas de centre de notifications à lui offrir, faute
    /// d'application empaquetée. Voir le mod-doc.
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

/// Ce qu'Ash observe de son autorisation, traduit pour la fenêtre.
///
/// Deux vocabulaires plutôt qu'un, et c'est la frontière qui l'impose :
/// [`Authorization`] est ce que macOS répond, [`NotificationPermission`] est ce que le
/// contrat TypeScript porte. Les confondre ferait voyager jusqu'à la webview un mot dont
/// le sens vient d'un `enum` d'Apple.
pub fn observed(authorization: Authorization) -> NotificationPermission {
    match authorization {
        Authorization::Granted => NotificationPermission::Granted,
        Authorization::Denied => NotificationPermission::Denied,
        Authorization::Undisclosed => NotificationPermission::Undisclosed,
    }
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
    fn given_a_macos_that_refuses_to_say_when_the_section_is_composed_then_ash_claims_nothing() {
        // Given — hors d'une application empaquetée, macOS n'a pas de centre de
        // notifications pour Ash. Affirmer « accordée » sur cette absence-là serait dire à
        // l'utilisateur l'exact contraire de ce qu'il constate, et c'est la seule faute que
        // cette ligne pourrait commettre — celle que le `Granted` constant du plugin
        // commettait.
        let permission = observed(Authorization::Undisclosed);

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
    fn given_a_user_who_turned_ash_off_in_system_settings_when_the_section_is_composed_then_the_window_says_refused(
    ) {
        // Given — la valeur qu'aucune pile du projet n'avait su produire jusqu'ici : le
        // plugin rendait `Granted` en dur, donc `Denied` était un cas mort, et la puce de la
        // spec §8 une promesse sans producteur. C'est ce test qui dit qu'elle en a un.
        let refused = Authorization::Denied;

        // When
        let shown = report(observed(refused));

        // Then
        assert_eq!(shown.permission, NotificationPermission::Denied);
        assert_eq!(shown.summary, "macOS notifications are not allowed");
    }

    #[test]
    fn given_the_section_when_it_names_what_interrupts_then_it_names_exactly_what_agents_notifies()
    {
        // Given — les deux états de la spec §8, et pas trois. La fenêtre les **rend** ; les
        // écrire une seconde fois ici ferait promettre `done` le jour où quelqu'un
        // l'ajouterait à `agents`, ou l'inverse.
        let shown = report(observed(Authorization::Granted));

        // When
        let named = shown.notified;

        // Then
        assert_eq!(named, vec![AgentState::Waiting, AgentState::Error]);
    }
}
