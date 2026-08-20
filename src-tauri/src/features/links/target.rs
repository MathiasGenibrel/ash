//! Ce qu'un mot peint dans un terminal a le droit de devenir — **la troisième frontière de
//! sécurité du dépôt**.
//!
//! Elle se lit après les deux autres, et pour la même raison qu'elles existent :
//! `features/git/git_cli.rs` (visiter un dépôt hostile ne doit rien exécuter) et
//! `features/usage/token.rs` (rien de ce qu'Ash a lu n'atteint la ligne de commande de
//! `security`). Celle-ci est la plus exposée des trois, et il faut le dire tel quel :
//!
//! > **La sortie d'un PTY est du texte hostile.** Un `cat` d'un fichier piégé, un agent qui
//! > relaie une page web, un `curl` d'un serveur quelconque peuvent peindre
//! > `javascript:…`, `file:///…` ou `data:…` dans le terminal, avec les couleurs qu'ils
//! > veulent. L'utilisateur qui clique ne sait pas d'où vient le texte.
//!
//! ## La frontière, en une phrase
//!
//! **Rien ne devient ouvrable sans passer par [`resolve`], et rien d'autre ne fabrique un
//! [`LinkTarget`].** Le type est un jeton : son champ est privé, son seul constructeur est
//! dans ce fichier, et `opener.rs` ne sait lancer que lui. Le frontend, lui, ne fabrique
//! rien du tout — il envoie des mots, et redemande à chaque ouverture (voir `commands.rs`) :
//! ce qui a été souligné à l'écran n'est **jamais** ce qui autorise l'ouverture.
//!
//! ## Les trois règles, et ce que chacune empêche
//!
//! | Règle | Ce qu'elle empêche |
//! |---|---|
//! | **Liste blanche** de schémas — `http://` et `https://`, et rien d'autre | qu'un schéma que personne n'a en tête parte vers LaunchServices. Une liste noire en oublie toujours un, et macOS en enregistre que l'utilisateur n'a jamais nommés : `javascript:`, `data:`, `file:`, `vbscript:` sont les quatre qu'on cite, ils ne sont pas les seuls, et c'est précisément l'argument |
//! | Un chemin n'existe **sur disque** que si [`Files`] le dit | qu'un mot qui ressemble à un chemin soit cliquable. Le refus est ici un défaut, pas une exception : ce qui n'a pas été vérifié reste du texte |
//! | Un chemin est **révélé**, jamais ouvert | qu'un `.sh`, un `.app`, un binaire exécutable soit lancé. La garantie est portée par `opener.rs` (`open -R`), et ce module la rend inévitable en n'ayant pas d'autre variante pour un chemin |
//!
//! ## L'invariant que `opener.rs` peut donc supposer
//!
//! **Un [`LinkTarget`] ne porte jamais une valeur qui commence par `-`.** C'est ce qui
//! empêche un mot du terminal d'être lu comme une **option** par le binaire qu'`opener.rs`
//! lance.
//!
//! L'invariant n'est pas une promesse de commentaire : il est **vérifié dans le seul
//! constructeur du type**, `LinkTarget::of`, que les deux branches de [`resolve`]
//! franchissent et qu'une troisième branche ne pourrait pas contourner. Un chemin doit y
//! être absolu, une URL ne doit pas commencer par `-`.
//!
//! Il est ensuite vérifié une **seconde** fois, ailleurs et autrement : par le `--` de
//! l'invocation, dans `opener.rs`. Les deux, parce qu'aucune des deux ne se relit quand on
//! ajoute une variante six mois plus tard — et parce qu'elles ne tombent pas ensemble, un
//! type d'un côté, un `argv` de l'autre.
//!
//! ## Ce que ce module ne décide pas
//!
//! Il ne décide pas **quels** mots lui sont soumis : la découpe d'une ligne en candidats
//! est un fait d'affichage, elle vit dans `src/features/terminal/link-scan.ts` et n'a
//! aucune autorité. Un candidat farfelu n'est pas un risque — il est refusé ici.

use std::path::{Path, PathBuf};

use super::files::Files;

/// La longueur au-delà de laquelle un candidat n'est plus regardé.
///
/// Une ligne de terminal peut faire des milliers de colonnes, et une sortie hostile peut
/// peindre un « chemin » d'un mégaoctet. 4096 est la limite de `PATH_MAX` sur macOS pour
/// un chemin complet ; au-delà, il n'y a rien à ouvrir, seulement du travail à donner au
/// système de fichiers.
const LONGEST_CANDIDATE: usize = 4096;

/// Les deux seuls schémas qu'Ash ouvre. Voir l'en-tête du module : **une liste blanche**.
const OPENABLE_SCHEMES: [&str; 2] = ["http://", "https://"];

