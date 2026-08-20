//! Où vit la fiche de ce worktree — dans le dépôt, ou à côté.
//!
//! ADR-0013 tranche le cas nominal : `.ash/worktree.md`, **versionné**, committé avec la
//! branche. C'est tout son intérêt — elle voyage, et un agent qui reprend la branche la
//! lit. Et l'ADR nomme aussitôt la limite : « `.ash/` peut être gitignoré, et l'équipe peut
//! ne pas vouloir de ce fichier. Dans ce cas la fiche vit dans `~/.ash/worktrees/` et perd
//! son unique avantage. **Ash ne doit ni forcer, ni imposer un `.gitignore`.** »
//!
//! D'où trois règles, dans cet ordre :
//!
//! 1. **le choix explicite gagne** — l'interrupteur de l'écran, gardé dans
//!    `~/.ash/cards.json` ([`super::modes`]) ;
//! 2. sinon, **`.ash` ignoré ⇒ mode local**. L'équipe a déjà dit ce qu'elle voulait, dans
//!    son `.gitignore` ; Ash le **lit**, et n'y écrit jamais ;
//! 3. sinon, **le fichier qui existe gagne** : une fiche déjà posée en local n'est pas
//!    déplacée dans le dépôt au premier redémarrage.
//!
//! Le défaut, quand rien de tout cela ne s'applique, est le mode du dépôt. C'est la lettre
//! de l'ADR, et la seule direction qui n'efface rien : passer du dépôt au local ne perd
//! qu'un emplacement, l'inverse aurait posé un fichier dans le dépôt de quelqu'un sans qu'il
//! l'ait demandé.
//!
//! **Ce qui n'est pas consulté**, et qu'il vaut mieux savoir : `.git/info/exclude`. Le lire
//! demanderait le dossier git **commun**, donc une résolution de worktree, donc un second
//! port pour un fichier que presque personne n'utilise ; l'interrupteur de la règle 1 couvre
//! le cas, et il le couvre en le disant.

use std::path::{Path, PathBuf};

use super::ports::CardFiles;

/// Les deux emplacements possibles d'une fiche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum CardMode {
    /// `.ash/worktree.md`, versionné — le cas nominal d'ADR-0013.
    Repo,
    /// `~/.ash/worktrees/…`, quand l'équipe ne veut pas du fichier dans le dépôt.
    Local,
}

/// Où est la fiche, et pourquoi là.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub mode: CardMode,
    pub path: PathBuf,
    /// Ce que le mode inverse donnerait — l'écran le montre pour que l'interrupteur dise
    /// où le fichier irait, et non seulement qu'il changerait quelque chose.
    pub other: PathBuf,
    /// Vrai quand c'est le `.gitignore` du dépôt qui a décidé, et non l'utilisateur.
    pub ignored_by_the_repo: bool,
}

/// Le dossier de la fiche dans un worktree, tel qu'ADR-0013 le nomme.
const IN_REPO: [&str; 2] = [".ash", "worktree.md"];

/// Décide de l'emplacement. **Ne lit que des fichiers, n'en écrit aucun.**
pub fn locate(
    files: &dyn CardFiles,
    worktree_root: &Path,
    home: &Path,
    chosen: Option<CardMode>,
) -> Place {
    let in_repo = worktree_root.join(IN_REPO[0]).join(IN_REPO[1]);
    let local = home
        .join(".ash")
        .join("worktrees")
        .join(file_name(worktree_root));
    let ignored = ash_is_ignored(files, worktree_root);

    let mode = match chosen {
        Some(mode) => mode,
        None if ignored => CardMode::Local,
        None if files.exists(&local) && !files.exists(&in_repo) => CardMode::Local,
        None => CardMode::Repo,
    };

    let (path, other) = match mode {
        CardMode::Repo => (in_repo, local),
        CardMode::Local => (local, in_repo),
    };
    Place {
        mode,
        path,
        other,
        ignored_by_the_repo: ignored,
    }
}

/// Le dépôt a-t-il dit qu'il ne voulait pas de `.ash/` ?
///
/// La lecture est celle d'un `.gitignore` de racine, et rien de plus : les motifs qui
/// désignent `.ash` à la racine du worktree. Un `.gitignore` de sous-dossier ne concerne pas
/// la fiche, et un motif exotique qui attraperait `.ash` par la bande ne sera pas vu — le
/// prix est une fiche proposée dans le dépôt alors qu'elle serait ignorée, ce que
/// l'interrupteur corrige en un geste. **L'inverse — écrire dans le `.gitignore` — n'est
/// jamais fait, nulle part dans cette feature.**
fn ash_is_ignored(files: &dyn CardFiles, worktree_root: &Path) -> bool {
    let Ok(Some(content)) = files.read(&worktree_root.join(".gitignore")) else {
        return false;
    };
    let mut ignored = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (negated, pattern) = match line.strip_prefix('!') {
            Some(rest) => (true, rest.trim()),
            None => (false, line),
        };
        if names_ash(pattern) {
            ignored = !negated;
        }
    }
    ignored
}

/// Ce motif désigne-t-il le dossier `.ash` de la racine ?
fn names_ash(pattern: &str) -> bool {
    let trimmed = pattern
        .trim_end_matches('/')
        .trim_end_matches("/*")
        .trim_end_matches("/**")
        .trim_start_matches('/')
        .trim_start_matches("**/");
    trimmed == ".ash"
}

