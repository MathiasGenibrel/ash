//! Ce qui peut empêcher un quota d'exister — et rien d'autre.
//!
//! **Aucune variante ne porte de donnée, et c'est une garantie de sécurité, pas une
//! économie de code.** [ADR-0017](../../../../docs/adr/0017-ash-lit-le-jeton-de-l-outil.md)
//! interdit au jeton d'apparaître « dans un journal, dans un fichier écrit par Ash, dans un
//! `argv`, dans un message d'erreur, ni dans un rapport de panique ». Un type d'erreur sans
//! champ rend la deuxième et la quatrième interdictions **structurelles** : il n'existe
//! aucun endroit où glisser un en-tête, un corps de réponse ou une sortie de `security`,
//! donc aucun `{why}` ne peut en recopier un par distraction.
//!
//! C'est aussi pour cette raison qu'on ne garde pas l'erreur d'`ureq` : elle sait rendre
//! l'URL appelée, et une URL est le début du chemin par lequel un en-tête finit dans une
//! trace.

/// Pourquoi Ash ne sait rien de l'usage du compte.
///
/// Toutes ces valeurs ont **le même effet visible** : les quotas disparaissent, et rien
/// n'est signalé (condition 3 d'ADR-0016). Elles se distinguent parce que le poller n'en
/// tire pas la même conduite — voir `Credentials` pour `Refused` et `NoToken`, et
/// [`UsagePoller`](super::UsagePoller) pour `Unauthorized`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageError {
    /// Le trousseau n'a pas l'item : personne n'est connecté à l'outil, ou il l'appelle
    /// autrement. **Aucun dialogue macOS n'est apparu** — un item absent ne se demande pas.
    NoToken,
    /// La lecture du trousseau a échoué autrement que par une absence : l'utilisateur a
    /// refusé, ou macOS a répondu quelque chose qu'on ne sait pas lire.
    ///
    /// C'est **définitif** (condition 4 d'ADR-0017) : Ash ne redemande pas.
    Refused,
    /// Le contenu de l'item n'est pas le document attendu — item renommé, format changé.
    Unreadable,
    /// L'appel n'a pas abouti : hors ligne, DNS, TLS, délai dépassé, proxy injoignable.
    Unreachable,
    /// L'hôte a refusé le jeton (401, 403). Le seul cas où Ash relit le trousseau : c'est
    /// la forme que prend l'expiration d'un jeton OAuth que l'outil vient de renouveler.
    Unauthorized,
    /// L'hôte a répondu autre chose qu'un succès — 429, 5xx, ou un statut inattendu.
    Rejected,
}

impl std::fmt::Display for UsageError {
    /// Des phrases **fixes**, sans interpolation d'aucune sorte : voir l'en-tête du module.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let said = match self {
            Self::NoToken => "no usage token in the keychain",
            Self::Refused => "the keychain did not give up the usage token",
            Self::Unreadable => "the usage token item is not in the expected shape",
            Self::Unreachable => "the usage endpoint could not be reached",
            Self::Unauthorized => "the usage endpoint refused the token",
            Self::Rejected => "the usage endpoint answered something else than a usage report",
        };
        f.write_str(said)
    }
}

impl std::error::Error for UsageError {}
