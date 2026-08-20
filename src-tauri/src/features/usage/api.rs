//! **La seule destination réseau du dépôt**, et le port qui la rend remplaçable.
//!
//! [ADR-0016](../../../../docs/adr/0016-ash-sort-sur-le-reseau.md), condition 4 : « il n'y
//! a pas de client HTTP générique offert au reste du code. Chaque appel nomme son hôte dans
//! le code de la feature qui en a besoin, et une feature qui n'a pas de raison d'appeler
//! n'a aucun moyen de le faire. » [`USAGE_ENDPOINT`] est cette adresse, et [`UsageApi`] est
//! la seule porte par laquelle le reste de la feature l'atteint. Rien d'autre du crate ne
//! nomme `ureq`.
//!
//! ## La frontière, en une phrase
//!
//! **Le jeton ne peut partir que vers l'hôte nommé ici** (condition 2 d'ADR-0017). Trois
//! réglages en font une propriété du code plutôt qu'une intention :
//!
//! | Réglage | Ce qu'il empêche |
//! |---|---|
//! | `max_redirects(0)` | qu'une réponse `302` fasse suivre l'en-tête `Authorization` vers un hôte que personne n'a nommé. C'est la protection qui compte ici : sans elle, l'hôte — ou ce qui se fait passer pour lui — choisirait la destination du secret |
//! | `https_only(true)` | qu'une redirection ou une URL mal composée fasse partir le jeton en clair |
//! | Une [`UsageError`] sans champ | qu'un corps de réponse ou une URL entre dans une trace. Voir `error.rs` |
//!
//! ## L'adresse, et d'où elle vient
//!
//! Elle n'a **pas** été devinée : elle est lue dans `ccstatusline`
//! (`src/utils/usage-fetch.ts`), l'outil que l'utilisateur emploie aujourd'hui et dont
//! ADR-0016 dit qu'il « appelle l'API directement ». L'en-tête `anthropic-beta` en vient
//! aussi — sans lui, l'hôte ne reconnaît pas une authentification OAuth d'abonnement.
//!
//! ## Ce que la condition 1 impose à ce module
//!
//! L'appel est **bloquant**, et c'est la propriété voulue : il vit sur le fil de fond du
//! poller, que personne n'attend. Aucun rendu, aucune passe de sonde, aucun hook n'a de
//! chemin jusqu'ici — [`UsagePoller`](super::UsagePoller) est le seul appelant, et il
//! n'expose que des lectures de ce qu'il a déjà.

use std::time::Duration;

use super::error::UsageError;
use super::quota::{read_usage, AccountUsage};
use super::token::AccessToken;

/// L'unique adresse réseau du dépôt. Voir l'en-tête.
pub const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";

/// Ce que l'hôte veut voir pour reconnaître une authentification d'abonnement.
const OAUTH_BETA: (&str, &str) = ("anthropic-beta", "oauth-2025-04-20");

/// Au-delà, on renonce — et les quotas disparaissent jusqu'au prochain tour.
///
/// Dix secondes : l'appel ne bloque rien de visible, mais un fil qui dort sur une socket
/// morte est un fil qui ne fera pas l'appel de la minute suivante.
const GIVE_UP_AFTER: Duration = Duration::from_secs(10);

/// D'où viennent les deux quotas.
///
/// Un trait, pour la raison qui vaut pour tous les effets système du dépôt, et une de plus
/// qui lui est propre : **aucun `cargo test` ne doit sortir sur le réseau**, ni dépendre de
/// ce qu'un compte a consommé le jour où il tourne.
pub trait UsageApi: Send + Sync {
    fn fetch(&self, token: &AccessToken) -> Result<AccountUsage, UsageError>;
}

/// L'hôte d'Anthropic, par `ureq`.
pub struct AnthropicUsage {
    agent: ureq::Agent,
}

impl Default for AnthropicUsage {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicUsage {
    #[must_use]
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            // Voir le tableau de l'en-tête : c'est ce qui empêche l'hôte de choisir où le
            // jeton part ensuite.
            .max_redirects(0)
            .https_only(true)
            .timeout_global(Some(GIVE_UP_AFTER))
            // Un statut d'erreur est une **réponse**, pas un incident de transport : sans
            // ceci, `ureq` transforme un 401 en `Err` et on ne pourrait plus distinguer
            // « le jeton a expiré » de « la machine est hors ligne ».
            .http_status_as_error(false)
            // Les racines de la plateforme, et non celles de Mozilla compilées dans le
            // binaire : un poste sous MDM ou derrière un proxy qui re-signe doit
            // fonctionner sans qu'Ash ait son propre avis (ADR-0016). Le choix se fait
            // **ici et à la compilation** : sans la fonctionnalité `platform-verifier`,
            // cette ligne ne serait pas une préférence mais une panique au premier appel —
            // voir le commentaire d'`ureq` dans `Cargo.toml`.
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .build();
        Self {
            agent: config.new_agent(),
        }
    }
}

