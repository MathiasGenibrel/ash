//! L'ouverture elle-même — **le troisième binaire externe qu'Ash lance**, et le premier
//! dont l'`argv` porte une donnée venue d'ailleurs.
//!
//! `features/usage/token.rs` a une invocation **constante** : « il n'y a pas de paramètre,
//! donc pas de chemin par lequel un nom de dépôt, un `cwd` sondé ou une réponse d'API
//! pourrait y entrer ». Ici, c'est l'inverse — le dernier argument **est** un mot que la
//! sortie d'un PTY a peint. C'est la différence qui donne son contenu à ce fichier, et
//! c'est elle qu'il faut avoir en tête avant d'y ajouter quoi que ce soit.
//!
//! ## Ce que chaque décision achète
//!
//! | Décision | Ce qu'elle empêche |
//! |---|---|
//! | Chemin **absolu** [`OPEN`] | qu'un `open` posé dans le `PATH` hérité du shell de l'utilisateur soit lancé à la place. C'est la même raison qu'`/usr/bin/security` |
//! | Le schéma est validé **avant** le lancement | qu'un `javascript:` atteigne LaunchServices. La validation n'est pas ici : elle est dans `target.rs`, et le typage la rend inévitable — cette fonction ne sait lancer qu'un [`LinkTarget`], que rien d'autre que `resolve` ne fabrique |
//! | `--` avant la valeur | qu'un mot commençant par `-` soit lu comme une **option** de `open`. `target.rs` garantit déjà qu'aucune valeur ne commence par `-` ; le `--` est la seconde barrière, celle qui tiendra le jour où une variante sera ajoutée sans qu'on relise l'autre fichier |
//! | `-R` pour un chemin | qu'un `.sh`, un `.app` ou un binaire soit **exécuté**. `-R` révèle dans le Finder et ne lance jamais rien, quel que soit le mode ou l'extension du fichier |
//! | `current_dir("/")` | qu'une URL soit réinterprétée comme un fichier **relatif** : `open` résout ses arguments depuis le répertoire courant du processus fils, et hériter du `cwd` d'Ash lui donnerait un dossier où l'utilisateur a des fichiers. Depuis `/`, il faudrait un `/https:` pour tromper qui que ce soit |
//! | `stdin`, `stdout`, `stderr` fermés | qu'une sortie de `open` entre dans le processus, et qu'un fils hérite d'un descripteur de PTY |
//! | Aucun argument composé par formatage | qu'une valeur soit collée à une autre. Chaque élément de l'`argv` est un élément, et il n'y a pas de shell — `Command` ne passe par `/bin/sh` nulle part |
//!
//! ## Pourquoi `/usr/bin/open` et pas `NSWorkspace`
//!
//! `objc2` et `objc2-foundation` sont déjà dans l'arbre, et `NSWorkspace` ferait les deux
//! gestes — `openURL:` et `selectFile:inFileViewerRootedAtPath:` — sans processus fils ni
//! `argv`. C'est un vrai argument, et il a été pesé.
//!
//! Il a été écarté au même barème que `NSURLSession` la veille
//! ([ADR-0016](../../../../docs/adr/0016-ash-sort-sur-le-reseau.md), alternatives
//! écartées) : ce serait un **troisième module `unsafe`** après la sonde et les
//! notifications, alors que « les deux modules `unsafe` existants sont là parce qu'aucune
//! bibliothèque sûre ne faisait le travail ; ce n'est pas le cas ici ». `unsafe_code` est
//! en `warn` dans le `Cargo.toml`, donc en **erreur** sous `clippy -- -D warnings` : chaque
//! `unsafe` du crate est un `#[allow]` explicite, et en poser un troisième pour économiser
//! un `fork` sur un geste que l'utilisateur déclenche à la main est un mauvais échange.
//!
//! Ce que ce choix coûte, et qu'il faut savoir : `open` décide lui-même si son argument est
//! un fichier ou une URL, là où `NSWorkspace` a deux méthodes distinctes. Le
//! `current_dir("/")` ci-dessus est ce qui rend ce reste inoffensif, et il est la seule
//! raison pour laquelle il est là. Le jour où le fils devient gênant pour une autre raison,
//! c'est cette ligne du barème qu'il faudra rouvrir.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

use super::error::LinkError;
use super::target::{Kind, LinkTarget};

/// Le binaire, par son chemin absolu. Voir l'en-tête.
const OPEN: &str = "/usr/bin/open";

