//! Des doublures d'usage et de trousseau, **dans le build de développement seulement**.
//!
//! Ce module entier vit sous `#[cfg(debug_assertions)]` : il n'est pas dans le binaire que
//! `bun run package` produit — pas « il ne s'y exécute pas », il ne s'y **compile pas**.
//! C'est le même interrupteur que celui qui sépare Ash d'Ash-dev, et `tauri build` l'éteint.
//!
//! ## Pourquoi il existe
//!
//! Deux pans de la feature ne sont pas exerçables à la main : voir un quota à 95 %,
//! [ADR-0016](../../../../docs/adr/0016-ash-sort-sur-le-reseau.md) demandant un compte, un
//! réseau qui répond et une consommation réelle ; et voir chacune des **cinq issues** de
//! lecture du trousseau qu'[ADR-0017](../../../../docs/adr/0017-ash-lit-le-jeton-de-l-outil.md)
//! oblige la fenêtre de réglages à distinguer, ce qui demanderait de casser le vrai
//! trousseau de la machine. Les deux ports existaient déjà — [`UsageApi`] et
//! [`TokenSource`] —, il ne manquait qu'un branchement.
//!
//! ## Ce que ça élargit, et qu'il faut dire plutôt que glisser
//!
//! `FakeKeychain` vivait — et vit toujours — sous `#[cfg(test)]` dans `token.rs`. Le faire
//! passer sous `#[cfg(debug_assertions)]` aurait été le geste le plus court, et c'est
//! précisément pour ça qu'il a été écarté : ADR-0017 resserre délibérément ce qui touche au
//! jeton (« tout ce qui touche au jeton est `pub(super)` »), et une doublure de test qui se
//! met à vivre dans un binaire qu'on lance est une surface qui a changé de nature sans que
//! personne ne l'ait décidé. Ce module est donc **une pièce distincte**, avec son propre
//! nom, son propre `cfg`, et une seule porte d'entrée ([`Rehearsal::parse`]).
//!
//! Ce que la doublure ne fait pas, et qui rend l'élargissement défendable : elle ne lit
//! aucun secret, n'ouvre aucun trousseau, ne compose aucune requête. Le jeton qu'elle rend
//! est une constante de ce fichier, et il ne peut atteindre aucun hôte — voir la règle du
//! couple ci-dessous. **Aucune condition d'ADR-0016 ni d'ADR-0017 n'est affaiblie** : elles
//! encadrent ce qu'Ash a le droit de lire et d'appeler, et la doublure ne lit ni n'appelle.
//!
//! ## La règle du couple : les deux ports, ou aucun
//!
//! Quand la variable est là, **les deux** doublures sont branchées, toujours. Il n'y a pas
//! de moyen de doubler l'un et de garder l'autre, et ce n'est pas une simplification : un
//! faux jeton envoyé au vrai hôte, ou un vrai jeton du trousseau envoyé à une fausse API,
//! sont exactement les deux mélanges qu'on ne veut pas pouvoir fabriquer.
//!
//! ## La forme de la variable — **et c'est le seul endroit où elle est écrite**
//!
//! ```text
//! ASH_DEV_USAGE="keychain=readable,host=ok,session=95@2m,weekly=28@3d"
//! ```
//!
//! | Clé | Valeurs | Défaut |
//! |---|---|---|
//! | `keychain` | `untried`, `readable`, `absent`, `refused`, `unreadable` | `readable` |
//! | `host` | `ok`, `unreachable`, `rejected`, `unauthorized` | `ok` |
//! | `session` | `<pourcentage>` ou `<pourcentage>@<délai>` | absent, donc aucun quota |
//! | `weekly` | idem | idem |
//!
//! Les cinq valeurs de `keychain` sont les cinq variantes de [`Readability`], écrites comme
//! le contrat TypeScript les écrit : le vocabulaire est déjà posé, la doublure l'emprunte
//! plutôt que d'en inventer un second. Les quatre valeurs de `host` sont les issues de
//! l'appel, `ok` mis à part, telles que [`UsageError`] les nomme.
//!
//! Un délai s'écrit `<nombre><s|m|h|d>` et se compte **à partir de maintenant**, à chaque
//! appel : `session=95@2m` reste à deux minutes de sa remise à zéro toute la session, ce qui
//! est ce qu'on veut d'un décor. Sans délai, le quota n'a pas de date — le cas que
//! `Quota::resets_at` rend facultatif.
//!
//! ## Une valeur illisible est un refus, jamais un repli
//!
//! [`Rehearsal::parse`] rend une [`RehearsalError`] pour toute clé inconnue, toute valeur
//! inconnue, toute clé répétée, et pour une variable posée mais vide. Le composition root en
//! fait un arrêt **bruyant** au démarrage. Se croire en doublure alors qu'on appelle
//! `api.anthropic.com` avec un vrai jeton du trousseau serait le pire mode de défaillance de
//! cette couture ; un démarrage qui échoue est infiniment préférable.

