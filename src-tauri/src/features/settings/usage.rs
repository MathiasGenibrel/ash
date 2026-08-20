//! Ce que la fenêtre de réglages dit des quotas : **qu'Ash appelle, et comment l'en
//! empêcher** (ADR-0016, condition 3, second sens).
//!
//! « L'utilisateur doit pouvoir savoir qu'Ash appelle, et **le couper**. Un interrupteur dans
//! la fenêtre de réglages, détenu par la feature concernée et persisté comme les trois de la
//! spec §9. Il existe **dès la première fonctionnalité réseau**, pas au jour où quelqu'un le
//! demande. » Cette section est cette phrase, et elle porte trois choses :
//!
//! - **l'interrupteur**, dont la position vient de `features::usage` et y retourne. Un
//!   interrupteur que personne ne peut basculer ne tient pas le critère ;
//! - **l'issue de la dernière lecture du trousseau**, que les conséquences d'ADR-0017
//!   exigent nommément — « la fenêtre de réglages doit donc pouvoir dire *le jeton n'est pas
//!   lisible* ». Sans elle, un refus, un item absent et une panne sont indiscernables, et
//!   c'est le prix du « en échec, la valeur disparaît » ;
//! - **la limite des deux comptes**, elle aussi demandée par ADR-0017 : le trousseau ne porte
//!   qu'un jeton, et Ash n'a aucun moyen de savoir de quel compte il vient (ADR-0007 prévoit
//!   `claude` et `claude-perso`). « Afficher un quota en le rattachant au mauvais compte
//!   serait pire que de ne rien rattacher du tout » — la section le dit donc, plutôt que de
//!   le résoudre.
//!
//! ## Rien n'est lu ici, et c'est la condition 1 d'ADR-0016
//!
//! **Composer cette section ne lit aucun trousseau et n'appelle aucun hôte.** Elle rapporte
//! ce que le fil de fond sait déjà : la position de l'interrupteur, et l'issue de la
//! **dernière** lecture. Une section qui irait chercher ferait surgir un dialogue macOS à
//! l'ouverture d'un panneau — c'est-à-dire sur un chemin de rendu, ce que la condition 1
//! interdit.
//!
//! C'est la seule différence de fond avec la section `notifications`, qui, elle, **relit**
//! l'autorisation macOS à chaque affichage : celle-là est une question bornée à deux
//! secondes et sans conséquence, celle-ci peut attendre un clic humain.
//!
//! ## Il est dans `settings` et non dans `usage`
//!
//! Même frontière que pour `notifications.rs` : `features::usage` détient les faits — un
//! booléen, une lisibilité, une adresse —, et cette feature-ci détient la **fenêtre**. Ce qui
//! s'écrit ici est de la prose qui décrit une règle de produit, et une règle de produit ne se
//! recopie pas dans une vue
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).

use crate::features::usage::{Readability, USAGE_ENDPOINT};

/// Où l'autorisation du trousseau se reprend, mot pour mot.
///
/// Une constante, et pas une phrase écrite dans la vue : c'est le seul élément *actionnable*
/// de la section, et le voir diverger du nom réel de l'item le rendrait inutile.
pub const KEYCHAIN_PATH: &str = "Keychain Access ▸ login ▸ Claude Code-credentials";

/// La section `usage` de la fenêtre, en entier.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    /// L'interrupteur d'ADR-0016. Sa position vient de `features::usage`, et y retourne.
    pub polling: bool,
    /// L'issue de la dernière lecture du trousseau — **un souvenir, pas une question**.
    pub token: Readability,
    /// La phrase de la ligne d'état du jeton.
    pub summary: String,
    /// Sa conséquence, en prose : ce que l'état coûte à l'utilisateur.
    pub note: String,
    /// Où reprendre l'autorisation. Toujours présent — il vaut aussi pour vérifier un « oui ».
    pub path: String,
    /// **L'hôte qu'Ash appelle, en toutes lettres.** C'est la première moitié de la condition
    /// 3 : savoir qu'Ash appelle, avant de pouvoir le couper. Il vient de la constante de
    /// `features::usage`, jamais d'une chaîne recopiée — une adresse affichée qui ne serait
    /// pas celle qu'on appelle serait pire que pas d'adresse du tout.
    pub endpoint: String,
    /// La limite des deux comptes (ADR-0017, conséquences).
    pub accounts: String,
}

/// La section entière : l'interrupteur, la ligne du jeton, et les deux mises en garde.
#[must_use]
pub fn report(polling: bool, token: Readability) -> UsageReport {
    let (summary, note) = said(token);
    UsageReport {
        polling,
        token,
        summary: summary.to_owned(),
        note: note.to_owned(),
        path: KEYCHAIN_PATH.to_owned(),
        endpoint: USAGE_ENDPOINT.to_owned(),
        accounts: ACCOUNTS.to_owned(),
    }
}

/// La limite qu'ADR-0017 demande de documenter plutôt que de résoudre.
const ACCOUNTS: &str =
    "the keychain holds one token. if you sign in with more than one account, the quotas are \
     those of whichever one wrote it last — ash has no way to tell which, and would rather \
     say nothing than name the wrong one.";

