//! Le jeton de l'outil, lu dans le trousseau — **le deuxième binaire externe qu'Ash lance**.
//!
//! [ADR-0017](../../../../docs/adr/0017-ash-lit-le-jeton-de-l-outil.md) décide qu'Ash lit
//! l'item de trousseau de Claude Code, à quatre conditions. Ce module les porte toutes les
//! quatre, et il faut le lire aussi sérieusement que `features/git/git_cli.rs` : c'est la
//! **seconde frontière de sécurité** du dépôt, et la première qui touche un secret.
//!
//! ## La frontière, en une phrase
//!
//! **Rien de ce qu'Ash a lu, reçu ou calculé n'atteint la ligne de commande de `security`.**
//! L'invocation est une constante ([`FIND_PASSWORD`]) composée à la compilation ; il n'y a
//! pas de paramètre, donc pas de chemin par lequel un nom de dépôt, un `cwd` sondé ou une
//! réponse d'API pourrait y entrer. Le jour où quelqu'un voudra rendre le nom du service
//! configurable, c'est ici qu'il tombera — et la réponse est non tant que la valeur vient
//! d'ailleurs que d'une constante du crate.
//!
//! Ce que chaque décision achète :
//!
//! | Décision | Ce qu'elle empêche |
//! |---|---|
//! | Chemin **absolu** `/usr/bin/security` | qu'un `security` posé dans le `PATH` hérité du shell de l'utilisateur soit lancé à la place |
//! | `argv` **constant**, sans aucune entrée | qu'une valeur venue d'un fichier, d'un dépôt ou du réseau devienne un argument |
//! | Le secret sort par **`stdout`**, jamais par un argument | qu'il soit lisible par `ps` pour tout processus de la machine — c'est l'argument même qui a fait écarter `curl` dans ADR-0016 |
//! | `stdin` fermé, `stderr` **jeté** | qu'un message d'erreur du trousseau, qui peut nommer l'item, entre dans le processus puis dans une trace |
//! | Aucune variable d'environnement retirée ni ajoutée | rien : c'est un constat. `security` n'en lit aucune qui change ce qu'il rend, et le proxy de l'appel réseau vit dans l'autre module |
//! | [`UsageError`] sans champ | que la sortie lue serve à composer un message. Voir `error.rs` |
//! | Tout ce qui **touche** au jeton est `pub(super)` | qu'un `AccessToken` sorte de la feature. Hors d'ici, [`Credentials`] n'a qu'un constructeur : lire, oublier et rapporter la lisibilité ne se disent qu'à l'intérieur, où le seul appelant est le fil de fond du poller |
//!
//! ## Il n'y a **pas** de délai d'attente, et c'est l'inverse de `git_cli.rs`
//!
//! `features/git/git_cli.rs` borne son processus fils à cinq secondes, parce qu'un dépôt trop
//! gros ne doit pas retenir une ligne de statut. Ici, ce que le processus fils attend est un
//! **clic humain** dans le dialogue du trousseau : le borner reviendrait à abandonner
//! l'autorisation pendant que l'utilisateur la lit, donc à fabriquer un refus qu'il n'a pas
//! donné — et la condition 4 d'ADR-0017 rendrait ce faux refus définitif. L'attente est donc
//! sans borne, et c'est la condition 1 d'ADR-0016 qui la rend inoffensive : elle vit sur un
//! fil que personne n'attend.
//!
//! ## Pourquoi `security` plutôt que `security-framework`
//!
//! La crate `security-framework` ferait le même appel sans processus fils, et ce serait
//! plus élégant. Elle a été écartée pour deux raisons, dans cet ordre : ADR-0016 vient
//! d'ajouter `ureq` à un arbre qui compte peu de dépendances, et l'`argv` d'ici est
//! **constant** — le risque que l'exécution d'un binaire externe fait courir ailleurs
//! (`git_cli.rs` et son dépôt hostile) n'a pas d'équivalent ici, faute d'entrée. Le jour
//! où distinguer finement les `OSStatus` deviendra nécessaire, l'échange redeviendra
//! intéressant.
//!
//! ## Les codes de sortie, et pourquoi ils décident d'une conduite
//!
//! `security` sort avec l'`OSStatus` tronqué à un octet. Deux valeurs comptent :
//!
//! - **44** — `errSecItemNotFound` (`-25300`, soit `44` modulo 256) : l'item n'existe pas.
//!   **Aucun dialogue n'est apparu**, personne n'a rien refusé, et un utilisateur qui se
//!   connecte à l'outil pendant qu'Ash tourne doit voir ses quotas arriver. Ash relira.
//! - **128** — `errSecUserCanceled` (`-128`) : l'utilisateur a refusé. C'est la condition 4
//!   d'ADR-0017, et [`Credentials`] la rend définitive.
//!
//! Tout autre code non nul est traité **comme un refus**, et c'est le sens prudent : ne
//! pas savoir pourquoi une lecture a échoué n'autorise pas à reposer la question toutes
//! les minutes.
//!
//! ## Ce que ça coûte en développement, et qu'il vaut mieux savoir
//!
//! **Ash-dev a son propre identifiant de paquet, donc sa propre autorisation** (ADR-0017,
//! conséquences). Le dialogue du trousseau reparaît donc à chaque build installé, et
//! `bun run smoke` peut le faire apparaître. C'est attendu, c'est le même effet de bord voulu
//! que pour les notifications et le stockage, et il n'y a rien à corriger : le contourner
//! serait précisément ce que la condition 1 interdit.
//!
//! ## Ce que la fenêtre de réglages lit ici, et ce qu'elle ne déclenche pas
//!
//! [`Credentials::readability`] rend l'issue de la **dernière** lecture, sans en faire
//! aucune. C'est la condition 1 d'ADR-0016 appliquée à une surface qui n'y pense pas : ouvrir
//! la section `usage` de la fenêtre de réglages ne doit pas faire surgir un dialogue de
//! trousseau, et la fenêtre n'a de toute façon rien à demander — elle rapporte ce que le fil
//! de fond sait déjà.

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use super::error::UsageError;