use std::sync::Arc;

use crate::shared::time::{Clock, UnixMillis};

use super::api::UsageApi;
use super::error::UsageError;
use super::quota::{AccountUsage, Quota};
use super::token::{AccessToken, TokenSource};

/// La variable qui demande les doublures. Sa forme est décrite en tête de ce module, et
/// **nulle part ailleurs**.
pub const REHEARSAL_VAR: &str = "ASH_DEV_USAGE";

/// Le jeton que la doublure rend : une constante, qui ne sort de nulle part et ne va nulle
/// part. Voir la règle du couple en tête de module.
const REHEARSED_TOKEN: &str = "ash-dev-rehearsal-token";

/// Ce que la variable décrit : un trousseau, un hôte, et deux quotas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rehearsal {
    keychain: Keychain,
    host: Host,
    session: Option<Rehearsed>,
    weekly: Option<Rehearsed>,
}

/// L'issue que le trousseau doublé donnera. Les cinq de [`Readability`](super::Readability).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keychain {
    /// Rien ne se règle : la lecture n'en finit pas, comme un dialogue de trousseau que
    /// personne n'a encore refermé. La fenêtre de réglages dit alors « rien tenté », qui est
    /// exactement ce qu'elle dirait dans ce cas-là en vrai.
    Untried,
    Readable,
    Absent,
    Refused,
    Unreadable,
}

/// Ce que l'hôte doublé répondra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Host {
    /// Les quotas décrits par la variable.
    Answers,
    Unreachable,
    Rejected,
    Unauthorized,
}

/// Un quota décrit par la variable : un pourcentage, et un délai avant remise à zéro.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rehearsed {
    percent: f64,
    resets_in: Option<std::time::Duration>,
}

/// Pourquoi la variable n'a pas été comprise.
///
/// Chaque variante nomme le fragment fautif : rien de ce qui entre ici n'est un secret —
/// c'est une variable d'environnement que le développeur vient d'écrire — et un refus qui ne
/// dirait pas *quoi* obligerait à relire ce fichier pour corriger une virgule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RehearsalError {
    /// La variable est posée mais ne demande rien. Le cas le plus courant est un
    /// `export ASH_DEV_USAGE=` cru suffisant pour l'annuler — il ne l'est pas, et se taire
    /// ici laisserait croire que le vrai chemin est repris alors qu'il ne l'est pas.
    NothingAsked,
    NotAPair(String),
    UnknownKey(String),
    RepeatedKey(String),
    UnknownValue {
        key: &'static str,
        value: String,
    },
    BadQuota(String),
}

impl std::fmt::Display for RehearsalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingAsked => f.write_str(
                "it asks for nothing — unset it entirely to go back to the real keychain and host",
            ),
            Self::NotAPair(fragment) => write!(f, "`{fragment}` is not a `key=value` pair"),
            Self::UnknownKey(key) => {
                write!(f, "`{key}` is not one of keychain, host, session, weekly")
            }
            Self::RepeatedKey(key) => write!(f, "`{key}` is named twice"),
            Self::UnknownValue { key, value } => write!(f, "`{value}` is not a `{key}` outcome"),
            Self::BadQuota(value) => write!(
                f,
                "`{value}` is not a percentage, optionally followed by `@` and a delay such as `2m`"
            ),
        }
    }
}

impl std::error::Error for RehearsalError {}

