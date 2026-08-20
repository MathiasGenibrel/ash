//! La lecture de `git status --porcelain=v2 --branch`.
//!
//! Une **règle pure** : une chaîne entre, des comptes sortent. Elle est séparée de
//! l'invocation ([`super::git_cli`]) pour que le format de git — le seul endroit où Ash
//! peut mentir sur un sujet que l'utilisateur vérifie d'un coup d'œil — soit vérifiable
//! sur des charges utiles fixes, sans lancer un seul processus.
//!
//! Le format `v2` est **stable et documenté** par git, contrairement à `--short`, qui est
//! prévu pour l'humain. Un seul appel donne les deux moitiés de la ligne de statut :
//! l'en-tête `# branch.ab +2 -1` porte l'avance et le retard sur l'amont, les lignes qui
//! suivent portent l'état de l'arbre.

use super::metadata::{Status, TreeStatus, Upstream};

/// Combien de chemins en conflit on retient au plus.
///
/// Le **compte** (`TreeStatus::conflicted`) n'est pas borné, lui : c'est la liste des
/// chemins qui l'est. Un rebase qui se plante sur un dossier `vendor/` entier peut en
/// aligner des milliers, et cette liste voyage jusqu'à la webview, puis — pour une partie
/// d'elle — jusque dans un prompt d'une seule ligne. Cent chemins tiennent déjà bien
/// au-delà de ce qu'un humain lit ; le compte, lui, reste juste.
const MAX_CONFLICT_PATHS: usize = 100;

/// Lit la sortie de `git status --porcelain=v2 --branch`.
///
/// Ne peut pas échouer : une ligne qu'on ne reconnaît pas est ignorée. Une sortie vide est
/// un arbre propre, ce qui est la vérité pour un dépôt fraîchement cloné, et une sortie
/// d'une version future de git rendra des comptes partiels plutôt que rien.
pub fn parse_status(output: &str) -> Status {
    let mut tree = TreeStatus::default();
    let mut upstream = None;
    let mut conflicts = Vec::new();

    for line in output.lines() {
        match line.split_once(' ') {
            Some(("#", header)) => {
                if let Some(counts) = header.strip_prefix("branch.ab ") {
                    upstream = parse_ahead_behind(counts);
                }
            }
            // Les fichiers non suivis comptent pour des **ajouts**. Un agent qui écrit du
            // code crée des fichiers avant de les ajouter à l'index : ne pas les compter
            // afficherait un arbre propre au moment précis où il ne l'est plus.
            //
            // Git rend un dossier entièrement nouveau comme **une** entrée (`? dir/`), et
            // c'est ce qu'on compte : le faire parcourir (`-uall`) coûterait le prix d'un
            // dossier de build oublié dans un `.gitignore`, à chaque rafraîchissement.
            Some(("?", _)) => tree.added += 1,
            // Un chemin en conflit n'est ni ajouté ni modifié : il attend une décision, et
            // c'est un état à part entière pendant un merge ou un rebase.
            Some(("u", rest)) => {
                tree.conflicted += 1;
                if conflicts.len() < MAX_CONFLICT_PATHS {
                    if let Some(path) = unmerged_path(rest) {
                        conflicts.push(path);
                    }
                }
            }
            // `1` = entrée ordinaire, `2` = renommée ou copiée. Les deux portent leur
            // état sur deux caractères : l'index d'abord, l'arbre de travail ensuite.
            Some(("1", rest)) | Some(("2", rest)) => count_change(&mut tree, rest),
            // `!` (ignoré) n'apparaît qu'avec `--ignored`, et ne dit rien du travail.
            _ => {}
        }
    }

    Status {
        tree,
        upstream,
        conflicts,
    }
}

/// Le chemin d'une ligne `u`, laissé **exactement** tel que git l'a écrit.
///
/// Le format est fixe : `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>` — neuf
/// champs, puis le chemin, qui est le reste de la ligne. Un chemin peut contenir des
/// espaces ; il ne peut donc pas être découpé, seulement sauté jusqu'à lui.
///
/// Le chemin n'est **pas** dé-échappé. L'invocation pose `core.quotePath=true`
/// ([`super::git_cli`]) : un chemin exotique arrive entre guillemets, avec ses octets en
/// séquences `\303\251`. C'est la forme que git affiche partout ailleurs, celle qu'un
/// agent relira sans se tromper, et surtout la seule qui garantisse qu'aucun octet de
/// contrôle ne traverse — un chemin est une donnée du dépôt visité, et ce qui en est fait
/// finit dans un terminal.
fn unmerged_path(rest: &str) -> Option<String> {
    let path = rest.splitn(10, ' ').nth(9)?.trim_end_matches(['\r', '\n']);
    (!path.is_empty()).then(|| path.to_owned())
}

