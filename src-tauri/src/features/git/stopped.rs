//! Ce qu'un rebase ou un merge **arrêté** dit de lui-même (spec §7.4).
//!
//! [`super::metadata`] lit déjà l'opération en cours — son genre, sa branche, son `onto`,
//! son `2/5`. Ce module ne refait pas cette lecture : il l'**étend** avec les trois choses
//! que la spec demande en plus, et que personne d'autre n'a sous la main au moment où le
//! rebase s'arrête :
//!
//! - les **chemins** en conflit, qui viennent des lignes `u` du `git status` déjà lancé
//!   par la surveillance ([`super::porcelain`]) — donc sans un seul verbe git de plus ;
//! - le **commit d'arrêt** (`rebase-merge/stopped-sha`, `rebase-apply/original-commit`) ;
//! - `ORIG_HEAD`, le filet de secours : où le worktree pointait avant l'opération.
//!
//! **Ash ne touche à rien** : tout est lu derrière le trait [`FileSystem`], rien n'est
//! écrit, et les deux sorties de secours — `abort` et `skip` — sont rendues comme du
//! **texte à afficher**, jamais comme quelque chose qu'Ash exécuterait
//! ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
//!
//! La lecture se fait **à la demande**, pas dans la surveillance : `ORIG_HEAD` et
//! `stopped-sha` ne bougent qu'aux instants où `rebase-merge/` ou `MERGE_HEAD` bougent
//! aussi, et ceux-là sont déjà surveillés ([`super::targets`]). Ajouter `ORIG_HEAD` aux
//! cibles ferait payer une relecture de plus pour une information que personne ne regarde
//! tant que le panneau des conflits est fermé.

use std::path::Path;

use super::control::optional_line;
use super::metadata::{Operation, OperationKind, WorktreeMetadata};
use super::ports::FileSystem;

/// Longueur d'un identifiant de commit abrégé, à la mode de git.
///
/// La même que celle de [`super::metadata`], et pour la même raison : ce qui s'affiche
/// d'un commit dans Ash a une seule forme.
const SHORT_COMMIT: usize = 7;

/// Le commit sur lequel l'opération s'est arrêtée.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StoppedCommit {
    /// L'identifiant abrégé — celui qu'un `git show` accepte tel quel.
    pub commit: String,
    /// La première ligne du message du commit qu'on essayait d'appliquer.
    ///
    /// `None` pour le moteur `apply` et pour `git am` : ils n'écrivent pas de fichier dont
    /// le contenu soit le message du commit **en cours**, et le déduire du patch reviendrait
    /// à écrire un lecteur de patchs pour une ligne d'affichage.
    pub subject: Option<String>,
}

/// Un rebase, un `am` ou un merge qui s'est arrêté et rend la main à l'utilisateur.
///
/// « Arrêté » veut dire : les fichiers de contrôle de l'opération sont là, et aucun
/// processus git ne tient plus le terminal. Le plus souvent c'est un conflit ; ça peut
/// aussi être un `edit` ou un `break` d'un rebase interactif — d'où une liste de conflits
/// qui a le droit d'être **vide** sans que ce soit une anomalie.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StoppedOperation {
    /// L'opération, telle que la ligne de statut l'affiche déjà.
    pub operation: Operation,
    /// Les chemins qui attendent une décision, tels que git les écrit. Liste **bornée**.
    pub conflicts: Vec<String>,
    /// Combien il y en a en tout. `None` quand `git status` n'a pas su répondre — `git`
    /// absent du `PATH`, dépôt trop gros pour le délai.
    ///
    /// Distinct de `conflicts.len()` : la liste est coupée à cent chemins, le compte non.
    /// Les confondre ferait dire « 100 fichiers » là où il y en a 3 000.
    pub conflicted_total: Option<u32>,
    pub stopped_at: Option<StoppedCommit>,
    /// `ORIG_HEAD` abrégé : où ce worktree pointait avant l'opération.
    ///
    /// C'est le filet de secours de la spec §7.4. Ash l'**affiche** ; il ne s'en sert pas.
    pub orig_head: Option<String>,
    /// La commande de test du worktree, quand une preuve la nomme (voir
    /// [`super::test_command`]). `None` est une réponse : un prompt qui nomme la mauvaise
    /// commande est pire qu'un prompt qui n'en nomme aucune.
    pub test_command: Option<String>,
    /// Les deux sorties de l'opération, **à afficher** (spec §7.4 : « `abort` et `skip`
    /// restent visibles avant d'entrer »).
    ///
    /// Ce sont des chaînes, pas des actions. Rien dans cette feature ne les exécute, et
    /// c'est délibéré : `--abort` jette le travail de l'utilisateur, et Ash ne valide rien
    /// à sa place ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
    pub escapes: Vec<String>,
}

