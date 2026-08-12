//! Les effets système de la vérification, derrière deux traits que la feature possède.
//!
//! Les quatre tests de la spec §9.1 sont les seules choses de `settings` qui touchent le
//! monde : trois lisent un dossier et le `PATH`, le quatrième **lance un processus**. Sans
//! ces traits, aucune des règles de [`super::verification`] ne serait vérifiable sans un
//! vrai `~/.claude` sur la machine de celui qui lance `cargo test`.
//!
//! Deux traits et non un seul : lire un dossier et lancer un programme n'ont ni le même
//! risque, ni le même coût, ni le même double. [`ConfigFiles`] est instantané et sans
//! danger ; [`CommandRunner`] est le seul endroit de la feature qui fait exister un
//! processus, et c'est là que se lit la frontière de sécurité.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::hooks::BlockAt;

/// Ce qu'on trouve à un chemin de configuration — le test 1, en entier.
///
/// Les trois façons d'échouer sont distinguées parce que **la correction qui a une chance
/// en dépend** : un dossier absent se remplace, un fichier se remonte d'un cran, un dossier
/// illisible se déverrouille. Les confondre en un `bool` ferait proposer trois fois le même
/// conseil générique.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Folder {
    /// Le dossier existe et se lit. Porte les noms de ses entrées directes — c'est déjà
    /// tout ce dont le test 2 a besoin, et ça évite un second aller-retour au disque.
    Readable(Vec<String>),
    /// Rien n'existe à ce chemin.
    Missing,
    /// Quelque chose y est, mais ce n'est pas un dossier.
    NotADirectory,
    /// Le dossier est là, et le système refuse de l'ouvrir.
    Unreadable,
}

/// Le système de fichiers, tel que la vérification en a besoin.
///
/// Une seule lecture, et le foyer : les chemins de configuration s'écrivent `~/.claude`
/// dans la spec §9 comme dans la maquette, et `~` n'est pas un dossier — c'est une
/// convention de shell qu'il faut résoudre avant de toucher au disque.
pub trait ConfigFiles: Send + Sync {
    fn read_folder(&self, path: &Path) -> Folder;

    /// Le dossier personnel, ou `None` si l'environnement n'en désigne aucun.
    fn home(&self) -> Option<PathBuf>;
}

/// Ce qu'Ash s'apprête à lancer, en entier et sans rien d'implicite.
///
/// **C'est la frontière de sécurité du test 4**, et elle est une structure de données
/// précisément pour ça : ce qui sera lancé est descriptible, donc affichable à
/// l'utilisateur (la maquette montre la commande réellement lancée) et **assertable dans un
/// test** sans lancer quoi que ce soit.
///
/// Trois invariants la tiennent, et [`super::verification`] est seul à la construire :
///
/// 1. **`program` est le chemin qu'a résolu le `PATH`**, jamais une saisie de
///    l'utilisateur. Le test 3 le trouve, le test 4 le lance : Ash ne lance donc jamais
///    autre chose que ce que taper le nom dans un shell aurait lancé. Le nom lui-même est
///    déjà contraint par [`SettingsError::NotACommandName`](super::SettingsError) — ni
///    espace, ni barre oblique.
/// 2. **`args` vient de l'adaptateur**, jamais du chemin de configuration ni d'un champ de
///    l'écran. Un chemin ne devient jamais un argument : il ne voyage que par
///    l'environnement, où rien ne l'interprète.
/// 3. **`env` est la totalité de l'environnement du processus.** On ne complète pas celui
///    d'Ash, on le remplace : une variable héritée changerait ce que la commande répond,
///    et le test 4 mesurerait alors autre chose que ce qu'il annonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// Le chemin absolu résolu par le `PATH`.
    pub program: PathBuf,
    /// Les arguments déclarés par l'adaptateur, dans l'ordre.
    pub args: Vec<String>,
    /// Tout l'environnement, et rien d'autre.
    pub env: Vec<(String, String)>,
    /// Au-delà, on renonce et on tue le processus.
    pub timeout: Duration,
}

impl Launch {
    /// La commande telle qu'on la montre pendant l'attente (maquette §3.5, état
    /// `verifying`). Ce n'est **pas** une ligne de shell : rien ne la relit, elle se lit.
    pub fn shown(&self) -> String {
        let mut shown = String::new();
        for (name, value) in &self.env {
            shown.push_str(name);
            shown.push('=');
            shown.push_str(value);
            shown.push(' ');
        }
        shown.push_str(&self.program.display().to_string());
        for arg in &self.args {
            shown.push(' ');
            shown.push_str(arg);
        }
        shown
    }
}

/// Ce que la commande a répondu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// A-t-elle rendu la main sans erreur ?
    pub succeeded: bool,
    /// Sa sortie standard, **tronquée**. Personne n'en déduit d'état ; elle sert à dire à
    /// l'utilisateur ce que la commande a répondu quand elle a mal répondu.
    pub output: String,
}