/// Le répertoire courant du processus fils. Voir l'en-tête : il n'y a rien à y trouver.
const NOWHERE: &str = "/";

/// Qui sait ouvrir. Un port, pour que **aucun `cargo test` n'ouvre le Finder ni un
/// navigateur** sur la machine de qui le lance.
pub trait Opener: Send + Sync {
    fn open(&self, target: &LinkTarget) -> Result<(), LinkError>;
}

/// L'`argv` que reçoit [`OPEN`], sans le nom du binaire.
///
/// Extraite pour que la composition — et pas une liste recopiée à côté — soit ce que les
/// tests du bas de fichier relisent. C'est la forme retenue par
/// `features/git/git_cli.rs` pour ses invocations, et pour la même raison.
fn argv(target: &LinkTarget) -> Vec<OsString> {
    match target.kind() {
        // Pas de `-R` : LaunchServices confie une URL `http(s)` au navigateur par défaut.
        Kind::Browse(url) => vec![OsString::from("--"), OsString::from(url)],
        // `-R` **révèle**, il ne lance pas. C'est la garantie « Ash n'exécute rien », et
        // elle tient sur ce seul mot.
        Kind::Reveal(path) => vec![
            OsString::from("-R"),
            OsString::from("--"),
            path.as_os_str().to_owned(),
        ],
    }
}

/// LaunchServices, par le binaire que macOS fournit.
pub struct LaunchServices;

impl Opener for LaunchServices {
    fn open(&self, target: &LinkTarget) -> Result<(), LinkError> {
        let status = Command::new(Path::new(OPEN))
            .args(argv(target))
            .current_dir(NOWHERE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| LinkError::Unopenable)?;

        // Pas de délai d'attente, contrairement à `git_cli.rs` : `open` rend la main dès que
        // LaunchServices a **pris** la demande, sans attendre que l'application se lance. Il
        // n'y a donc rien à borner, et le fils ne survit pas à l'appel.
        if status.success() {
            Ok(())
        } else {
            Err(LinkError::Unopenable)
        }
    }
}

#[cfg(test)]
pub struct FakeOpener {
    opened: std::sync::Mutex<Vec<LinkTarget>>,
}

#[cfg(test)]
impl FakeOpener {
    pub fn new() -> Self {
        Self {
            opened: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn opened(&self) -> Vec<LinkTarget> {
        self.opened
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone()
    }
}

#[cfg(test)]
impl Opener for FakeOpener {
    fn open(&self, target: &LinkTarget) -> Result<(), LinkError> {
        if let Ok(mut opened) = self.opened.lock() {
            opened.push(target.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::links::files::FakeFiles;
    use crate::features::links::target::resolve;
    use std::path::Path;

    fn target(candidate: &str, present: &[&str]) -> LinkTarget {
        let files = FakeFiles::with(present.iter().copied());
        resolve(
            candidate,
            Path::new("/dev/ash"),
            Some(Path::new("/Users/moi")),
            &files,
        )
        .expect("le candidat de ce test est ouvrable")
    }

    #[test]
    fn given_a_path_to_reveal_when_composing_the_invocation_then_it_reveals_and_never_launches() {
        // Given
        let deploy = target("scripts/deploy.sh", &["/dev/ash/scripts/deploy.sh"]);
        // When
        let argv = argv(&deploy);
        // Then
        assert_eq!(
            argv,
            vec![
                OsString::from("-R"),
                OsString::from("--"),
                OsString::from("/dev/ash/scripts/deploy.sh"),
            ]
        );
    }

    #[test]
    fn given_a_url_when_composing_the_invocation_then_it_is_the_last_argument_after_a_double_dash()
    {
        // Given
        let url = target("https://example.com/x", &[]);
        // When
        let argv = argv(&url);
        // Then
        assert_eq!(
            argv,
            vec![
                OsString::from("--"),
                OsString::from("https://example.com/x")
            ]
        );
    }

    #[test]
    fn given_any_openable_candidate_when_composing_the_invocation_then_nothing_can_be_read_as_an_option(
    ) {
        // Given — la valeur porte un mot qui, seul, serait une option de `open`
        let dashes = target("-R", &["/dev/ash/-R"]);
        // When
        let argv = argv(&dashes);
        // Then — le `--` est là, et la valeur vient après lui
        let separator = argv
            .iter()
            .position(|argument| argument == "--")
            .expect("toute invocation porte son `--`");
        assert_eq!(argv.len() - 1, separator + 1);
        assert_eq!(argv[argv.len() - 1], OsString::from("/dev/ash/-R"));
    }
}