/// Lit l'état d'une opération arrêtée, à partir des métadonnées déjà connues.
///
/// `metadata` porte l'opération et — quand `git` a répondu — les chemins en conflit :
/// c'est ce qui évite de relancer un `git status` ici. `git_dir` est le dossier git
/// **propre** au worktree : `stopped-sha` et `ORIG_HEAD` y vivent, et deux worktrees du
/// même dépôt ont chacun les leurs
/// ([ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md)).
///
/// Rend `None` quand rien n'est en cours — c'est le cas courant, et il se rend en
/// n'affichant rien.
pub fn read_stopped(
    fs: &dyn FileSystem,
    git_dir: &Path,
    metadata: &WorktreeMetadata,
    test_command: Option<String>,
) -> Option<StoppedOperation> {
    let operation = metadata.operation.clone()?;
    let status = metadata.status.as_ref();

    Some(StoppedOperation {
        conflicts: status
            .map(|status| status.conflicts.clone())
            .unwrap_or_default(),
        conflicted_total: status.map(|status| status.tree.conflicted),
        stopped_at: read_stopped_commit(fs, git_dir, operation.kind),
        orig_head: optional_line(fs, &git_dir.join("ORIG_HEAD")).map(|line| short_commit(&line)),
        escapes: escapes_of(operation.kind),
        test_command,
        operation,
    })
}

/// Le commit d'arrêt, là où le moteur qui tourne l'a écrit.
///
/// Les deux moteurs de rebase ne le nomment pas pareil, et un merge n'en a pas du tout :
/// il s'arrête *entre* deux histoires, pas *sur* un commit. Inventer un commit d'arrêt pour
/// un merge en désignant `MERGE_HEAD` dirait quelque chose de faux — c'est le commit
/// fusionné, que l'opération porte déjà sous `onto`.
fn read_stopped_commit(
    fs: &dyn FileSystem,
    git_dir: &Path,
    kind: OperationKind,
) -> Option<StoppedCommit> {
    match kind {
        OperationKind::Merge => None,
        _ => {
            let merge = git_dir.join("rebase-merge");
            // `stopped-sha` est déjà abrégé par git ; `original-commit` ne l'est pas.
            // Les deux passent par le même raccourcissement pour n'avoir qu'une forme.
            if let Some(commit) = optional_line(fs, &merge.join("stopped-sha")) {
                return Some(StoppedCommit {
                    commit: short_commit(&commit),
                    subject: optional_line(fs, &merge.join("message")),
                });
            }
            let commit = optional_line(fs, &git_dir.join("rebase-apply/original-commit"))?;
            Some(StoppedCommit {
                commit: short_commit(&commit),
                subject: None,
            })
        }
    }
}