/// Le binaire, en dur et en absolu. Voir le tableau de l'en-tête.
const SECURITY: &str = "/usr/bin/security";

/// L'invocation, en entier, et **sans trou où glisser une valeur**.
///
/// `-w` demande que seul le mot de passe soit écrit sur `stdout` : sans lui, `security`
/// rend une description de l'item, plus verbeuse et plus difficile à ne pas recopier
/// ailleurs par erreur. `-s` nomme le service, et sa valeur est la constante ci-dessous.
const FIND_PASSWORD: [&str; 4] = ["find-generic-password", "-w", "-s", CREDENTIALS_SERVICE];

/// L'item que Claude Code écrit dans le trousseau de session.
///
/// Le jour où il change de nom, les quotas disparaissent en silence — ADR-0017 le dit et
/// l'assume, et c'est pourquoi la fenêtre de réglages devra pouvoir dire « le jeton n'est
/// pas lisible ».
const CREDENTIALS_SERVICE: &str = "Claude Code-credentials";

/// `errSecItemNotFound`, tronqué à un octet par `security`.
const ITEM_NOT_FOUND: i32 = 44;

/// Le jeton, et un type qui ne sait pas se raconter.
///
/// Pas de `Display`, pas de `Debug` dérivé, pas de `Serialize` : les trois traits par
/// lesquels un secret arrive dans une trace, un event Tauri ou un fichier. La seule sortie
/// est [`Self::expose`], `pub(super)`, et son unique appelant est la composition de l'en-tête
/// `Authorization` dans `api.rs`.
#[derive(Clone, PartialEq, Eq)]
pub struct AccessToken(String);

impl AccessToken {
    /// Un jeton, ou rien si la chaîne est vide.
    ///
    /// Un jeton vide passerait le typage et ferait partir un `Authorization: Bearer ` que
    /// l'hôte refuserait : autant ne pas appeler.
    #[must_use]
    pub fn new(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| Self(trimmed.to_owned()))
    }

    /// Le jeton en clair — **le seul endroit du crate qui le lise**.
    pub(super) fn expose(&self) -> &str {
        &self.0
    }
}

/// La seule chose qu'un jeton dit de lui-même.
impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccessToken(<redacted>)")
    }
}