impl UsageApi for AnthropicUsage {
    /// Un `GET`, et rien d'autre.
    ///
    /// `HTTPS_PROXY` est honoré sans qu'on le demande : la configuration par défaut d'`ureq`
    /// lit `ALL_PROXY`, `HTTPS_PROXY` et `https_proxy` de l'environnement, et
    /// `Agent::config_builder()` part de ce défaut.
    fn fetch(&self, token: &AccessToken) -> Result<AccountUsage, UsageError> {
        let answered = self
            .agent
            .get(USAGE_ENDPOINT)
            // Le seul endroit du crate qui lise le jeton en clair.
            .header("Authorization", &format!("Bearer {}", token.expose()))
            .header(OAUTH_BETA.0, OAUTH_BETA.1)
            .call()
            // L'erreur d'`ureq` sait rendre l'URL appelée : elle est jetée ici, elle
            // n'entre pas dans `UsageError`.
            .map_err(|_| UsageError::Unreachable)?;

        let status = answered.status().as_u16();
        if let Some(refused) = refusal(status) {
            return Err(refused);
        }

        let body = answered
            .into_body()
            .read_to_string()
            .map_err(|_| UsageError::Unreachable)?;
        read_usage(&body)
    }
}

/// Ce qu'un statut veut dire pour la suite, ou `None` s'il porte une réponse.
///
/// Séparée de l'appel pour être relue par un test : c'est elle qui décide si le jeton sera
/// relu au tour suivant, et se tromper de côté voudrait dire soit reposer la question du
/// trousseau à chaque `429`, soit ne jamais rattraper un jeton expiré.
fn refusal(status: u16) -> Option<UsageError> {
    match status {
        200..=299 => None,
        // Le jeton d'accès a expiré, ou a été révoqué. C'est le seul cas qui vaille une
        // relecture du trousseau.
        401 | 403 => Some(UsageError::Unauthorized),
        _ => Some(UsageError::Rejected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_host_that_refuses_the_token_when_the_status_is_read_then_only_that_case_asks_for_a_fresh_one(
    ) {
        // Given — un jeton OAuth expire toutes les heures, et l'outil en écrit un neuf.
        // Confondre cette réponse-là avec un `429` reposerait la question du trousseau à
        // chaque limitation de débit ; la confondre dans l'autre sens laisserait les
        // quotas éteints jusqu'au redémarrage d'Ash
        let statuses = [200, 204, 401, 403, 429, 500, 302];

        // When
        let read: Vec<Option<UsageError>> = statuses.iter().map(|s| refusal(*s)).collect();

        // Then — le 302 compte : `max_redirects(0)` le rend visible ici plutôt que de
        // laisser l'en-tête `Authorization` suivre un `Location`
        assert_eq!(
            read,
            vec![
                None,
                None,
                Some(UsageError::Unauthorized),
                Some(UsageError::Unauthorized),
                Some(UsageError::Rejected),
                Some(UsageError::Rejected),
                Some(UsageError::Rejected),
            ]
        );
    }

    #[test]
    fn given_the_only_network_destination_of_the_repository_when_it_is_read_then_it_names_one_host_over_tls(
    ) {
        // Given — la condition 4 d'ADR-0016 : une destination nommée par besoin. Ce test
        // est ce qui fera tomber la construction le jour où quelqu'un composera l'adresse
        // à partir d'une valeur venue d'ailleurs — d'un fichier, d'un réglage, d'une
        // réponse précédente
        let endpoint = USAGE_ENDPOINT;

        // When
        let host = endpoint.strip_prefix("https://").and_then(|rest| {
            rest.split_once('/')
                .map(|(host, path)| (host.to_owned(), format!("/{path}")))
        });

        // Then
        assert_eq!(
            host,
            Some((
                String::from("api.anthropic.com"),
                String::from("/api/oauth/usage")
            ))
        );
    }
}