impl Rehearsal {
    /// Ce que la variable décrit, ou pourquoi elle ne décrit rien.
    ///
    /// **Une fonction pure** : elle ne lit pas l'environnement. C'est le composition root
    /// qui le fait, parce que lire l'environnement est un effet système et que la règle du
    /// dépôt est qu'une feature n'en fait pas en douce — celui-ci, en plus, décide de
    /// l'assemblage, ce qui est très exactement le métier du composition root.
    pub fn parse(spec: &str) -> Result<Self, RehearsalError> {
        let mut rehearsal = Self {
            keychain: Keychain::Readable,
            host: Host::Answers,
            session: None,
            weekly: None,
        };
        let mut named: Vec<String> = Vec::new();

        for fragment in spec.split(',').map(str::trim).filter(|it| !it.is_empty()) {
            let (key, value) = fragment
                .split_once('=')
                .ok_or_else(|| RehearsalError::NotAPair(fragment.to_owned()))?;
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();

            if named.contains(&key) {
                return Err(RehearsalError::RepeatedKey(key));
            }
            match key.as_str() {
                "keychain" => rehearsal.keychain = keychain(value)?,
                "host" => rehearsal.host = host(value)?,
                "session" => rehearsal.session = Some(rehearsed(value)?),
                "weekly" => rehearsal.weekly = Some(rehearsed(value)?),
                _ => return Err(RehearsalError::UnknownKey(key)),
            }
            named.push(key);
        }

        if named.is_empty() {
            return Err(RehearsalError::NothingAsked);
        }
        Ok(rehearsal)
    }

    /// Le trousseau doublé. Voir la règle du couple : il ne se demande jamais seul.
    #[must_use]
    pub fn tokens(&self) -> Arc<dyn TokenSource> {
        Arc::new(RehearsedTokens {
            keychain: self.keychain,
        })
    }

    /// L'hôte doublé, qui ne compose aucune requête et ne nomme aucune adresse.
    #[must_use]
    pub fn api(&self, clock: Arc<dyn Clock>) -> Arc<dyn UsageApi> {
        Arc::new(RehearsedApi {
            answer: *self,
            clock,
        })
    }
}

fn keychain(value: &str) -> Result<Keychain, RehearsalError> {
    match value.to_ascii_lowercase().as_str() {
        "untried" => Ok(Keychain::Untried),
        "readable" => Ok(Keychain::Readable),
        "absent" => Ok(Keychain::Absent),
        "refused" => Ok(Keychain::Refused),
        "unreadable" => Ok(Keychain::Unreadable),
        _ => Err(RehearsalError::UnknownValue {
            key: "keychain",
            value: value.to_owned(),
        }),
    }
}

fn host(value: &str) -> Result<Host, RehearsalError> {
    match value.to_ascii_lowercase().as_str() {
        "ok" => Ok(Host::Answers),
        "unreachable" => Ok(Host::Unreachable),
        "rejected" => Ok(Host::Rejected),
        "unauthorized" => Ok(Host::Unauthorized),
        _ => Err(RehearsalError::UnknownValue {
            key: "host",
            value: value.to_owned(),
        }),
    }
}

/// `95`, ou `95@2m`.
fn rehearsed(value: &str) -> Result<Rehearsed, RehearsalError> {
    let refuse = || RehearsalError::BadQuota(value.to_owned());
    let (raw_percent, raw_delay) = match value.split_once('@') {
        Some((percent, delay)) => (percent.trim(), Some(delay.trim())),
        None => (value.trim(), None),
    };

    let percent: f64 = raw_percent.parse().map_err(|_| refuse())?;
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        return Err(refuse());
    }

    let resets_in = match raw_delay {
        None => None,
        Some(delay) => Some(seconds(delay).ok_or_else(refuse)?),
    };
    Ok(Rehearsed { percent, resets_in })
}

/// `45s`, `2m`, `3h`, `7d` — et rien d'autre.
fn seconds(delay: &str) -> Option<std::time::Duration> {
    let (count, unit) = delay.split_at_checked(delay.len().checked_sub(1)?)?;
    let count: u64 = count.parse().ok()?;
    let per_unit = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 60 * 60,
        "d" => 24 * 60 * 60,
        _ => return None,
    };
    count
        .checked_mul(per_unit)
        .map(std::time::Duration::from_secs)
}

/// Le trousseau, sans trousseau.
struct RehearsedTokens {
    keychain: Keychain,
}