/// D'où vient le jeton.
///
/// Un trait, pour la raison qui vaut pour tous les effets système du dépôt, et une de plus
/// qui lui est propre : **aucun `cargo test` ne doit lire le trousseau de qui le lance**,
/// ni faire apparaître un dialogue d'autorisation macOS sur son écran.
pub trait TokenSource: Send + Sync {
    fn read(&self) -> Result<AccessToken, UsageError>;
}

/// Le trousseau de session, par `/usr/bin/security`. Voir l'en-tête du module.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeychainTokens;

impl TokenSource for KeychainTokens {
    fn read(&self) -> Result<AccessToken, UsageError> {
        let found = Command::new(SECURITY)
            .args(FIND_PASSWORD)
            .stdin(Stdio::null())
            // Le trousseau nomme l'item dans ses messages d'erreur : on ne les lit pas.
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .output()
            .map_err(|_| UsageError::Refused)?;

        if !found.status.success() {
            return Err(match found.status.code() {
                Some(ITEM_NOT_FOUND) => UsageError::NoToken,
                // 128 (`errSecUserCanceled`) et tout le reste : voir l'en-tête.
                _ => UsageError::Refused,
            });
        }

        // Le corps de l'item n'entre nulle part ailleurs que dans cette expression : ni
        // dans une erreur, ni dans une trace, ni dans une valeur qu'on garderait.
        let document = String::from_utf8(found.stdout).map_err(|_| UsageError::Unreadable)?;
        access_token_in(&document).ok_or(UsageError::Unreadable)
    }
}

/// Le jeton d'accès dans le document de l'item, ou `None`.
///
/// L'item porte un JSON dont on ne lit **qu'un champ** — `claudeAiOauth.accessToken`. Tout
/// ce qui l'entoure (le jeton de renouvellement, la date d'expiration, les portées) est
/// ignoré, et c'est délibéré : Ash n'a besoin que de celui-là, et ne renouvelle rien.
fn access_token_in(document: &str) -> Option<AccessToken> {
    let parsed = serde_json::from_str::<serde_json::Value>(document).ok()?;
    AccessToken::new(parsed.get("claudeAiOauth")?.get("accessToken")?.as_str()?)
}

/// Le jeton tel que le poller le voit : lu une fois, oublié quand l'hôte le refuse, et
/// **plus jamais redemandé** après un refus de l'utilisateur.
///
/// Trois conduites, et chacune répond à une phrase d'une ADR :
///
/// - **Gardé en mémoire entre deux appels.** C'est ce que la condition 3 d'ADR-0017 autorise
///   nommément depuis son amendement, et elle en donne la raison : ce qui est interdit est la
///   **persistance**, ce qui survit à l'extinction. Un jeton en mémoire meurt avec le
///   processus. Relire le trousseau à chaque cycle rouvrirait le dialogue de macOS toutes les
///   minutes chez qui a cliqué « Autoriser » plutôt qu'« Toujours autoriser » — or la
///   condition 1 dit qu'Ash ne **contourne** pas ce dialogue, pas qu'il doit le déclencher en
///   boucle.
/// - **Oublié sur un refus de l'hôte** ([`UsageError::Unauthorized`]). Un jeton OAuth
///   expire, et l'outil en écrit un neuf dans le même item : sans cet oubli, les quotas
///   disparaîtraient au bout d'une heure et ne reviendraient qu'au redémarrage d'Ash, qui
///   tourne toute la journée.
/// - **Fermé pour de bon** après un [`UsageError::Refused`]. C'est la condition 4
///   d'ADR-0017 : « pas de nouvelle demande, pas de bannière, pas d'invite », et rien ne
///   rouvre — pas même l'interrupteur de réglages.
pub struct Credentials {
    source: Arc<dyn TokenSource>,
    known: Mutex<Known>,
}

/// Ce qu'on sait du jeton, à un instant donné.
///
/// Deux faits sous **un seul verrou**, et pas deux champs sous deux verrous : le second dit
/// ce que le premier a coûté, et les laisser diverger ferait afficher « lisible » à la
/// fenêtre de réglages pendant que le poller n'a plus rien.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Known {
    /// Ce qui décide de la **conduite** : relire, se servir, ou ne plus jamais demander.
    held: Held,
    /// Ce qui décide de ce que la fenêtre de réglages **dit**. Voir [`Readability`].
    readability: Readability,
}

