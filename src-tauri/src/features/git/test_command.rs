//! La commande de test d'un worktree — **quand une preuve la nomme**, et sinon rien.
//!
//! Le prompt de conflit de la spec §7.4 porte trois choses, et celle-ci est la seule qui
//! ne se lise pas dans `.git`. Elle ne se devine pas non plus : un prompt qui nomme la
//! mauvaise commande fait lancer la mauvaise commande à un agent qui a les droits
//! d'écriture, et coûte plus cher qu'un prompt qui n'en nomme aucune. La règle est donc
//! **preuve ou silence**, jamais convention :
//!
//! | Preuve, à la racine du worktree | Commande |
//! |---|---|
//! | `package.json` avec un vrai script `test` | celle du gestionnaire que le verrou désigne |
//! | `Cargo.toml` | `cargo test` |
//! | `Makefile` avec une cible `test` | `make test` |
//!
//! Ce qui n'est **pas** une preuve, et ne produit donc rien : un `package.json` sans
//! script `test` (le champ manquant est une réponse), un dossier `tests/`, un
//! `pyproject.toml` sans configuration de test, un `Cargo.toml` un cran plus bas — la
//! recherche s'arrête à la racine du worktree, parce que descendre ferait choisir entre
//! des candidats qu'on ne sait pas départager.
//!
//! Rien n'est exécuté ici, et rien ne le sera : la commande est un **texte** qui part dans
//! un prompt que l'utilisateur lit avant de l'envoyer
//! ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).

use std::path::Path;

use super::ports::FileSystem;

/// Les verrous qu'on sait lire, et le gestionnaire que chacun désigne.
///
/// L'ordre compte : `bun.lock` et `bun.lockb` cohabitent avec un `package-lock.json`
/// oublié dans plus d'un dépôt, et c'est le plus spécifique qui doit gagner.
const LOCKFILES: [(&str, &str); 5] = [
    ("bun.lock", "bun"),
    ("bun.lockb", "bun"),
    ("pnpm-lock.yaml", "pnpm"),
    ("yarn.lock", "yarn"),
    ("package-lock.json", "npm"),
];

/// La commande de test du worktree, ou `None` si rien ne la nomme.
pub fn detect_test_command(fs: &dyn FileSystem, worktree_root: &Path) -> Option<String> {
    node_test_command(fs, worktree_root)
        .or_else(|| cargo_test_command(fs, worktree_root))
        .or_else(|| make_test_command(fs, worktree_root))
}

/// `bun test`, `pnpm test`… — seulement si le `package.json` déclare vraiment un `test`.
///
/// Le script est lu avec un vrai analyseur JSON : chercher `"test"` dans le texte
/// trouverait aussi une dépendance nommée `test`, et un `npm test` sur un dépôt qui n'en a
/// pas rendrait « missing script: test » à l'agent.
fn node_test_command(fs: &dyn FileSystem, root: &Path) -> Option<String> {
    let manifest = fs.read_to_string(&root.join("package.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let script = manifest.get("scripts")?.get("test")?.as_str()?;
    if script.trim().is_empty() {
        return None;
    }

    let manager = LOCKFILES
        .iter()
        .find(|(lockfile, _)| fs.entry(&root.join(lockfile)).is_some())
        .map(|(_, manager)| *manager)
        // Sans verrou, rien ne désigne un gestionnaire, et `npm` est le seul que la
        // présence d'un `package.json` implique.
        .unwrap_or("npm");

    Some(format!("{manager} test"))
}

fn cargo_test_command(fs: &dyn FileSystem, root: &Path) -> Option<String> {
    fs.entry(&root.join("Cargo.toml"))
        .map(|_| "cargo test".to_owned())
}

/// `make test`, seulement si le `Makefile` porte la cible.
///
/// Une cible est une ligne qui **commence** par son nom : `test:` indenté est une
/// commande à l'intérieur d'une autre règle, pas une cible.
fn make_test_command(fs: &dyn FileSystem, root: &Path) -> Option<String> {
    let makefile = fs.read_to_string(&root.join("Makefile")).ok()?;
    makefile
        .lines()
        .any(|line| line.starts_with("test:") || line.starts_with("test :"))
        .then(|| "make test".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::fake_fs::FakeFs;

    fn detect(fs: &FakeFs) -> Option<String> {
        detect_test_command(fs, Path::new("/dev/ash"))
    }

    #[test]
    fn given_a_worktree_whose_manifest_declares_a_test_script_when_looking_for_a_test_command_then_it_names_the_manager_the_lockfile_designates(
    ) {
        // Given — le dépôt d'Ash lui-même : `bun` et rien d'autre
        let fs = FakeFs::new()
            .file(
                "/dev/ash/package.json",
                r#"{"scripts": {"test": "bun test", "lint": "eslint ."}}"#,
            )
            .file("/dev/ash/bun.lock", "");

        // When / Then — `npm test` ici lancerait le mauvais outil sur le bon dépôt
        assert_eq!(detect(&fs), Some("bun test".to_owned()));
    }

    #[test]
    fn given_a_manifest_without_a_test_script_when_looking_for_a_test_command_then_none_is_invented(
    ) {
        // Given — un `package.json` n'implique pas une suite de tests. Un `npm test` sur
        // ce dépôt-là rend « missing script: test » à l'agent, qui redemande.
        let fs = FakeFs::new()
            .file(
                "/dev/ash/package.json",
                r#"{"scripts": {"build": "vite build"}}"#,
            )
            .file("/dev/ash/package-lock.json", "");

        // When / Then
        assert_eq!(detect(&fs), None);
    }

    #[test]
    fn given_a_dependency_named_test_when_looking_for_a_test_command_then_it_is_not_mistaken_for_a_script(
    ) {
        // Given — chercher `"test"` dans le texte du manifeste trouverait ceci
        let fs = FakeFs::new().file(
            "/dev/ash/package.json",
            r#"{"devDependencies": {"test": "^1.0.0"}}"#,
        );

        // When / Then
        assert_eq!(detect(&fs), None);
    }

    #[test]
    fn given_a_rust_crate_at_the_root_when_looking_for_a_test_command_then_it_is_cargo_test() {
        // Given
        let fs = FakeFs::new().file("/dev/ash/Cargo.toml", "[package]\nname = \"ash\"\n");

        // When / Then
        assert_eq!(detect(&fs), Some("cargo test".to_owned()));
    }

    #[test]
    fn given_a_makefile_that_only_mentions_test_inside_a_recipe_when_looking_then_it_is_not_a_target(
    ) {
        // Given — une ligne indentée est une commande d'une autre règle, pas une cible :
        // `make test` y répondrait « No rule to make target »
        let fs = FakeFs::new().file("/dev/ash/Makefile", "all:\n\ttest -d build\n");

        // When / Then
        assert_eq!(detect(&fs), None);
    }

    #[test]
    fn given_a_makefile_with_a_test_target_when_looking_for_a_test_command_then_it_is_make_test() {
        // Given
        let fs = FakeFs::new().file("/dev/ash/Makefile", "all:\n\techo hi\n\ntest:\n\tpytest\n");

        // When / Then
        assert_eq!(detect(&fs), Some("make test".to_owned()));
    }

    #[test]
    fn given_a_worktree_that_names_no_test_command_anywhere_when_looking_then_ash_says_nothing() {
        // Given — un dépôt de notes, un dépôt de configuration. Le prompt sera alors muet
        // sur les tests, ce qui est la vérité (spec §7.4).
        let fs = FakeFs::new().file("/dev/ash/README.md", "# notes\n");

        // When / Then
        assert_eq!(detect(&fs), None);
    }
}