/// Ce que chaque issue de lecture raconte, et ce qu'elle coûte.
///
/// **Totale sur les cinq variantes** : une sixième ne compilerait pas tant que personne
/// n'aurait dit ce que sa ligne raconte. C'est ce qui empêche un état d'apparaître dans la
/// fenêtre sans phrase, c'est-à-dire de disparaître en silence — exactement ce que les
/// conséquences d'ADR-0017 demandent d'éviter.
fn said(token: Readability) -> (&'static str, &'static str) {
    match token {
        // Ash vient de démarrer, ou sa fenêtre n'a pas encore été devant. Ce n'est pas une
        // panne, et le dire comme telle enverrait chercher un problème qui n'existe pas.
        Readability::Untried => (
            "ash hasn't asked the keychain yet",
            "the quotas are read while ash is the window in front of you, and never while it is behind another one. macOS will ask you once, for this item:",
        ),
        Readability::Readable => (
            "ash can read claude code's token",
            "it is read when ash calls, kept in memory, and never written anywhere. revoke it whenever you like, here:",
        ),
        // Aucun dialogue n'est apparu : il n'y a rien à autoriser, seulement à se connecter.
        Readability::Absent => (
            "no claude code token in the keychain",
            "sign in to the tool and the quotas will appear on their own. nothing is wrong, and there is nothing to grant — the item would live here:",
        ),
        // La condition 4 d'ADR-0017 : définitif. La fenêtre le dit plutôt que de laisser
        // croire qu'un rechargement suffirait.
        Readability::Refused => (
            "the keychain did not give up claude code's token",
            "ash asked once and will not ask again — quotas stay empty for this session. allow it again here, then restart ash:",
        ),
        // Le cas que les conséquences d'ADR-0017 disent devoir être nommable : « le jour où
        // l'item change de nom ou de forme, les quotas disparaissent en silence ».
        Readability::Unreadable => (
            "claude code's token is not in the shape ash expects",
            "the item is there, but what it holds has changed. the quotas stay empty until ash learns the new shape — nothing you can do from here:",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_user_who_refused_the_keychain_when_the_section_is_composed_then_it_says_it_is_final_and_where_to_take_it_back(
    ) {
        // Given — les conséquences d'ADR-0017 : sans cette ligne, l'utilisateur n'a aucun
        // moyen de distinguer un refus, un item absent et une panne. Les trois donnent le
        // même écran vide, et c'est exactement le prix du « en échec, la valeur disparaît »
        let refused = Readability::Refused;

        // When
        let shown = report(true, refused);

        // Then — et le mot « restart » compte : la condition 4 rend le refus définitif pour
        // la session, donc rouvrir la fenêtre ne suffirait pas
        assert_eq!(
            shown.summary,
            "the keychain did not give up claude code's token"
        );
        assert!(shown.note.contains("will not ask again"));
        assert!(shown.note.contains("restart ash"));
        assert_eq!(shown.path, KEYCHAIN_PATH);
    }

    #[test]
    fn given_the_five_ways_a_read_can_end_when_the_section_is_composed_then_each_one_says_something_of_its_own(
    ) {
        // Given — une phrase partagée par deux issues ferait mentir l'une des deux : « rien
        // tenté » n'est pas « en panne », et « pas connecté » n'est pas « refusé ». C'est la
        // seule chose que cette section doive à son utilisateur
        let endings = [
            Readability::Untried,
            Readability::Readable,
            Readability::Absent,
            Readability::Refused,
            Readability::Unreadable,
        ];

        // When
        let said: Vec<String> = endings
            .iter()
            .map(|token| report(true, *token).summary)
            .collect();

        // Then
        let distinct: std::collections::BTreeSet<&String> = said.iter().collect();
        assert_eq!(
            distinct.len(),
            endings.len(),
            "deux issues disent la même chose : {said:?}"
        );
    }

    #[test]
    fn given_a_user_who_has_not_had_the_window_in_front_yet_when_the_section_is_composed_then_it_is_not_reported_as_a_failure(
    ) {
        // Given — la condition 2 d'ADR-0016 fait qu'aucune lecture ne part tant qu'Ash est
        // derrière une autre fenêtre. Un « jeton illisible » affiché là-dessus serait faux,
        // et enverrait chercher un problème qui n'existe pas
        let untried = Readability::Untried;

        // When
        let shown = report(true, untried);

        // Then
        assert!(!shown.summary.contains("not"));
        assert!(shown.note.contains("in front of you"));
    }

    #[test]
    fn given_the_section_when_it_names_the_host_ash_calls_then_it_is_the_one_the_code_calls() {
        // Given — la première moitié de la condition 3 d'ADR-0016 : savoir qu'Ash appelle,
        // avant de pouvoir le couper. Une adresse recopiée à la main dans la fenêtre finirait
        // par ne plus être celle qu'on appelle, et l'écran mentirait sur ce qui sort de la
        // machine
        let shown = report(true, Readability::Readable);

        // When / Then
        assert_eq!(shown.endpoint, USAGE_ENDPOINT);
        assert!(shown.endpoint.contains("api.anthropic.com"));
    }

    #[test]
    fn given_a_user_with_two_claude_accounts_when_the_section_is_composed_then_it_admits_it_cannot_tell_which(
    ) {
        // Given — ADR-0007 prévoit deux dossiers de configuration, donc deux comptes ; le
        // trousseau, lui, ne porte qu'un jeton. « Afficher un quota en le rattachant au
        // mauvais compte serait pire que de ne rien rattacher du tout »
        let shown = report(true, Readability::Readable);

        // When / Then
        assert!(shown.accounts.contains("one token"));
        assert!(shown.accounts.contains("no way to tell which"));
    }

    #[test]
    fn given_a_user_who_cut_the_calls_when_the_section_is_composed_then_it_shows_his_choice_and_not_the_default(
    ) {
        // Given — la seule chose qu'un écran de réglages doive à son utilisateur : montrer ce
        // qui est réglé. Un interrupteur qui se redessinerait à son défaut ferait croire à un
        // réglage perdu, et le ferait rejouer — donc rallumer les appels qu'il a coupés
        let cut = report(false, Readability::Readable);
        let calling = report(true, Readability::Readable);

        // When / Then
        assert!(!cut.polling);
        assert!(calling.polling);
    }
}