/// Ce qu'on sait du jeton, à un instant donné.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Held {
    /// Rien encore, ou plus rien : la prochaine demande relira le trousseau.
    Unread,
    Token(AccessToken),
    /// Refusé une fois, donc pour toujours.
    Closed,
}

/// L'issue de la **dernière** lecture du trousseau, telle que la fenêtre de réglages la dit.
///
/// Les conséquences d'ADR-0017 l'exigent nommément : « le jour où l'item change de nom ou de
/// forme, les quotas disparaissent en silence. C'est la conduite voulue, et elle se
/// diagnostiquera mal. La fenêtre de réglages doit donc pouvoir dire *le jeton n'est pas
/// lisible* ». Sans ces variantes, un refus, un item absent et une panne seraient
/// indiscernables — et c'est exactement le prix du « en échec, la valeur disparaît ».
///
/// **Lire cette valeur ne lit rien** : c'est un souvenir, pas une question posée au système.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum Readability {
    /// Rien n'a encore été tenté — Ash vient de démarrer, ou sa fenêtre n'a pas encore été
    /// devant. Ce n'est **pas** une panne, et la section le dit ainsi.
    #[default]
    Untried,
    /// Le trousseau a rendu un jeton exploitable.
    Readable,
    /// L'item n'est pas dans le trousseau : personne n'est connecté à l'outil, ou il
    /// s'authentifie autrement. **Aucun dialogue n'est apparu.**
    Absent,
    /// L'utilisateur a refusé, ou macOS a répondu autre chose qu'on ne sait pas lire.
    /// **Définitif** (condition 4 d'ADR-0017).
    Refused,
    /// L'item est là, mais son contenu n'est plus le document attendu — c'est le cas que les
    /// conséquences d'ADR-0017 disent devoir être nommable.
    Unreadable,
}

impl From<&Result<AccessToken, UsageError>> for Readability {
    /// Ce qu'une lecture apprend, traduit pour la fenêtre.
    ///
    /// Deux vocabulaires plutôt qu'un, et c'est la frontière qui l'impose, exactement comme
    /// pour l'autorisation de notifier : [`UsageError`] est ce qui règle la conduite du
    /// poller, [`Readability`] est ce que le contrat TypeScript porte.
    fn from(read: &Result<AccessToken, UsageError>) -> Self {
        match read {
            Ok(_) => Self::Readable,
            Err(UsageError::NoToken) => Self::Absent,
            Err(UsageError::Unreadable) => Self::Unreadable,
            Err(_) => Self::Refused,
        }
    }
}

impl Credentials {
    pub fn from(source: Arc<dyn TokenSource>) -> Self {
        Self {
            source,
            known: Mutex::new(Known {
                held: Held::Unread,
                readability: Readability::Untried,
            }),
        }
    }

    /// L'issue de la dernière lecture — **et aucune lecture n'est faite ici**.
    ///
    /// C'est ce que la section `usage` de la fenêtre de réglages affiche. Voir l'en-tête du
    /// module pour la raison pour laquelle cette méthode ne peut pas être un `read()`
    /// déguisé.
    pub(super) fn readability(&self) -> Readability {
        self.locked().readability
    }

    /// Le jeton, lu si nécessaire.
    ///
    /// **La lecture est bloquante et peut attendre un clic humain** (ADR-0017,
    /// conséquences) : cette méthode n'a rien à faire sur un chemin de rendu, de sonde ou
    /// de hook. Son seul appelant vit sur le fil de fond du poller.
    pub(super) fn token(&self) -> Result<AccessToken, UsageError> {
        let held = self.locked().held.clone();
        match held {
            Held::Token(token) => Ok(token),
            Held::Closed => Err(UsageError::Refused),
            Held::Unread => {
                let read = self.source.read();
                let mut known = self.locked();
                known.readability = Readability::from(&read);
                known.held = match &read {
                    Ok(token) => Held::Token(token.clone()),
                    // Une absence n'est pas un refus : personne n'a rien vu passer, et
                    // l'utilisateur peut se connecter à l'outil dans la minute.
                    Err(UsageError::NoToken) => Held::Unread,
                    Err(_) => Held::Closed,
                };
                read
            }
        }
    }