/// Le nom du fichier local d'un worktree, sous `~/.ash/worktrees/`.
///
/// Deux exigences qui se contredisent, conciliées comme le journal les concilie
/// (`journal/entry.rs`) : **lisible**, parce que la spec §10 promet un dossier qu'on inspecte
/// à l'œil nu, et **unique**, parce que deux worktrees homonymes existent sur une machine —
/// `ash-sidebar` peut être le nom d'un worktree dans deux dépôts.
///
/// La fonction est écrite ici plutôt que partagée avec le journal : sa clé n'est pas la même
/// — un worktree, pas un dépôt commun — et quinze lignes d'empreinte non cryptographique ne
/// valent pas un module transverse qui porterait la règle de nommage de deux features.
fn file_name(worktree_root: &Path) -> String {
    let full = worktree_root.to_string_lossy();
    format!("{}-{:016x}.md", readable(&full), fingerprint(&full))
}

fn readable(path: &str) -> String {
    let name: String = path
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("worktree")
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() || letter == '-' || letter == '_' {
                letter.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    if name.is_empty() {
        "worktree".to_owned()
    } else {
        name
    }
}

/// FNV-1a 64 bits : elle sépare des chemins, elle ne protège rien.
fn fingerprint(path: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::card::fakes::MemoryCardFiles;

    const HOME: &str = "/Users/moi";
    const WORKTREE: &str = "/dev/ash";

    #[test]
    fn given_a_repo_that_says_nothing_when_the_card_is_placed_then_it_goes_into_the_repository() {
        // Given — le cas nominal d'ADR-0013 : la fiche voyage avec la branche.
        let files = MemoryCardFiles::new();

        // When
        let place = locate(&files, Path::new(WORKTREE), Path::new(HOME), None);

        // Then
        assert_eq!(place.mode, CardMode::Repo);
        assert_eq!(place.path, Path::new("/dev/ash/.ash/worktree.md"));
    }

    #[test]
    fn given_a_gitignore_that_excludes_ash_when_the_card_is_placed_then_it_falls_back_to_the_home()
    {
        // Given — « l'équipe peut ne pas vouloir de ce fichier », et elle l'a déjà écrit.
        // Ash lit cette phrase ; il ne l'écrit jamais.
        let files = MemoryCardFiles::new().file("/dev/ash/.gitignore", "target/\n.ash/\n");

        // When
        let place = locate(&files, Path::new(WORKTREE), Path::new(HOME), None);

        // Then
        assert_eq!(place.mode, CardMode::Local);
        assert!(place.ignored_by_the_repo);
        assert!(
            place.path.starts_with("/Users/moi/.ash/worktrees"),
            "{:?}",
            place.path
        );
        // …et le `.gitignore` est ressorti tel quel : la lecture n'a rien écrit.
        assert_eq!(
            files.contents("/dev/ash/.gitignore"),
            Some("target/\n.ash/\n".to_owned())
        );
    }

    #[test]
    fn given_a_gitignore_that_excludes_ash_then_takes_it_back_when_the_card_is_placed_then_it_goes_into_the_repository(
    ) {
        // Given — la forme courante d'un `.gitignore` d'équipe : une exclusion large, puis
        // une exception. Lire la première et s'arrêter donnerait le mauvais mode.
        let files = MemoryCardFiles::new().file("/dev/ash/.gitignore", ".ash/\n!.ash\n");

        // When
        let place = locate(&files, Path::new(WORKTREE), Path::new(HOME), None);

        // Then
        assert_eq!(place.mode, CardMode::Repo);
    }

    #[test]
    fn given_a_card_already_written_next_to_the_repository_when_it_is_placed_then_it_is_not_moved()
    {
        // Given — Ash a écrit en local hier ; le `.gitignore` a changé depuis. Repasser au
        // dépôt sans rien dire perdrait de vue la fiche existante.
        let local = format!("{HOME}/.ash/worktrees/{}", file_name(Path::new(WORKTREE)));
        let files = MemoryCardFiles::new().file(&local, "# pourquoi\n");

        // When
        let place = locate(&files, Path::new(WORKTREE), Path::new(HOME), None);

        // Then
        assert_eq!(place.mode, CardMode::Local);
        assert_eq!(place.path, Path::new(&local));
    }

    #[test]
    fn given_a_user_who_chose_the_repository_when_ash_is_gitignored_then_the_choice_wins() {
        // Given — « Ash ne doit ni forcer, ni imposer » : la détection est un défaut, pas
        // une décision. Quelqu'un peut vouloir sa fiche versionnée malgré le `.gitignore`.
        let files = MemoryCardFiles::new().file("/dev/ash/.gitignore", ".ash\n");

        // When
        let place = locate(
            &files,
            Path::new(WORKTREE),
            Path::new(HOME),
            Some(CardMode::Repo),
        );

        // Then
        assert_eq!(place.mode, CardMode::Repo);
        assert_eq!(place.path, Path::new("/dev/ash/.ash/worktree.md"));
        // …et la raison du défaut reste dite : l'écran peut prévenir que le fichier sera ignoré.
        assert!(place.ignored_by_the_repo);
    }

    #[test]
    fn given_two_worktrees_of_the_same_name_when_their_local_cards_are_named_then_they_do_not_collide(
    ) {
        // Given — `ash-sidebar` existe dans deux dépôts. Un nom sans empreinte ferait lire à
        // l'un la fiche de l'autre.
        // When
        let one = file_name(Path::new("/wt/a/ash-sidebar"));
        let other = file_name(Path::new("/wt/b/ash-sidebar"));

        // Then
        assert_ne!(one, other);
        assert!(one.starts_with("ash-sidebar-"), "{one}");
    }
}