impl TokenSource for RehearsedTokens {
    /// Les mêmes valeurs que le vrai chemin, pas des valeurs qui leur ressemblent : ce sont
    /// les [`UsageError`] que `KeychainTokens` rend, donc les mêmes
    /// [`Readability`](super::Readability) une fois traversé `Credentials`.
    fn read(&self) -> Result<AccessToken, UsageError> {
        match self.keychain {
            // La seule issue qui ne se règle pas, parce que « rien tenté » n'est pas une
            // réponse du trousseau mais son absence de réponse. Le fil qui attend ici est
            // celui du sondage, que la condition 1 d'ADR-0016 rend inoffensif — la vraie
            // lecture peut d'ailleurs attendre un clic humain aussi longtemps, et `token.rs`
            // dit pourquoi elle n'a pas de délai d'attente.
            Keychain::Untried => loop {
                std::thread::park();
            },
            Keychain::Readable => AccessToken::new(REHEARSED_TOKEN).ok_or(UsageError::Unreadable),
            Keychain::Absent => Err(UsageError::NoToken),
            Keychain::Refused => Err(UsageError::Refused),
            Keychain::Unreadable => Err(UsageError::Unreadable),
        }
    }
}

/// L'hôte, sans réseau. Il ne nomme aucune adresse, et ne pourrait pas : `ureq` n'est pas
/// dans ce fichier, et l'unique URL du dépôt reste celle d'`api.rs`.
struct RehearsedApi {
    answer: Rehearsal,
    clock: Arc<dyn Clock>,
}

impl UsageApi for RehearsedApi {
    fn fetch(&self, _token: &AccessToken) -> Result<AccountUsage, UsageError> {
        match self.answer.host {
            Host::Unreachable => Err(UsageError::Unreachable),
            Host::Rejected => Err(UsageError::Rejected),
            Host::Unauthorized => Err(UsageError::Unauthorized),
            Host::Answers => Ok(AccountUsage {
                session: self.quota(self.answer.session),
                weekly: self.quota(self.answer.weekly),
            }),
        }
    }
}