    /// L'hôte a refusé ce jeton : le relire au prochain tour.
    ///
    /// La lisibilité, elle, ne bouge pas : le trousseau avait bien rendu un jeton, et c'est
    /// l'hôte qui n'en veut plus. Dire « illisible » ici enverrait l'utilisateur regarder du
    /// mauvais côté.
    pub(super) fn forget(&self) {
        let mut known = self.locked();
        if matches!(known.held, Held::Token(_)) {
            known.held = Held::Unread;
        }
    }

    /// Un verrou empoisonné veut dire qu'un fil a paniqué **ailleurs** en le tenant. Ce
    /// qu'il protège est intact, et propager la panique ferait tomber le fil de fond pour
    /// une jauge.
    fn locked(&self) -> std::sync::MutexGuard<'_, Known> {
        self.known
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un trousseau en mémoire, qui compte ce qu'on lui demande.
    ///
    /// Le compte est ce qui rend la condition 4 d'ADR-0017 vérifiable : « ne redemande
    /// pas » n'est observable que par le nombre de fois où la question est posée.
    #[derive(Default)]
    struct FakeKeychain {
        answers: Mutex<Vec<Result<AccessToken, UsageError>>>,
        asked: Mutex<usize>,
    }

    impl FakeKeychain {
        /// Ce que la lecture rendra, dans l'ordre. La dernière réponse est répétée.
        fn answering(answers: Vec<Result<AccessToken, UsageError>>) -> Arc<Self> {
            Arc::new(Self {
                answers: Mutex::new(answers),
                asked: Mutex::new(0),
            })
        }

        fn asked(&self) -> usize {
            *self.asked.lock().unwrap()
        }
    }

    impl TokenSource for FakeKeychain {
        fn read(&self) -> Result<AccessToken, UsageError> {
            *self.asked.lock().unwrap() += 1;
            let mut answers = self.answers.lock().unwrap();
            if answers.len() > 1 {
                answers.remove(0)
            } else {
                answers.first().cloned().unwrap_or(Err(UsageError::NoToken))
            }
        }
    }

    fn token(raw: &str) -> AccessToken {
        AccessToken::new(raw).expect("un jeton non vide")
    }

    #[test]
    fn given_a_user_who_refused_the_keychain_when_ash_needs_the_token_again_then_it_never_asks_a_second_time(
    ) {
        // Given — la condition 4 d'ADR-0017. Sans elle, un utilisateur qui refuse une fois
        // verrait le dialogue du trousseau revenir toutes les minutes, toute la journée
        let keychain = FakeKeychain::answering(vec![Err(UsageError::Refused)]);
        let credentials = Credentials::from(Arc::clone(&keychain) as Arc<dyn TokenSource>);

        // When
        let first = credentials.token();
        let later = credentials.token();
        credentials.forget();
        let after_forgetting = credentials.token();

        // Then — une seule question posée, et le refus tient même après un `forget`
        assert_eq!(first, Err(UsageError::Refused));
        assert_eq!(later, Err(UsageError::Refused));
        assert_eq!(after_forgetting, Err(UsageError::Refused));
        assert_eq!(keychain.asked(), 1);
    }

    #[test]
    fn given_an_absent_keychain_item_when_the_user_signs_into_the_tool_then_the_token_is_found_on_a_later_try(
    ) {
        // Given — un item absent n'a fait apparaître aucun dialogue : personne n'a rien
        // refusé. Fermer là-dessus voudrait dire qu'ouvrir Ash avant de se connecter à
        // l'outil condamne les quotas jusqu'au redémarrage
        let keychain = FakeKeychain::answering(vec![
            Err(UsageError::NoToken),
            Err(UsageError::NoToken),
            Ok(token("secret-abc")),
        ]);
        let credentials = Credentials::from(Arc::clone(&keychain) as Arc<dyn TokenSource>);

        // When
        let before = credentials.token();
        let _ = credentials.token();
        let after_signing_in = credentials.token();

        // Then
        assert_eq!(before, Err(UsageError::NoToken));
        assert_eq!(after_signing_in, Ok(token("secret-abc")));
    }

