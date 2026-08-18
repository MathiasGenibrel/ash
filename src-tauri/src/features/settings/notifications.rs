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
//! ses commandes et sa capacité. Ce qu'`agents` possède et que ce module lui demande sont
//! les deux seules choses qui lui appartiennent vraiment : **quels états peuvent
//! interrompre** ([`SWITCHABLE_STATES`]), et **lesquels l'utilisateur laisse passer**
//! ([`NotificationChoices`]).
//!
//! ## Les trois interrupteurs
//!
//! Ce module ne les détient pas : il les **rend**, un par état de [`SWITCHABLE_STATES`], avec
//! sa position telle que `agents` la garde. Le geste, lui, repart à `agents` par
//! `settings_set_notification` — la fenêtre demande, elle n'applique pas
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). C'est ce qui fait
//! qu'éteindre `waiting` coupe vraiment la bannière au lieu de la masquer une fois arrivée :
//! le filtre est sur le chemin qui poste, dans le superviseur.
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

use crate::features::agents::{AgentState, NotificationChoices, SWITCHABLE_STATES};
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
    /// Les trois interrupteurs de la spec §9, dans l'ordre de la spec §8.
    ///
    /// Ils voyagent depuis le backend plutôt que d'être écrits dans la vue : ce sont eux qui
    /// décident d'une bannière, et une liste recopiée finirait par offrir un interrupteur qui
    /// ne commande rien — ou par en cacher un qui commande encore.
    pub switches: Vec<NotificationSwitch>,
}

/// Un interrupteur : l'état qu'il commande, sa position, et ce qu'il promet.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NotificationSwitch {
    pub state: AgentState,
    pub enabled: bool,
    /// Ce que l'état veut dire, en quelques mots — la colonne « Événement » du design.
    ///
    /// Ici et non dans la vue, pour la raison qui vaut pour toute cette section : c'est une
    /// phrase qui décrit une règle de produit, et une règle de produit ne se recopie pas dans
    /// un écran.
    pub means: String,
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

/// La section entière : la ligne d'autorisation, et les trois interrupteurs.
pub fn report(
    permission: NotificationPermission,
    choices: NotificationChoices,
) -> NotificationsReport {
    let (summary, note) = match permission {
        NotificationPermission::Granted => (
            "macOS notifications are allowed",
            // Aucun état n'est nommé ici : les interrupteurs disent lesquels, et l'utilisateur
            // en change. Une phrase qui promettrait « waiting et error » mentirait à la
            // seconde où il en éteint un.
            "banners arrive only while ash is in the background, and only for what is switched on below.",
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
        switches: SWITCHABLE_STATES
            .iter()
            .map(|state| NotificationSwitch {
                state: *state,
                enabled: choices.allows(*state),
                means: means(*state).to_owned(),
            })
            .collect(),
    }
}

/// Ce que chaque interrupteur promet, mot pour mot depuis le design.
///
/// Le `match` est exhaustif : un sixième état d'agent ne compilerait pas tant que personne
/// n'aurait dit ce que sa ligne raconte. Les deux états sans interrupteur n'apparaissent
/// jamais dans la section — [`SWITCHABLE_STATES`] ne les porte pas — mais le type, lui, les
/// contient.
fn means(state: AgentState) -> &'static str {
    match state {
        AgentState::Waiting => "an agent is waiting for an answer",
        AgentState::Error => "an agent failed",
        AgentState::Done => "an agent finished",
        AgentState::Idle => "an agent is idle",
        AgentState::Working => "an agent is working",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : les interrupteurs tels qu'Ash sort de sa boîte (spec §8).
    fn out_of_the_box() -> NotificationChoices {
        NotificationChoices::default()
    }

    /// La position de l'interrupteur de cet état, dans la section composée.
    fn switch(shown: &NotificationsReport, state: AgentState) -> bool {
        shown
            .switches
            .iter()
            .find(|switch| switch.state == state)
            .map(|switch| switch.enabled)
            .unwrap_or_else(|| panic!("{state:?} a un interrupteur"))
    }

    #[test]
    fn given_a_user_who_refused_the_permission_when_the_section_is_composed_then_it_says_what_it_costs_and_where_to_grant_it(
    ) {
        // Given — la dernière puce de la spec §8. Une notification qui n'arrive jamais est
        // indiscernable d'un agent qui n'attend pas : sans cette ligne, l'utilisateur
        // conclut qu'Ash ne marche pas, et le produit perd son critère de sortie.
        let permission = NotificationPermission::Denied;

        // When
        let shown = report(permission, out_of_the_box());

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
        let shown = report(permission, out_of_the_box());

        // Then
        assert_eq!(permission, NotificationPermission::Undisclosed);
        assert_ne!(
            shown.summary,
            report(NotificationPermission::Granted, out_of_the_box()).summary
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
        let shown = report(observed(refused), out_of_the_box());

        // Then
        assert_eq!(shown.permission, NotificationPermission::Denied);
        assert_eq!(shown.summary, "macOS notifications are not allowed");
    }

    #[test]
    fn given_a_fresh_install_when_the_section_is_composed_then_it_offers_the_three_switches_of_the_spec_at_their_defaults(
    ) {
        // Given — le tableau du design, tel quel : `waiting` et `error` allumés, `done`
        // éteint. C'est ce que l'écran promet, et il ne l'écrit nulle part lui-même — la
        // fenêtre rendrait volontiers trois cases cochées si le backend se taisait
        let shown = report(observed(Authorization::Granted), out_of_the_box());

        // When
        let offered: Vec<AgentState> = shown.switches.iter().map(|switch| switch.state).collect();

        // Then
        assert_eq!(
            offered,
            vec![AgentState::Waiting, AgentState::Error, AgentState::Done]
        );
        assert!(switch(&shown, AgentState::Waiting));
        assert!(switch(&shown, AgentState::Error));
        assert!(!switch(&shown, AgentState::Done));
    }

    #[test]
    fn given_a_user_who_turned_waiting_off_and_done_on_when_the_section_is_composed_then_it_shows_his_choice_and_not_the_defaults(
    ) {
        // Given — c'est la seule chose qu'un écran de réglages doive à son utilisateur :
        // montrer ce qui est réglé. Un interrupteur qui se redessinerait à son défaut ferait
        // croire à un réglage perdu, et le ferait rejouer — donc rallumer ce qu'il a éteint
        let chosen = out_of_the_box()
            .with(AgentState::Waiting, false)
            .with(AgentState::Done, true);

        // When
        let shown = report(observed(Authorization::Granted), chosen);

        // Then
        assert!(!switch(&shown, AgentState::Waiting));
        assert!(switch(&shown, AgentState::Done));
    }

    #[test]
    fn given_a_user_who_muted_a_state_when_the_permission_line_is_read_then_it_promises_nothing_it_no_longer_does(
    ) {
        // Given — la ligne « autorisation accordée » nommait les deux états qui
        // interrompent. Depuis qu'ils s'éteignent, une phrase qui les nomme est un mensonge
        // à un interrupteur près, et c'est le genre de mensonge qu'on ne remarque qu'en
        // n'étant pas prévenu
        let muted = out_of_the_box().with(AgentState::Waiting, false);

        // When
        let shown = report(NotificationPermission::Granted, muted);

        // Then
        assert!(!shown.note.contains("waiting"));
        assert!(shown.note.contains("switched on below"));
    }
}