impl RehearsedApi {
    /// Un [`Quota`] construit directement, et non un JSON qu'on ferait relire à
    /// `read_usage` : la lecture de la réponse a ses propres tests, et une doublure qui
    /// recomposerait un corps ne prouverait rien de plus tout en pouvant, elle, se tromper.
    fn quota(&self, rehearsed: Option<Rehearsed>) -> Option<Quota> {
        let rehearsed = rehearsed?;
        Some(Quota {
            percent: rehearsed.percent,
            resets_at: rehearsed.resets_in.map(|delay| {
                self.clock
                    .wall()
                    .saturating_add(UnixMillis::try_from(delay.as_millis()).unwrap_or(0))
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::super::token::{Credentials, Readability};
    use super::*;

    /// Une heure murale fixe : la date rendue est un calcul, pas une lecture d'horloge.
    struct FixedClock(UnixMillis);

    impl Clock for FixedClock {
        fn now(&self) -> Instant {
            Instant::now()
        }

        fn wall(&self) -> UnixMillis {
            self.0
        }
    }

    fn rehearsal(spec: &str) -> Rehearsal {
        Rehearsal::parse(spec).expect("une variable que le test écrit bien")
    }

    fn usage_of(spec: &str, at: UnixMillis) -> Result<AccountUsage, UsageError> {
        let doubled = rehearsal(spec);
        let api = doubled.api(Arc::new(FixedClock(at)));
        let token = AccessToken::new("peu importe").expect("un jeton non vide");
        api.fetch(&token)
    }

    #[test]
    fn given_each_keychain_outcome_the_variable_names_when_the_credentials_read_it_then_the_settings_window_says_the_matching_one(
    ) {
        // Given — c'est tout l'objet de la couture : les cinq issues qu'ADR-0017 oblige à
        // distinguer doivent être atteignables sans toucher au vrai trousseau, et la
        // doublure doit produire *les mêmes* valeurs que `KeychainTokens`, pas des valeurs
        // qui leur ressemblent. `untried` manque ici, et seulement ici : c'est la seule qui
        // ne se règle pas — la lecture n'en finit pas, exprès (voir `RehearsedTokens`)
        let asked = ["readable", "absent", "refused", "unreadable"];

        // When
        let said: Vec<Readability> = asked
            .iter()
            .map(|outcome| {
                let credentials =
                    Credentials::from(rehearsal(&format!("keychain={outcome}")).tokens());
                let _ = credentials.token();
                credentials.readability()
            })
            .collect();

        // Then
        assert_eq!(
            said,
            vec![
                Readability::Readable,
                Readability::Absent,
                Readability::Refused,
                Readability::Unreadable,
            ]
        );
    }

    #[test]
    fn given_the_five_keychain_outcomes_when_the_variable_names_them_then_all_five_are_accepted() {
        // Given — le vocabulaire est celui de `Readability`, tel que le contrat TypeScript
        // l'écrit. Ce test est ce qui tombera le jour où une sixième issue apparaîtra sans
        // que la couture la nomme
        let words = ["untried", "readable", "absent", "refused", "unreadable"];

        // When
        let parsed: Vec<Option<Keychain>> = words
            .iter()
            .map(|word| Rehearsal::parse(&format!("keychain={word}")).ok())
            .map(|read| read.map(|it| it.keychain))
            .collect();

        // Then
        assert_eq!(
            parsed,
            vec![
                Some(Keychain::Untried),
                Some(Keychain::Readable),
                Some(Keychain::Absent),
                Some(Keychain::Refused),
                Some(Keychain::Unreadable),
            ]
        );
    }

    #[test]
    fn given_a_variable_asking_for_a_high_session_quota_when_the_doubled_host_answers_then_it_carries_that_percentage_and_a_date_ahead(
    ) {
        // Given — un quota à 95 % et une remise à zéro proche : ce qu'un compte réel ne
        // rendra pas sur commande
        let now: UnixMillis = 1_800_000_000_000;

        // When
        let answered = usage_of("session=95@2m,weekly=28@3d", now);

        // Then — un pourcentage, et une **date absolue**, comme ce qui traverse la frontière
        assert_eq!(
            answered,
            Ok(AccountUsage {
                session: Some(Quota {
                    percent: 95.0,
                    resets_at: Some(now + 2 * 60 * 1000),
                }),
                weekly: Some(Quota {
                    percent: 28.0,
                    resets_at: Some(now + 3 * 24 * 60 * 60 * 1000),
                }),
            })
        );
    }

    #[test]
    fn given_a_variable_asking_for_a_host_that_does_not_answer_when_the_call_is_made_then_the_value_disappears(
    ) {
        // Given — la conduite forte d'ADR-0016 : en échec, la valeur disparaît. Elle n'était
        // pas exerçable sans débrancher la machine
        let now: UnixMillis = 1_800_000_000_000;

        // When
        let answered = usage_of("host=unreachable,session=95", now);

        // Then — une erreur, dont le poller fera un `AccountUsage::unknown` : ni zéro, ni
        // tiret, et surtout pas les 95 % que la variable décrit par ailleurs
        assert_eq!(answered, Err(UsageError::Unreachable));
    }

    #[test]
    fn given_a_variable_that_cannot_be_read_when_it_is_parsed_then_it_is_refused_rather_than_ignored(
    ) {
        // Given — le mode de défaillance qui compte : se croire en doublure alors qu'on
        // appelle le vrai hôte avec un vrai jeton du trousseau. Chacune de ces variables est
        // une faute de frappe plausible, et aucune ne doit retomber sur le vrai chemin
        let mistakes = [
            "",
            "   ",
            "keychain",
            "keychain=granted",
            "trousseau=absent",
            "host=ok,host=rejected",
            "session=120",
            "session=95@2",
            "session=95@2y",
            "session=beaucoup",
        ];

        // When
        let read: Vec<bool> = mistakes
            .iter()
            .map(|spec| Rehearsal::parse(spec).is_ok())
            .collect();

        // Then
        assert_eq!(read, vec![false; mistakes.len()]);
    }

    #[test]
    fn given_a_variable_that_names_no_quota_when_the_doubled_host_answers_then_it_knows_nothing_rather_than_zero(
    ) {
        // Given — un compte sans fenêtre de limitation rend cette forme-là, et `0 %` serait
        // un chiffre inventé
        let doubled = "keychain=readable";

        // When
        let answered = usage_of(doubled, 1_800_000_000_000);

        // Then
        assert_eq!(answered, Ok(AccountUsage::unknown()));
    }
}