    #[test]
    fn given_a_token_already_read_when_the_next_call_needs_it_then_the_keychain_is_not_asked_again()
    {
        // Given — relire à chaque appel reposerait la question du trousseau toutes les
        // minutes à qui a cliqué « Autoriser » plutôt qu'« Toujours autoriser »
        let keychain = FakeKeychain::answering(vec![Ok(token("secret-abc"))]);
        let credentials = Credentials::from(Arc::clone(&keychain) as Arc<dyn TokenSource>);

        // When
        let first = credentials.token();
        let second = credentials.token();

        // Then
        assert_eq!(first, second);
        assert_eq!(keychain.asked(), 1);
    }

    #[test]
    fn given_a_token_the_host_refused_when_the_tool_has_renewed_it_then_ash_reads_the_fresh_one() {
        // Given — un jeton OAuth expire, et l'outil en écrit un neuf dans le même item.
        // Sans relecture, les quotas disparaîtraient au bout d'une heure pour ne revenir
        // qu'au redémarrage d'une application qui tourne toute la journée
        let keychain =
            FakeKeychain::answering(vec![Ok(token("expired")), Ok(token("renewed-by-the-tool"))]);
        let credentials = Credentials::from(Arc::clone(&keychain) as Arc<dyn TokenSource>);

        // When
        let expired = credentials.token();
        credentials.forget();
        let renewed = credentials.token();

        // Then
        assert_eq!(expired, Ok(token("expired")));
        assert_eq!(renewed, Ok(token("renewed-by-the-tool")));
    }

    #[test]
    fn given_a_keychain_item_when_its_document_is_read_then_only_the_oauth_access_token_comes_out()
    {
        // Given — l'item porte plus que le jeton d'accès : un jeton de renouvellement, une
        // date d'expiration, des portées. Rien de tout cela n'a de raison d'entrer dans
        // Ash, qui ne renouvelle rien
        let item = r#"{
            "claudeAiOauth": {
                "accessToken": "sk-ant-oat-visible",
                "refreshToken": "sk-ant-ort-never-read",
                "expiresAt": 1786000000000,
                "scopes": ["user:inference"]
            }
        }"#;

        // When
        let found = access_token_in(item);