/// Comment sortir de cette opération — le texte, et rien d'autre.
///
/// Un merge n'a pas de `--skip` : il n'a qu'un pas.
fn escapes_of(kind: OperationKind) -> Vec<String> {
    let verb = match kind {
        OperationKind::Rebase => "git rebase",
        OperationKind::Am => "git am",
        OperationKind::Merge => return vec!["git merge --abort".to_owned()],
    };
    vec![format!("{verb} --abort"), format!("{verb} --skip")]
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(SHORT_COMMIT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::git::fake_fs::FakeFs;
    use crate::features::git::metadata::{read_metadata, Progress, Status, TreeStatus};
    use crate::features::git::porcelain::parse_status;

    const MAIN: &str = "80eca445d2118da8da44686ec2f7047789b5da1f";
    const STOPPED: &str = "1a2b3c4";

    /// Test Data Builder : un worktree dont le rebase s'est arrêté.
    ///
    /// Défauts valides et déterministes — le moteur `merge`, deux commits sur cinq, un
    /// conflit, un `ORIG_HEAD`. Un scénario n'écrase que ce qu'il regarde.
    struct StoppedBuilder {
        fs: FakeFs,
        status: Option<Status>,
        test_command: Option<String>,
    }

    impl StoppedBuilder {
        fn new() -> Self {
            let fs = FakeFs::new()
                .plain_repo("/dev/ash")
                .file("/dev/ash/.git/HEAD", &format!("{MAIN}\n"))
                .file("/dev/ash/.git/refs/heads/main", &format!("{MAIN}\n"))
                .file("/dev/ash/.git/ORIG_HEAD", &format!("{MAIN}\n"))
                .file("/dev/ash/.git/rebase-merge/head-name", "refs/heads/feat\n")
                .file("/dev/ash/.git/rebase-merge/onto", &format!("{MAIN}\n"))
                .file("/dev/ash/.git/rebase-merge/msgnum", "2\n")
                .file("/dev/ash/.git/rebase-merge/end", "5\n")
                .file(
                    "/dev/ash/.git/rebase-merge/stopped-sha",
                    &format!("{STOPPED}\n"),
                )
                .file(
                    "/dev/ash/.git/rebase-merge/message",
                    "add the probe\n\nbody\n",
                );
            Self {
                fs,
                status: Some(parse_status(
                    "u UU N... 100644 100644 100644 100644 a b c src/probe.rs\n\
                     u UU N... 100644 100644 100644 100644 a b c src/app/main.ts\n",
                )),
                test_command: Some("cargo test".to_owned()),
            }
        }

        fn file(mut self, path: &str, content: &str) -> Self {
            self.fs.write(&format!("/dev/ash/.git/{path}"), content);
            self
        }

        fn without(mut self, path: &str) -> Self {
            self.fs.remove(&format!("/dev/ash/.git/{path}"));
            self
        }

        /// `git` n'a pas répondu : pas d'état d'arbre, donc pas de chemins.
        fn without_status(mut self) -> Self {
            self.status = None;
            self
        }

        fn status(mut self, status: Status) -> Self {
            self.status = Some(status);
            self
        }

        fn read(&self) -> Option<StoppedOperation> {
            let git_dir = Path::new("/dev/ash/.git");
            let mut metadata = read_metadata(&self.fs, git_dir, git_dir).unwrap();
            metadata.status.clone_from(&self.status);
            read_stopped(&self.fs, git_dir, &metadata, self.test_command.clone())
        }
    }

    #[test]
    fn given_a_rebase_stopped_on_a_conflict_when_reading_it_then_it_carries_the_paths_the_stopped_commit_and_the_rescue(
    ) {
        // Given — les trois choses que la spec §7.4 demande, et que seul Ash a sous la main
        let worktree = StoppedBuilder::new();

        // When
        let stopped = worktree.read().unwrap();

        // Then
        assert_eq!(
            stopped.conflicts,
            vec!["src/probe.rs".to_owned(), "src/app/main.ts".to_owned()]
        );
        assert_eq!(
            stopped.stopped_at,
            Some(StoppedCommit {
                commit: "1a2b3c4".to_owned(),
                subject: Some("add the probe".to_owned()),
            })
        );
        assert_eq!(stopped.orig_head, Some("80eca44".to_owned()));
        assert_eq!(
            stopped.operation.progress,
            Some(Progress { step: 2, total: 5 })
        );
    }

    #[test]
    fn given_a_stopped_rebase_when_reading_it_then_abort_and_skip_are_offered_as_text_to_show() {
        // Given — spec §7.4 : « `abort` et `skip` restent visibles ». Visible n'est pas
        // exécutable : `--abort` jette le travail, et Ash ne valide rien à la place de
        // l'utilisateur (ADR-0015).
        let worktree = StoppedBuilder::new();

        // When
        let stopped = worktree.read().unwrap();

        // Then
        assert_eq!(
            stopped.escapes,
            vec![
                "git rebase --abort".to_owned(),
                "git rebase --skip".to_owned()
            ]
        );
    }

    #[test]
    fn given_a_stopped_merge_when_reading_it_then_it_offers_no_skip_and_invents_no_stopped_commit()
    {
        // Given — un merge n'a qu'un pas : il s'arrête *entre* deux histoires, pas *sur*
        // un commit. Désigner `MERGE_HEAD` comme commit d'arrêt dirait autre chose.
        let worktree = StoppedBuilder::new()
            .without("rebase-merge")
            .file("MERGE_HEAD", &format!("{MAIN}\n"));

        // When
        let stopped = worktree.read().unwrap();

        // Then
        assert_eq!(stopped.stopped_at, None);
        assert_eq!(stopped.escapes, vec!["git merge --abort".to_owned()]);
    }

    #[test]
    fn given_a_rebase_run_by_the_apply_engine_when_reading_it_then_the_stopped_commit_comes_from_its_own_file(
    ) {
        // Given — `git rebase --apply` nomme `original-commit` ce que le moteur `merge`
        // appelle `stopped-sha` : le chercher au mauvais endroit perdrait le commit
        let worktree = StoppedBuilder::new().without("rebase-merge").file(
            "rebase-apply/original-commit",
            "9f8e7d6c5b4a39281706f5e4d3c2b1a098765432\n",
        );

        // When
        let stopped = worktree.read().unwrap();

        // Then — abrégé comme partout ailleurs, et sans sujet : ce moteur n'en écrit pas
        assert_eq!(
            stopped.stopped_at,
            Some(StoppedCommit {
                commit: "9f8e7d6".to_owned(),
                subject: None,
            })
        );
    }

    #[test]
    fn given_a_repository_where_git_did_not_answer_when_reading_a_stopped_rebase_then_no_conflict_count_is_invented(
    ) {
        // Given — `git` absent du `PATH`, ou dépôt trop gros pour le délai de 5 s. Rendre
        // zéro conflit ferait dire « rien à résoudre » au moment précis où il y a tout à
        // résoudre.
        let worktree = StoppedBuilder::new().without_status();

        // When
        let stopped = worktree.read().unwrap();

        // Then
        assert!(stopped.conflicts.is_empty());
        assert_eq!(stopped.conflicted_total, None);
    }

    #[test]
    fn given_a_rebase_with_more_conflicts_than_the_list_holds_when_reading_then_the_total_is_still_exact(
    ) {
        // Given — la liste des chemins est bornée par `porcelain` ; le compte ne l'est pas
        let worktree = StoppedBuilder::new().status(Status {
            tree: TreeStatus {
                conflicted: 3_000,
                ..TreeStatus::default()
            },
            upstream: None,
            conflicts: vec!["vendor/a.rs".to_owned()],
        });

        // When
        let stopped = worktree.read().unwrap();

        // Then
        assert_eq!(stopped.conflicts.len(), 1);
        assert_eq!(stopped.conflicted_total, Some(3_000));
    }

    #[test]
    fn given_a_worktree_with_nothing_in_progress_when_reading_then_there_is_nothing_to_show() {
        // Given — le cas courant, et de loin
        let worktree = StoppedBuilder::new()
            .without("rebase-merge")
            .file("HEAD", "ref: refs/heads/main\n");

        // When
        let stopped = worktree.read();

        // Then
        assert_eq!(stopped, None);
    }
}