/// Ce qu'Ash accepte d'ouvrir, et la preuve que quelqu'un l'a vérifié.
///
/// Le champ est privé et le seul constructeur est [`resolve`] : hors de ce fichier, il n'y
/// a aucun moyen d'en fabriquer un, donc aucun moyen de faire parvenir une valeur non
/// vérifiée à `opener.rs`. C'est la même idée que `Invocation` dans
/// `features/git/git_cli.rs`, appliquée à une entrée bien plus hostile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    kind: Kind,
}

/// Les deux formes d'ouverture, et il n'y en aura pas de troisième sans relire l'en-tête.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Kind {
    /// Une URL `http(s)`, à confier au navigateur par défaut.
    Browse(String),
    /// Un chemin qui existe, à **révéler** dans le Finder. Jamais à exécuter.
    Reveal(PathBuf),
}

impl LinkTarget {
    /// **Le seul endroit du dépôt où un [`LinkTarget`] naît**, et donc le seul endroit où
    /// l'invariant de l'en-tête est *vérifié* plutôt que raisonné.
    ///
    /// Il l'était jusqu'ici par construction — un chemin sort absolu de [`absolute`], une
    /// URL commence par `http` —, ce qui est vrai mais ne se relit pas : il fallait suivre
    /// trois fonctions pour s'en convaincre, et une quatrième branche ajoutée plus tard
    /// n'aurait rien eu à franchir. Ici, elle ne peut pas ne pas le franchir, et une
    /// variante nouvelle **doit** dire comment elle satisfait l'invariant.
    ///
    /// Le cas qui n'était pas couvert : `absolute` joint un `~/…` à `home` sans vérifier
    /// que `home` est absolu. Un `HOME` relatif — que le processus hérite, donc qu'Ash ne
    /// choisit pas — rendait un `Reveal` relatif, et seul le `--` d'`opener.rs` tenait
    /// encore. La double protection reste double, mais les deux barrières sont de nouveau
    /// **différentes** : celle-ci est un type, l'autre est un `argv`.
    fn of(kind: Kind) -> Option<Self> {
        let could_be_read_as_an_option = match &kind {
            // Une URL sort de la liste blanche, donc commence par `http`. La condition est
            // écrite quand même : c'est elle que la variante suivante viendra copier.
            Kind::Browse(url) => url.starts_with('-'),
            // Un chemin absolu commence par `/`, donc jamais par `-`.
            Kind::Reveal(path) => !path.is_absolute(),
        };
        (!could_be_read_as_an_option).then_some(Self { kind })
    }

    pub(super) fn kind(&self) -> &Kind {
        &self.kind
    }
}

/// Ce qu'un candidat devient, ou `None` — qui est la réponse par défaut.
///
/// `cwd` est le répertoire courant de l'onglet, celui que la sonde d'
/// [ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md) suit à travers les `cd` :
/// c'est lui qui donne à Ash ce qu'un terminal ordinaire n'a pas, la capacité de résoudre
/// un chemin **relatif** sans se tromper. `home` étend le `~`, et vaut `None` quand
/// l'environnement n'a pas de `HOME` — auquel cas un `~/…` n'est simplement pas ouvrable.
pub fn resolve(
    candidate: &str,
    cwd: &Path,
    home: Option<&Path>,
    files: &dyn Files,
) -> Option<LinkTarget> {
    let candidate = candidate.trim();
    if candidate.is_empty() || candidate.len() > LONGEST_CANDIDATE {
        return None;
    }
    // Un caractère de contrôle dans un « chemin » n'a qu'une provenance plausible : une
    // sortie qui essaie de composer autre chose que ce qui s'affiche. `\0` en particulier
    // tronquerait l'argument passé au système.
    if candidate.chars().any(char::is_control) {
        return None;
    }

    if let Some(url) = as_url(candidate) {
        return LinkTarget::of(Kind::Browse(url));
    }

    // **Tout ce qui porte un schéma s'arrête ici**, y compris `file:`, `data:`,
    // `javascript:` et `vbscript:`. Sans ce refus, `file:///etc/passwd` retomberait dans
    // la branche « chemin » par un chemin détourné, et un `data:` deviendrait un nom de
    // fichier relatif que quelqu'un pourrait un jour poser dans un dépôt.
    if has_scheme(candidate) {
        return None;
    }

    let path = absolute(candidate, cwd, home)?;
    // La seule question posée au disque, et elle est posée **avant** toute ouverture :
    // c'est la deuxième des trois règles de l'en-tête. Voir `files.rs` pour ce que
    // « exister » veut dire exactement.
    if !files.exists(&path) {
        return None;
    }
    LinkTarget::of(Kind::Reveal(path))
}