/// Lancer un programme — le seul endroit de `settings` qui fait exister un processus.
///
/// **Ce n'est pas une lecture de la sortie du PTY**, et la distinction n'est pas de forme :
/// [ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md) écarte l'idée de *déduire un
/// état d'agent* de ce qu'un outil affiche, parce qu'un faux `waiting` détruirait la
/// confiance dans la seule notification qui compte. Ici rien n'est déduit et aucun état
/// d'agent n'est produit : l'utilisateur demande explicitement « est-ce bien cet outil, à
/// cet endroit ? », Ash lance une invocation inoffensive choisie par l'adaptateur, et
/// rapporte si elle a répondu. C'est ponctuel, déclenché, et sans conséquence sur la
/// sidebar.
pub trait CommandRunner: Send + Sync {
    /// Le chemin absolu de `command` dans le `PATH`, `None` si elle n'y est pas (test 3).
    fn locate(&self, command: &str) -> Option<PathBuf>;

    /// Lance ce que [`Launch`] décrit, ou dit pourquoi rien n'a répondu (test 4).
    fn run(&self, launch: &Launch) -> Result<Answer, String>;
}

/// Poser le bloc de hooks, le retirer, et savoir où il en est — **sans connaître un seul
/// adaptateur**.
///
/// C'est le troisième effet système de la feature, et le seul qui **écrive chez
/// l'utilisateur**. Il est derrière un trait pour la raison habituelle — aucun test de
/// `settings` ne doit toucher au `~/.claude` de qui lance `cargo test` — mais aussi pour une
/// raison propre à ce port : `settings` n'a pas le droit de connaître `ClaudeCodeAdapter`
/// ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)), et l'instrumentation
/// d'un outil est justement ce que son adaptateur seul sait décrire. La traduction
/// « identifiant d'adaptateur + dossier → fichier et bloc » se fait donc **dans la
/// composition root**, qui est le seul endroit à connaître les deux côtés.
///
/// Le dossier passé est **déjà résolu** : `~` est étendu, le défaut de l'adaptateur est
/// appliqué. Le port n'a aucune convention de chemin à connaître.
pub trait HookBlocks: Send + Sync {
    /// Où en est le bloc, sans rien écrire. `None` quand cet adaptateur n'instrumente rien.
    fn inspect(&self, adapter: &str, config_dir: &Path) -> Option<BlockAt>;

    /// Pose ou met à jour le bloc. Rend la raison du refus, telle qu'on la montre.
    fn install(&self, adapter: &str, config_dir: &Path) -> Result<(), String>;

    /// Retire le bloc et ses marqueurs.
    fn remove(&self, adapter: &str, config_dir: &Path) -> Result<(), String>;
}

/// Résout le `~` de tête d'un chemin de configuration.
///
/// Pure, et donc éprouvée : `~` n'est pas un dossier mais une convention de shell, et la
/// spec §9 comme la maquette écrivent les chemins avec. Seul le `~` **de tête** est résolu
/// — `~utilisateur` est une autre convention, que personne n'écrit dans ce fichier, et un
/// `~` au milieu d'un chemin est un vrai caractère de nom de fichier.
pub fn expand_home(raw: &str, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return PathBuf::from(raw);
    };
    if raw == "~" {
        return home.to_path_buf();
    }
    match raw.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_configuration_path_written_with_a_tilde_when_it_is_resolved_then_it_lands_in_the_home_folder(
    ) {
        // Given — la spec §9 et la maquette écrivent `~/.claude` ; `~` n'est pas un dossier
        let raw = "~/.claude-perso";

        // When
        let resolved = expand_home(raw, Some(Path::new("/Users/ash")));

        // Then
        assert_eq!(resolved, PathBuf::from("/Users/ash/.claude-perso"));
    }

    #[test]
    fn given_a_path_whose_tilde_is_not_the_first_character_when_it_is_resolved_then_it_stays_a_real_character(
    ) {
        // Given — `~` au milieu d'un nom est un caractère de fichier ordinaire ; le
        // remplacer inventerait un chemin que l'utilisateur n'a pas écrit
        let raw = "/dev/notes~/config";

        // When
        let resolved = expand_home(raw, Some(Path::new("/Users/ash")));

        // Then
        assert_eq!(resolved, PathBuf::from("/dev/notes~/config"));
    }

    #[test]
    fn given_a_launch_when_it_is_shown_to_the_user_then_it_names_the_folder_it_imposes() {
        // Given — la maquette montre la commande réellement lancée pendant l'attente : ce
        // qui est lancé sans qu'on l'ait tapé doit être lisible
        let launch = Launch {
            program: PathBuf::from("/usr/local/bin/claude-perso"),
            args: vec!["--version".to_owned()],
            env: vec![(
                "CLAUDE_CONFIG_DIR".to_owned(),
                "/Users/ash/.claude-perso".to_owned(),
            )],
            timeout: Duration::from_secs(5),
        };

        // When
        let shown = launch.shown();

        // Then
        assert_eq!(
            shown,
            "CLAUDE_CONFIG_DIR=/Users/ash/.claude-perso /usr/local/bin/claude-perso --version"
        );
    }
}