        // Then
        assert_eq!(found, Some(token("sk-ant-oat-visible")));
    }

    #[test]
    fn given_a_keychain_item_that_is_not_the_document_ash_expects_when_it_is_read_then_nothing_comes_out(
    ) {
        // Given — l'item peut changer de forme ou de nom sans prévenir (ADR-0017,
        // conséquences), et ce qui en sort ne doit jamais être « presque un jeton »
        let unreadable = [
            "",
            "not json at all",
            "{}",
            r#"{"claudeAiOauth":{}}"#,
            r#"{"claudeAiOauth":{"accessToken":null}}"#,
            r#"{"claudeAiOauth":{"accessToken":""}}"#,
            r#"{"claudeAiOauth":{"accessToken":"   "}}"#,
            r#"{"accessToken":"at-the-wrong-level"}"#,
        ];

        // When
        let read: Vec<Option<AccessToken>> =
            unreadable.iter().map(|i| access_token_in(i)).collect();

        // Then
        assert_eq!(read, vec![None; unreadable.len()]);
    }

    #[test]
    fn given_a_settings_window_that_opens_when_it_asks_how_the_token_read_went_then_nothing_is_read(
    ) {
        // Given — la condition 1 d'ADR-0016 appliquée à une surface qui n'y pense pas :
        // ouvrir la section `usage` ne doit pas faire surgir un dialogue de trousseau. La
        // seule façon de le vérifier est de compter les questions posées
        let keychain = FakeKeychain::answering(vec![Ok(token("secret-abc"))]);
        let credentials = Credentials::from(Arc::clone(&keychain) as Arc<dyn TokenSource>);

        // When — la fenêtre s'ouvre avant que le fil de fond n'ait rien tenté, puis après
        let before_anything = credentials.readability();
        let _ = credentials.token();
        let after_the_background_thread_read = credentials.readability();
        let opened_ten_times: Vec<Readability> =
            (0..10).map(|_| credentials.readability()).collect();

        // Then — « rien tenté » n'est pas « en panne », et dix ouvertures n'ont posé aucune
        // question
        assert_eq!(before_anything, Readability::Untried);
        assert_eq!(after_the_background_thread_read, Readability::Readable);
        assert_eq!(opened_ten_times, vec![Readability::Readable; 10]);
        assert_eq!(keychain.asked(), 1);
    }

    #[test]
    fn given_the_ways_a_keychain_read_can_end_when_the_settings_window_names_them_then_a_refusal_an_absence_and_a_breakage_stay_three_different_things(
    ) {
        // Given — les conséquences d'ADR-0017 : « le jour où l'item change de nom ou de
        // forme, les quotas disparaissent en silence. C'est la conduite voulue, et elle se
        // diagnostiquera mal. » Les confondre laisserait l'utilisateur sans aucun moyen de
        // savoir s'il doit se connecter, autoriser, ou attendre un correctif
        let endings = [
            Ok(token("secret-abc")),
            Err(UsageError::NoToken),
            Err(UsageError::Refused),
            Err(UsageError::Unreadable),
        ];

        // When
        let named: Vec<Readability> = endings.iter().map(Readability::from).collect();

        // Then
        assert_eq!(
            named,
            vec![
                Readability::Readable,
                Readability::Absent,
                Readability::Refused,
                Readability::Unreadable,
            ]
        );
    }

    #[test]
    fn given_a_token_the_host_refused_when_the_settings_window_is_read_then_it_does_not_blame_the_keychain(
    ) {
        // Given — un `401` veut dire que l'hôte n'en veut plus, pas que le trousseau s'est
        // fermé. Afficher « illisible » enverrait l'utilisateur ouvrir Trousseaux d'accès
        // pour un problème qui n'y est pas
        let keychain = FakeKeychain::answering(vec![Ok(token("expired"))]);
        let credentials = Credentials::from(Arc::clone(&keychain) as Arc<dyn TokenSource>);
        let _ = credentials.token();

        // When
        credentials.forget();

        // Then
        assert_eq!(credentials.readability(), Readability::Readable);
    }

    #[test]
    fn given_a_token_when_anything_tries_to_print_it_then_the_secret_is_not_in_what_comes_out() {
        // Given — la condition 2 d'ADR-0017 : jamais dans un journal, un message d'erreur
        // ou un rapport de panique. `Debug` est la porte par laquelle un secret entre dans
        // une trace, et `assert_eq!` l'emprunte tout seul quand un test échoue
        let secret = token("sk-ant-oat-01-the-actual-secret");

        // When
        let printed = format!("{secret:?}");
        let inside_a_result = format!("{:?}", Ok::<_, UsageError>(secret.clone()));

        // Then
        assert!(!printed.contains("sk-ant"), "le jeton est dans {printed}");
        assert!(!inside_a_result.contains("sk-ant"));
        assert_eq!(printed, "AccessToken(<redacted>)");
    }

    #[test]
    fn given_the_invocation_of_security_when_it_is_composed_then_nothing_of_ash_can_enter_it() {
        // Given — la frontière de sécurité de l'en-tête, relue plutôt que supposée. Le
        // jour où quelqu'un ajoutera un argument venu d'ailleurs, c'est ce test qui
        // tombera — et il tombera avant que le nom du service ne devienne un paramètre
        let composed: Vec<&str> = std::iter::once(SECURITY).chain(FIND_PASSWORD).collect();

        // When — ce que `ps` verrait de cette ligne de commande
        let visible = composed.join(" ");

        // Then — un chemin absolu, un verbe, et un service constant : pas un octet de plus
        assert_eq!(
            visible,
            "/usr/bin/security find-generic-password -w -s Claude Code-credentials"
        );
        assert!(
            composed[0].starts_with('/'),
            "un `security` du PATH hérité du shell serait lancé à la place"
        );
        assert!(
            FIND_PASSWORD.contains(&"-w"),
            "sans -w, `security` rend une description de l'item plutôt que le seul secret"
        );
    }
}