/// L'URL, si et seulement si son schéma est l'un des deux.
///
/// La comparaison est insensible à la casse **sur l'ASCII seulement** : `HTTPS://` est le
/// même schéma, mais `to_lowercase()` sur la chaîne entière abîmerait le reste de l'URL —
/// un chemin d'URL est sensible à la casse — et ferait entrer les règles de casse Unicode
/// dans une décision de sécurité, ce qui est exactement le genre d'endroit où le turc `İ`
/// finit par surprendre.
fn as_url(candidate: &str) -> Option<String> {
    let is_openable = OPENABLE_SCHEMES
        .iter()
        .any(|scheme| candidate.len() > scheme.len() && starts_with_ascii(candidate, scheme));
    is_openable.then(|| candidate.to_owned())
}

fn starts_with_ascii(candidate: &str, prefix: &str) -> bool {
    candidate
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Vrai si le candidat commence par `<schéma>:` au sens de la RFC 3986.
///
/// La règle est volontairement **large** : une lettre, puis des lettres, chiffres, `+`,
/// `-` ou `.`, puis `:`. Elle attrape donc plus de choses qu'un vrai schéma — mais elle
/// n'autorise rien : elle **refuse**. Un candidat de trop est un mot qui reste du texte ;
/// un candidat manqué serait un schéma qui passe.
fn has_scheme(candidate: &str) -> bool {
    let Some(colon) = candidate.find(':') else {
        return false;
    };
    let scheme = &candidate[..colon];
    let mut letters = scheme.chars();
    letters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && letters.all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '+' | '-' | '.'))
}

