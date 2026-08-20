//! Ce qui est réellement transverse, et rien d'autre.
//!
//! Un module n'entre ici que s'il sert **au moins deux features** et ne porte **aucune
//! règle** propre à l'une d'elles (voir `.claude/docs/architecture.md`). Le temps remplit
//! les deux conditions : lire l'heure est un effet système, pas une règle de git ni une
//! règle d'agent.

pub mod text_diff;
pub mod time;

/// Le filet du contrat ne mord que si `cargo` a lu `src-tauri/.cargo/config.toml`.
///
/// `cargo` découvre ce fichier en remontant depuis le **répertoire courant**, jamais depuis
/// le manifeste. `cargo test --manifest-path src-tauri/Cargo.toml` lancé depuis la racine
/// du dépôt — une invocation que `CLAUDE.md` documente — ne le voit donc pas : `ts-rs`
/// retombe sur son dossier par défaut, `src-tauri/bindings/`, et `src/shared/ipc/generated/`
/// garde les types de la veille. `bun run typecheck` compare alors un contrat périmé et se
/// tait, ce qui est exactement le faux négatif que tout ce dispositif existe pour
/// supprimer. Le seul indice était un dossier non suivi de plus dans `git status`.
///
/// Ce test est ici plutôt que dans une feature parce que la destination ne concerne aucune
/// d'elles en particulier : les trente formes exportées viennent de six features.
#[cfg(test)]
mod contract_export {
    use std::path::{Path, PathBuf};

    /// Où `mirror.ts` et ses trois jumeaux lisent les types tirés des `struct`.
    const MIRRORED_BY_THE_FRONTEND: &str = "../src/shared/ipc/generated";

    #[test]
    fn given_the_types_the_frontend_mirrors_when_cargo_exports_them_then_they_land_where_the_mirrors_read(
    ) {
        // Given — la destination telle que `ts-rs` la lira, et celle qu'on attend. La
        // seconde est ancrée sur le manifeste, donc indépendante d'où `cargo` est lancé.
        let configured = std::env::var("TS_RS_EXPORT_DIR").unwrap_or_else(|_| {
            panic!(
                "TS_RS_EXPORT_DIR n'est pas posée : `cargo` n'a pas lu \
                 src-tauri/.cargo/config.toml. Relance depuis src-tauri/ — sinon `ts-rs` \
                 écrit dans src-tauri/bindings/, {MIRRORED_BY_THE_FRONTEND} reste périmé, \
                 et `bun run typecheck` compare un contrat de la veille sans rien dire."
            )
        });
        let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join(MIRRORED_BY_THE_FRONTEND);

        // When
        let destination = PathBuf::from(&configured).canonicalize();

        // Then
        assert_eq!(
            destination.ok(),
            expected.canonicalize().ok(),
            "TS_RS_EXPORT_DIR vaut {configured}, et non le dossier que les `mirror.ts` \
             lisent : les types exportés n'arriveront pas sous les yeux de `tsc`."
        );
    }
}
