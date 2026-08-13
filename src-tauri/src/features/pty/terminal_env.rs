//! Ce qu'Ash **déclare** au shell d'un onglet — la surface qu'il lui offre.
//!
//! Rien ici n'est une préférence de l'utilisateur : c'est l'identité du terminal, et
//! aucun émulateur ne la laisse à l'héritage. Terminal.app et iTerm posent
//! `xterm-256color`, alacritty pose `alacritty` ; hériter serait faux même quand ça
//! marche, puisqu'un Ash lancé depuis alacritty donnerait au shell le terminfo
//! d'alacritty pour une surface qui est un xterm.js.
//!
//! Le défaut que ça corrige n'apparaissait qu'en application empaquetée : un `.app`
//! démarré par le Finder ou le Dock reçoit l'environnement de **launchd**, sans `TERM`
//! ni `LANG`. Sans `TERM` exploitable, zsh ne sait pas adresser le curseur, et ZLE
//! *ajoute* au lieu de remplacer à chaque redessin de la ligne — taper `ll` affichait
//! `llll`. En `bun run tauri dev`, le processus hérite du shell qui l'a lancé, et rien
//! ne se voyait.

/// Le terminfo de la surface d'Ash : xterm.js est un émulateur xterm.
const TERM: &str = "xterm-256color";

/// xterm.js rend les couleurs 24 bits, et c'est ainsi qu'on le dit.
const COLORTERM: &str = "truecolor";

/// Convention suivie par Terminal.app, iTerm et VS Code ; plusieurs configurations shell
/// la lisent pour s'adapter à leur hôte.
const TERM_PROGRAM: &str = "Ash";

/// Le repli de locale, et seulement quand l'environnement n'en porte aucune.
///
/// Sans locale UTF-8, zsh compte le `➜` d'un prompt comme trois caractères au lieu d'un,
/// ce qui décale toute l'arithmétique du redessin de la ligne.
const FALLBACK_LANG: &str = "en_US.UTF-8";

/// Les variables qu'Ash pose dans l'environnement du shell d'un onglet.
///
/// `TERM`, `COLORTERM` et `TERM_PROGRAM` sont posés **toujours** : ils décrivent la
/// surface, pas un goût. La locale, elle, appartient à l'utilisateur — Ash ne comble
/// qu'un vide, et n'écrase jamais un `LANG` ou un `LC_ALL` existant.
///
/// Ce que l'environnement du processus Ash porte déjà est lu ici, et nulle part ailleurs :
/// l'appelant n'a pas à connaître cette lecture pour poser ces variables.
///
/// Suite possible, hors périmètre ici : dériver la vraie locale de macOS (`AppleLocale`,
/// dans les préférences globales) plutôt que de se rabattre sur l'anglais. Ça demande un
/// appel système, donc un port de plus dans la feature.
pub fn terminal_env() -> Vec<(String, String)> {
    declared(&ambient)
}

/// La règle, séparée de la lecture de l'environnement qui l'alimente.
///
/// `is_set` dit si l'environnement ambiant porte déjà une variable utilisable. La
/// décision est ainsi une **fonction pure de ce qu'on lui donne**, et ses tests ne
/// dépendent pas de la machine qui les exécute. Cette couture reste **interne** : elle
/// n'existe que pour les tests de ce module, et rien au dehors n'a à la connaître.
fn declared(is_set: &dyn Fn(&str) -> bool) -> Vec<(String, String)> {
    let mut posed = vec![
        ("TERM", TERM),
        ("COLORTERM", COLORTERM),
        ("TERM_PROGRAM", TERM_PROGRAM),
    ];

    if !is_set("LC_ALL") && !is_set("LANG") {
        posed.push(("LANG", FALLBACK_LANG));
    }

    posed
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

/// L'environnement du processus Ash porte-t-il une valeur utilisable pour cette variable ?
///
/// Une variable vide vaut absente : un `LANG=` ne dit pas plus la locale qu'un `LANG`
/// manquant, et laisserait le prompt aussi faux.
fn ambient(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un environnement décrit par le scénario, et rien d'autre — surtout pas celui de la
    /// machine de test.
    fn carrying(present: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |name| present.contains(&name)
    }

    fn value_of<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
        env.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn given_an_environment_without_any_terminal_when_a_shell_is_started_then_ash_declares_its_own()
    {
        // Given — l'environnement de launchd : il ne porte rien.
        let empty = carrying(&[]);

        // When
        let env = declared(&empty);

        // Then
        assert_eq!(value_of(&env, "TERM"), Some("xterm-256color"));
        assert_eq!(value_of(&env, "COLORTERM"), Some("truecolor"));
    }

    #[test]
    fn given_an_environment_already_carrying_a_terminal_when_a_shell_is_started_then_ash_still_declares_its_own(
    ) {
        // Given — Ash lancé depuis un autre terminal : son `TERM` décrit *sa* surface.
        let inherited = carrying(&["TERM", "COLORTERM"]);

        // When
        let env = declared(&inherited);

        // Then — jamais conditionnel : la surface reste celle d'Ash.
        assert_eq!(value_of(&env, "TERM"), Some("xterm-256color"));
        assert_eq!(value_of(&env, "COLORTERM"), Some("truecolor"));
    }

    #[test]
    fn given_an_environment_without_any_locale_when_a_shell_is_started_then_a_utf8_one_is_supplied()
    {
        // Given
        let without_locale = carrying(&[]);

        // When
        let env = declared(&without_locale);

        // Then
        assert_eq!(value_of(&env, "LANG"), Some("en_US.UTF-8"));
    }

    #[test]
    fn given_a_lang_chosen_by_the_user_when_a_shell_is_started_then_ash_leaves_it_alone() {
        // Given
        let with_lang = carrying(&["LANG"]);

        // When
        let env = declared(&with_lang);

        // Then — la locale appartient à l'utilisateur ; poser la nôtre l'écraserait, le
        // `spec.env` ayant le dernier mot sur l'héritage.
        assert_eq!(value_of(&env, "LANG"), None);
    }

    #[test]
    fn given_an_lc_all_that_governs_every_category_when_a_shell_is_started_then_no_lang_is_added() {
        // Given — `LC_ALL` l'emporte sur `LANG` : poser un `LANG` ici ne changerait rien
        // pour le shell, mais mentirait sur ce qu'Ash a décidé.
        let with_lc_all = carrying(&["LC_ALL"]);

        // When
        let env = declared(&with_lc_all);

        // Then
        assert_eq!(value_of(&env, "LANG"), None);
    }
}