/// Le chemin absolu que le candidat désigne, ou `None` s'il n'en désigne aucun.
///
/// **La sortie est toujours absolue**, et c'est l'invariant que documente l'en-tête : un
/// `cwd` relatif — qui ne devrait pas arriver, mais qui viendrait du frontend — fait
/// refuser le candidat relatif plutôt que produire un chemin ambigu.
fn absolute(candidate: &str, cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(rest) = candidate.strip_prefix("~/") {
        return Some(home?.join(rest));
    }
    if candidate == "~" {
        return Some(home?.to_path_buf());
    }
    // `~autre/…` n'est pas étendu : le dossier d'un autre utilisateur se lit dans
    // `/Users/…`, et deviner l'emplacement d'un compte demanderait d'interroger le
    // répertoire des utilisateurs pour un gain nul.
    if candidate.starts_with('~') {
        return None;
    }

    let path = Path::new(candidate);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    if !cwd.is_absolute() {
        return None;
    }
    Some(cwd.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::links::files::FakeFiles;

    const CWD: &str = "/dev/ash";
    const HOME: &str = "/Users/moi";

    fn nothing_exists() -> FakeFiles {
        FakeFiles::with([])
    }

    fn resolved(candidate: &str, files: &FakeFiles) -> Option<LinkTarget> {
        resolve(candidate, Path::new(CWD), Some(Path::new(HOME)), files)
    }

    #[test]
    fn given_an_https_url_when_resolving_then_the_browser_gets_it_untouched() {
        // Given
        let files = nothing_exists();
        // When
        let target = resolved("https://example.com/a/B?q=1", &files);
        // Then
        assert_eq!(
            target.map(|it| it.kind().clone()),
            Some(Kind::Browse("https://example.com/a/B?q=1".to_owned()))
        );
    }

    #[test]
    fn given_a_scheme_in_capitals_when_resolving_then_it_is_the_same_scheme_and_the_rest_keeps_its_case(
    ) {
        // Given
        let files = nothing_exists();
        // When
        let target = resolved("HTTPS://example.com/Path", &files);
        // Then
        assert_eq!(
            target.map(|it| it.kind().clone()),
            Some(Kind::Browse("HTTPS://example.com/Path".to_owned()))
        );
    }

    /// Le critère d'acceptation de l'issue #126, et la raison d'être du module.
    #[test]
    fn given_a_scheme_outside_the_whitelist_when_resolving_then_nothing_is_openable() {
        // Given — chacun de ces mots peut être peint par une sortie de PTY
        let files = FakeFiles::with(["/dev/ash/javascript:alert(1)", "/etc/passwd"]);
        let hostile = [
            "javascript:alert(1)",
            "data:text/html;base64,PHNjcmlwdD4=",
            "file:///etc/passwd",
            "vbscript:msgbox(1)",
            "x-apple-systempreferences:com.apple.preference",
            "ftp://example.com/x",
            "mailto:someone@example.com",
            "ash://select-tab",
        ];
        for candidate in hostile {
            // When
            let target = resolved(candidate, &files);
            // Then
            assert_eq!(target, None, "{candidate} ne doit jamais être ouvrable");
        }
    }

    #[test]
    fn given_a_relative_path_that_exists_in_the_tab_cwd_when_resolving_then_it_resolves_against_that_cwd(
    ) {
        // Given
        let files = FakeFiles::with(["/dev/ash/src/features/terminal/index.ts"]);
        // When
        let target = resolved("src/features/terminal/index.ts", &files);
        // Then
        assert_eq!(
            target.map(|it| it.kind().clone()),
            Some(Kind::Reveal(PathBuf::from(
                "/dev/ash/src/features/terminal/index.ts"
            )))
        );
    }

    #[test]
    fn given_a_word_that_looks_like_a_path_but_exists_nowhere_when_resolving_then_it_stays_text() {
        // Given
        let files = FakeFiles::with(["/dev/ash/src/real.ts"]);
        // When
        let target = resolved("src/imaginary.ts", &files);
        // Then
        assert_eq!(target, None);
    }

    #[test]
    fn given_a_tilde_path_when_resolving_then_it_is_expanded_against_home() {
        // Given
        let files = FakeFiles::with(["/Users/moi/.ash/theme.json"]);
        // When
        let target = resolved("~/.ash/theme.json", &files);
        // Then
        assert_eq!(
            target.map(|it| it.kind().clone()),
            Some(Kind::Reveal(PathBuf::from("/Users/moi/.ash/theme.json")))
        );
    }

    #[test]
    fn given_no_home_in_the_environment_when_resolving_a_tilde_path_then_it_stays_text() {
        // Given
        let files = FakeFiles::with(["/Users/moi/.ash/theme.json"]);
        // When
        let target = resolve("~/.ash/theme.json", Path::new(CWD), None, &files);
        // Then
        assert_eq!(target, None);
    }

    #[test]
    fn given_an_executable_file_when_resolving_then_it_is_a_reveal_like_any_other_path() {
        // Given — la garantie « Ash n'exécute rien » est structurelle : il n'existe pas de
        // variante qui lance quoi que ce soit, quelle que soit l'extension.
        let files = FakeFiles::with(["/dev/ash/scripts/deploy.sh", "/Applications/Mail.app"]);
        // When
        let script = resolved("scripts/deploy.sh", &files);
        let bundle = resolved("/Applications/Mail.app", &files);
        // Then
        assert_eq!(
            script.map(|it| it.kind().clone()),
            Some(Kind::Reveal(PathBuf::from("/dev/ash/scripts/deploy.sh")))
        );
        assert_eq!(
            bundle.map(|it| it.kind().clone()),
            Some(Kind::Reveal(PathBuf::from("/Applications/Mail.app")))
        );
    }

    #[test]
    fn given_a_candidate_that_starts_with_a_dash_when_resolving_then_what_comes_out_is_still_absolute(
    ) {
        // Given — l'invariant sur lequel `opener.rs` s'appuie : rien de ce qui sort d'ici
        // ne peut être lu comme une option par le binaire lancé.
        let files = FakeFiles::with(["/dev/ash/-rf"]);
        // When
        let target = resolved("-rf", &files);
        // Then
        assert_eq!(
            target.map(|it| it.kind().clone()),
            Some(Kind::Reveal(PathBuf::from("/dev/ash/-rf")))
        );
    }

    #[test]
    fn given_a_home_that_is_not_absolute_when_resolving_a_tilde_path_then_it_stays_text() {
        // Given — `HOME` est hérité du processus, donc Ash ne le choisit pas ; un `HOME`
        // relatif rendrait une valeur qu'`open` pourrait lire comme une option.
        let files = FakeFiles::with(["-rf/.ash/theme.json"]);
        // When
        let target = resolve(
            "~/.ash/theme.json",
            Path::new(CWD),
            Some(Path::new("-rf")),
            &files,
        );
        // Then
        assert_eq!(target, None);
    }

    #[test]
    fn given_a_relative_candidate_and_a_cwd_that_is_not_absolute_when_resolving_then_it_stays_text()
    {
        // Given
        let files = FakeFiles::with(["src/x.ts"]);
        // When
        let target = resolve(
            "src/x.ts",
            Path::new("dev/ash"),
            Some(Path::new(HOME)),
            &files,
        );
        // Then
        assert_eq!(target, None);
    }

    #[test]
    fn given_a_candidate_carrying_a_control_character_when_resolving_then_it_stays_text() {
        // Given — une sortie hostile compose ce qu'elle veut, y compris un `\0` qui
        // tronquerait l'argument une fois arrivé au système.
        let files = FakeFiles::with(["/dev/ash/notes.md"]);
        // When
        let target = resolved("notes.md\u{0}/../../etc", &files);
        // Then
        assert_eq!(target, None);
    }

    #[test]
    fn given_a_candidate_longer_than_a_path_when_resolving_then_the_disk_is_never_asked() {
        // Given
        let files = FakeFiles::with([]);
        let painted = format!("/{}", "a".repeat(LONGEST_CANDIDATE));
        // When
        let target = resolved(&painted, &files);
        // Then
        assert_eq!(target, None);
        assert_eq!(files.asked(), 0);
    }
}