/// Range une entrée changée dans l'un des trois comptes.
///
/// La convention est celle des invites de shell (posh-git), que la maquette reprend : ce
/// sont des **nombres de fichiers**, jamais des lignes. Un fichier modifié dans l'index
/// **et** dans l'arbre de travail est un seul fichier, donc un seul compte.
fn count_change(tree: &mut TreeStatus, rest: &str) {
    let mut states = rest.chars();
    let (Some(staged), Some(worktree)) = (states.next(), states.next()) else {
        return;
    };

    if staged == 'D' || worktree == 'D' {
        // La disparition l'emporte : un fichier modifié puis supprimé est supprimé.
        tree.deleted += 1;
    } else if staged == 'A' {
        tree.added += 1;
    } else {
        // `M`, `T` (changement de type), `R` et `C` (renommage, copie). Un renommage est
        // compté comme une modification : le fichier existait déjà, il n'y en a pas un de
        // plus dans l'arbre.
        tree.modified += 1;
    }
}

/// `+2 -1` — l'avance et le retard sur la branche amont.
fn parse_ahead_behind(counts: &str) -> Option<Upstream> {
    let (ahead, behind) = counts.trim().split_once(' ')?;
    Some(Upstream {
        ahead: ahead.strip_prefix('+')?.parse().ok()?,
        behind: behind.strip_prefix('-')?.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : une sortie de `git status --porcelain=v2 --branch`.
    ///
    /// Les lignes sont recopiées de vraies sorties de git — c'est la seule façon d'être
    /// sûr des colonnes. Défaut valide et déterministe : un dépôt sur `main`, sans amont
    /// et sans rien de modifié.
    struct PorcelainBuilder {
        headers: Vec<String>,
        entries: Vec<String>,
    }

    impl PorcelainBuilder {
        fn new() -> Self {
            Self {
                headers: vec![
                    "# branch.oid 200f7b936e7e7d0e8b85366bd9c0f569095b9525".to_owned(),
                    "# branch.head main".to_owned(),
                ],
                entries: Vec::new(),
            }
        }

        /// Le dépôt suit une branche amont, avec cette avance et ce retard.
        fn tracking(mut self, ahead: u32, behind: u32) -> Self {
            self.headers
                .push("# branch.upstream origin/main".to_owned());
            self.headers.push(format!("# branch.ab +{ahead} -{behind}"));
            self
        }

        fn entry(mut self, line: &str) -> Self {
            self.entries.push(line.to_owned());
            self
        }

        fn parse(&self) -> Status {
            let lines: Vec<&str> = self
                .headers
                .iter()
                .chain(self.entries.iter())
                .map(String::as_str)
                .collect();
            parse_status(&lines.join("\n"))
        }
    }

    /// Les lignes d'entrée telles que git les écrit, une par cas.
    mod lines {
        pub const STAGED_ADD: &str = "1 A. N... 000000 100644 100644 0000000000000000000000000000000000000000 3e757656cf36eca53338e520d134963a44f793f8 added.txt";
        pub const WORKTREE_MODIFIED: &str = "1 .M N... 100644 100644 100644 4bcfe98e640c8284511312660fb8709b0afa888e 4bcfe98e640c8284511312660fb8709b0afa888e mod.txt";
        pub const STAGED_AND_WORKTREE_MODIFIED: &str = "1 MM N... 100644 100644 100644 4bcfe98e640c8284511312660fb8709b0afa888e 4bcfe98e640c8284511312660fb8709b0afa888e both.txt";
        pub const WORKTREE_DELETED: &str = "1 .D N... 100644 100644 000000 f2ad6c76f0115a6ba5b00456a849810e7ec0af20 f2ad6c76f0115a6ba5b00456a849810e7ec0af20 gone.txt";
        pub const STAGED_MODIFIED_THEN_DELETED: &str = "1 MD N... 100644 100644 000000 f2ad6c76f0115a6ba5b00456a849810e7ec0af20 f2ad6c76f0115a6ba5b00456a849810e7ec0af20 vanished.txt";
        pub const RENAMED: &str = "2 R. N... 100644 100644 100644 61780798228d17af2d34fce4cfbdf35556832472 61780798228d17af2d34fce4cfbdf35556832472 R100 renamed2.txt\trenamed.txt";
        pub const UNMERGED: &str =
            "u UU N... 100644 100644 100644 100644 aaaa bbbb cccc conflict.txt";
        pub const UNMERGED_WITH_SPACES: &str =
            "u UU N... 100644 100644 100644 100644 aaaa bbbb cccc docs/my notes.md";
        pub const UNMERGED_QUOTED: &str =
            r#"u UU N... 100644 100644 100644 100644 aaaa bbbb cccc "src/caf\303\251.rs""#;
        pub const UNTRACKED_FILE: &str = "? untracked.txt";
        pub const UNTRACKED_DIRECTORY: &str = "? untracked_dir/";
    }

    #[test]
    fn given_a_worktree_with_a_new_a_modified_and_a_deleted_file_when_reading_the_status_then_each_is_counted_once(
    ) {
        // Given — ce que la maquette écrit `+3 ~1` : des **fichiers**, pas des lignes
        let porcelain = PorcelainBuilder::new()
            .entry(lines::STAGED_ADD)
            .entry(lines::WORKTREE_MODIFIED)
            .entry(lines::WORKTREE_DELETED);

        // When
        let status = porcelain.parse();

        // Then
        assert_eq!(status.tree.added, 1);
        assert_eq!(status.tree.modified, 1);
        assert_eq!(status.tree.deleted, 1);
        assert_eq!(status.tree.conflicted, 0);
    }

    #[test]
    fn given_a_file_modified_both_in_the_index_and_in_the_worktree_when_reading_then_it_counts_once(
    ) {
        // Given — git en fait **une** ligne `MM` ; la compter deux fois afficherait `~2`
        // pour un seul fichier ouvert dans l'éditeur
        let porcelain = PorcelainBuilder::new().entry(lines::STAGED_AND_WORKTREE_MODIFIED);

        // When
        let status = porcelain.parse();

        // Then
        assert_eq!(status.tree.modified, 1);
        assert_eq!(status.tree.added, 0);
    }

    #[test]
    fn given_a_file_modified_then_deleted_when_reading_then_it_is_counted_as_deleted() {
        // Given — `MD` : modifié dans l'index, disparu de l'arbre. Il n'est plus là.
        let porcelain = PorcelainBuilder::new().entry(lines::STAGED_MODIFIED_THEN_DELETED);

        // When
        let status = porcelain.parse();

        // Then
        assert_eq!(status.tree.deleted, 1);
        assert_eq!(status.tree.modified, 0);
    }

    #[test]
    fn given_a_renamed_file_when_reading_the_status_then_it_counts_as_modified_not_as_added() {
        // Given — une ligne `2`, avec deux chemins séparés par une tabulation. Le fichier
        // existait déjà : le compter en `+` ferait croire à une création.
        let porcelain = PorcelainBuilder::new().entry(lines::RENAMED);

        // When
        let status = porcelain.parse();

        // Then
        assert_eq!(status.tree.modified, 1);
        assert_eq!(status.tree.added, 0);
    }

    #[test]
    fn given_untracked_files_when_reading_the_status_then_they_count_as_added() {
        // Given — un agent crée des fichiers avant de les ajouter à l'index. Ne pas les
        // compter montrerait un arbre propre pendant qu'il écrit.
        let porcelain = PorcelainBuilder::new()
            .entry(lines::UNTRACKED_FILE)
            .entry(lines::UNTRACKED_DIRECTORY);

        // When
        let status = porcelain.parse();

        // Then — un dossier entièrement nouveau est **une** entrée pour git, et une seule
        // ici : c'est le prix de ne pas le faire parcourir à chaque rafraîchissement
        assert_eq!(status.tree.added, 2);
    }

    #[test]
    fn given_a_merge_conflict_when_reading_the_status_then_the_file_is_neither_added_nor_modified()
    {
        // Given
        let porcelain = PorcelainBuilder::new().entry(lines::UNMERGED);

        // When
        let status = porcelain.parse();

        // Then — un conflit attend une décision ; le ranger dans `~` le rendrait invisible
        assert_eq!(status.tree.conflicted, 1);
        assert_eq!(status.tree.modified, 0);
        assert_eq!(status.tree.added, 0);
    }

    #[test]
    fn given_a_stopped_merge_when_reading_the_status_then_it_names_the_conflicting_paths() {
        // Given — spec §7.4 demande les **chemins**, pas leur nombre : un prompt qui dit
        // « trois fichiers » fait redemander lesquels
        let porcelain = PorcelainBuilder::new()
            .entry(lines::UNMERGED)
            .entry(lines::UNMERGED_WITH_SPACES)
            .entry(lines::WORKTREE_MODIFIED);

        // When
        let status = porcelain.parse();

        // Then — le chemin est le **reste** de la ligne : le découper sur l'espace
        // couperait `my notes.md` en deux
        assert_eq!(
            status.conflicts,
            vec!["conflict.txt".to_owned(), "docs/my notes.md".to_owned()]
        );
        assert_eq!(status.tree.conflicted, 2);
    }

    #[test]
    fn given_a_conflicting_path_that_git_had_to_quote_when_reading_then_it_keeps_git_s_own_escaping(
    ) {
        // Given — `core.quotePath=true` est posé par l'invocation durcie : git rend un
        // chemin exotique entre guillemets, octets échappés. Le dé-échapper ferait entrer
        // des octets d'un dépôt visité dans un texte qui finit dans un terminal.
        let porcelain = PorcelainBuilder::new().entry(lines::UNMERGED_QUOTED);

        // When
        let status = porcelain.parse();

        // Then
        assert_eq!(status.conflicts, vec![r#""src/caf\303\251.rs""#.to_owned()]);
    }

    #[test]
    fn given_a_rebase_that_conflicts_on_a_whole_vendored_tree_when_reading_then_the_list_is_bounded_but_the_count_is_not(
    ) {
        // Given — la liste traverse la frontière Tauri puis, en partie, un prompt d'une
        // seule ligne ; le compte, lui, est ce que la ligne de statut affiche
        let mut porcelain = PorcelainBuilder::new();
        for index in 0..(MAX_CONFLICT_PATHS + 7) {
            porcelain = porcelain.entry(&format!(
                "u UU N... 100644 100644 100644 100644 aaaa bbbb cccc vendor/f{index}.rs"
            ));
        }

        // When
        let status = porcelain.parse();

        // Then
        assert_eq!(status.conflicts.len(), MAX_CONFLICT_PATHS);
        assert_eq!(status.tree.conflicted, MAX_CONFLICT_PATHS as u32 + 7);
    }

    #[test]
    fn given_a_branch_that_tracks_an_upstream_when_reading_the_status_then_it_reports_the_ahead_and_behind_counts(
    ) {
        // Given — le `↑2 ↓1` de la maquette, dans l'en-tête du même appel
        let porcelain = PorcelainBuilder::new().tracking(2, 1);

        // When
        let status = porcelain.parse();

        // Then
        assert_eq!(
            status.upstream,
            Some(Upstream {
                ahead: 2,
                behind: 1
            })
        );
    }

    #[test]
    fn given_a_branch_without_an_upstream_when_reading_the_status_then_there_is_nothing_to_compare()
    {
        // Given — une branche locale toute neuve n'a pas d'amont, et git n'écrit alors
        // aucune ligne `branch.ab` : afficher `↑0 ↓0` serait une comparaison inventée
        let porcelain = PorcelainBuilder::new().entry(lines::WORKTREE_MODIFIED);

        // When
        let status = porcelain.parse();

        // Then
        assert_eq!(status.upstream, None);
        assert_eq!(status.tree.modified, 1);
    }

    #[test]
    fn given_a_repository_without_a_single_commit_when_reading_the_status_then_its_untracked_files_still_count(
    ) {
        // Given — `git init` suivi de quelques fichiers : `branch.oid` vaut `(initial)`,
        // et il n'y a ni amont, ni `HEAD` né
        let status =
            parse_status("# branch.oid (initial)\n# branch.head main\n? u.txt\n? autre.txt\n");

        // Then
        assert_eq!(status.tree.added, 2);
        assert_eq!(status.upstream, None);
    }

    #[test]
    fn given_a_detached_head_when_reading_the_status_then_the_tree_is_still_counted() {
        // Given — pendant un rebase, `HEAD` est détaché : git écrit `(detached)` et pas
        // de `branch.ab`, mais l'arbre de travail, lui, a bien des fichiers en conflit
        let status = parse_status(
            "# branch.oid 5664dfe2463b4cbb71a5e046878859b41062c475\n\
             # branch.head (detached)\n\
             u UU N... 100644 100644 100644 100644 aaaa bbbb cccc conflict.txt\n",
        );

        // Then
        assert_eq!(status.tree.conflicted, 1);
        assert_eq!(status.upstream, None);
    }

    #[test]
    fn given_a_clean_worktree_when_reading_the_status_then_every_count_is_zero() {
        // Given — le cas le plus courant, et celui où la ligne de statut doit rester muette
        let porcelain = PorcelainBuilder::new().tracking(0, 0);

        // When
        let status = porcelain.parse();

        // Then
        assert_eq!(status.tree, TreeStatus::default());
        assert!(status.tree.is_clean());
        assert_eq!(
            status.upstream,
            Some(Upstream {
                ahead: 0,
                behind: 0
            })
        );
    }
}
