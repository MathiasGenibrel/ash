//! Ce qu'Ash écrit dans sa zone, et comment il reconnaît ce qu'il y avait laissé.
//!
//! ```markdown
//! | agent | work | when |
//! |---|---|---|
//! | claude | 4 commits · 15m22s | now |
//! ```
//!
//! C'est la table d'ADR-0013, au caractère près, et c'est du markdown que n'importe quel
//! éditeur affiche déjà : la fiche ne porte **aucune syntaxe propre à Ash**.
//!
//! # Comment Ash sait qu'on n'a pas touché à son bloc
//!
//! Le régime exige de « refuser d'écrire si le bloc a été édité à la main ». Reste à savoir
//! ce que veut dire *à la main*. Trois réponses étaient possibles ; deux ont été écartées :
//!
//! - **une empreinte dans le marqueur** (`<!-- ash:log fnv:… -->`) : exacte, mais elle
//!   inventerait la syntaxe propre à Ash qu'ADR-0013 refuse en toutes lettres, et elle
//!   partirait dans la *pull request* sous les yeux de gens qui n'ont pas Ash ;
//! - **un double du bloc gardé dans `~/.ash/`** : exact aussi, mais il ne voyage pas — or la
//!   fiche, elle, voyage avec la branche. Sur la machine du collègue, Ash tiendrait tout
//!   bloc pour édité à la main, et le refus serait permanent ;
//! - **la grammaire** — retenue. Ce qu'Ash laisse est entièrement dérivé du journal, donc
//!   entièrement décrit par [`table`] : « ce bloc est le mien » et « ce bloc est une table
//!   que j'aurais pu écrire » sont la même phrase. Elle ne dépend d'aucun état gardé, elle
//!   voyage, et elle n'ajoute rien au fichier.
//!
//! **Sa limite, qu'il faut connaître** : une ligne écrite à la main *dans la grammaire* —
//! `| bob | 3 commits · 1m00s | now |` — passe pour une ligne d'Ash et sera remplacée. La
//! grammaire est étroite (un compte de commits, une durée, un « il y a ») pour que ça
//! demande de l'obstination, mais elle ne le rend pas impossible. Ce qui est garanti reste
//! entier : rien **hors** du bloc n'est touché, une sauvegarde précède l'écriture, et le
//! diff est montré avant.

use crate::shared::time::UnixMillis;

use super::ports::WorkRecord;

/// Le travail d'un agent dans ce worktree, tel que le journal l'a observé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    pub agent: String,
    pub commits: usize,
    /// Le premier et le dernier commit observés, en secondes Unix — la matière de `work` et
    /// de `when`.
    pub first: u64,
    pub last: u64,
}

/// Les commits observés, groupés par agent, du plus récent au plus ancien.
///
/// Le groupement est par **commande** (`claude`, `claude-perso`, `codex`) et non par
/// adaptateur : c'est ce que le journal écrit (ADR-0014), et c'est ce qui distingue deux
/// comptes d'un même outil.
pub fn tally(records: &[WorkRecord]) -> Vec<Work> {
    let mut rows: Vec<Work> = Vec::new();
    for record in records {
        match rows.iter_mut().find(|row| row.agent == record.agent) {
            Some(row) => {
                row.commits += 1;
                row.first = row.first.min(record.authored_at);
                row.last = row.last.max(record.authored_at);
            }
            None => rows.push(Work {
                agent: record.agent.clone(),
                commits: 1,
                first: record.authored_at,
                last: record.authored_at,
            }),
        }
    }
    rows.sort_by(|left, right| {
        right
            .last
            .cmp(&left.last)
            .then_with(|| left.agent.cmp(&right.agent))
    });
    rows
}

/// La table, telle qu'elle part dans le bloc.
///
/// Une absence de travail rend une **chaîne vide**, et non une table sans ligne : un
/// en-tête seul se lirait comme une panne, alors qu'un worktree où aucun agent n'a encore
/// commité est le cas de tous les worktrees, au début.
pub fn table(rows: &[Work], now: UnixMillis) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let now = now / 1_000;
    let mut lines = vec![
        "| agent | work | when |".to_owned(),
        "|---|---|---|".to_owned(),
    ];
    for row in rows {
        lines.push(format!(
            "| {} | {} · {} | {} |",
            row.agent,
            commits(row.commits),
            span(row.last.saturating_sub(row.first)),
            ago(now.saturating_sub(row.last)),
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

/// Ce bloc est-il bien ce qu'Ash y aurait laissé ?
///
/// Un bloc vide en fait partie : c'est celui qu'Ash vient de poser, ou celui d'une branche
/// où aucun agent n'a encore commité.
pub fn is_ours(body: &str) -> bool {
    let mut lines = body.trim().lines();
    let Some(header) = lines.next() else {
        return true;
    };
    if header.trim() != "| agent | work | when |" {
        return false;
    }
    if lines.next().map(str::trim) != Some("|---|---|---|") {
        return false;
    }
    lines.all(|line| is_a_row(line.trim()))
}

/// La grammaire d'une ligne — la seule chose qui distingue le journal d'Ash d'une table que
/// quelqu'un a écrite à la main.
fn is_a_row(line: &str) -> bool {
    let Some(inner) = line
        .strip_prefix('|')
        .and_then(|rest| rest.strip_suffix('|'))
    else {
        return false;
    };
    let cells: Vec<&str> = inner.split('|').map(str::trim).collect();
    let [agent, work, when] = cells[..] else {
        return false;
    };
    !agent.is_empty() && is_work(work) && is_ago(when)
}

/// `4 commits · 15m22s`
fn is_work(cell: &str) -> bool {
    let Some((count, duration)) = cell.split_once(" · ") else {
        return false;
    };
    let Some((number, noun)) = count.split_once(' ') else {
        return false;
    };
    number.parse::<usize>().is_ok()
        && (noun == "commit" || noun == "commits")
        && !duration.is_empty()
        && duration
            .chars()
            .all(|letter| letter.is_ascii_digit() || "dhms".contains(letter))
}

/// `now`, `3m ago`, `2h ago`, `5d ago`
fn is_ago(cell: &str) -> bool {
    if cell == "now" {
        return true;
    }
    let Some(amount) = cell.strip_suffix(" ago") else {
        return false;
    };
    let (number, unit) = amount.split_at(amount.len().saturating_sub(1));
    !number.is_empty()
        && number.chars().all(|letter| letter.is_ascii_digit())
        && matches!(unit, "m" | "h" | "d")
}

fn commits(count: usize) -> String {
    if count == 1 {
        "1 commit".to_owned()
    } else {
        format!("{count} commits")
    }
}

/// Une durée dite comme la ligne de statut la dit — `15m22s`, et jamais `0s`.
fn span(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m{:02}s", seconds / 60, seconds % 60),
        3600..=86_399 => format!("{}h{:02}m", seconds / 3600, (seconds % 3600) / 60),
        _ => format!("{}d{:02}h", seconds / 86_400, (seconds % 86_400) / 3600),
    }
}

/// Depuis quand, à la maille où ça reste lisible.
///
/// Volontairement **grossier** : la fiche est un fichier versionné, et une valeur à la
/// seconde ferait un diff à chaque écriture. La minute est déjà la maille la plus fine qui
/// vaille dans un document qu'on commite.
fn ago(seconds: u64) -> String {
    match seconds {
        0..=59 => "now".to_owned(),
        60..=3599 => format!("{}m ago", seconds / 60),
        3600..=86_399 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : un commit observé par le journal.
    fn wrote(agent: &str, at: u64) -> WorkRecord {
        WorkRecord {
            agent: agent.to_owned(),
            authored_at: at,
        }
    }

    const NOON: u64 = 1_755_000_000;

    #[test]
    fn given_several_commits_by_the_same_agent_when_the_log_is_tallied_then_it_is_one_row_spanning_them(
    ) {
        // Given — quatre commits de `claude` sur un quart d'heure, et un de `codex` avant.
        // La table d'ADR-0013 a une ligne par agent, pas une par commit.
        let observed = [
            wrote("claude", NOON),
            wrote("codex", NOON - 3_600),
            wrote("claude", NOON - 922),
            wrote("claude", NOON - 100),
            wrote("claude", NOON - 500),
        ];

        // When
        let rows = tally(&observed);

        // Then — le plus récent en tête, et la durée est celle du premier au dernier
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].agent, "claude");
        assert_eq!(rows[0].commits, 4);
        assert_eq!(rows[0].last - rows[0].first, 922);
    }

    #[test]
    fn given_a_tallied_agent_when_the_table_is_written_then_it_reads_like_the_adr_example() {
        // Given — l'exemple d'ADR-0013, à reproduire au caractère près : c'est un contrat de
        // format, et c'est aussi ce que la relecture du bloc reconnaîtra plus tard.
        let rows = [Work {
            agent: "claude".to_owned(),
            commits: 4,
            first: NOON - 922,
            last: NOON,
        }];

        // When
        let written = table(&rows, NOON * 1_000);

        // Then
        assert_eq!(
            written,
            "| agent | work | when |\n|---|---|---|\n| claude | 4 commits · 15m22s | now |\n"
        );
    }

    #[test]
    fn given_a_worktree_where_no_agent_has_committed_when_the_table_is_written_then_it_stays_empty()
    {
        // Given — le cas de tous les worktrees au début. Un en-tête seul se lirait comme une
        // panne du journal.
        // When
        let written = table(&[], NOON * 1_000);

        // Then
        assert!(written.is_empty());
        // …et un bloc vide reste reconnaissable comme celui d'Ash : sans ça, la première
        // écriture réelle refuserait au motif que « le bloc a été édité ».
        assert!(is_ours(&written));
    }

    #[test]
    fn given_the_block_ash_last_wrote_when_it_is_read_back_then_ash_recognizes_it_as_his() {
        // Given — le tour suivant : Ash relit ce qu'il a écrit. C'est la boucle qui doit se
        // fermer, sans quoi la deuxième écriture refuserait toujours.
        let rows = [Work {
            agent: "claude".to_owned(),
            commits: 1,
            first: NOON - 7_300,
            last: NOON - 7_300,
        }];
        let written = table(&rows, NOON * 1_000);

        // When / Then
        assert!(is_ours(&written), "Ash ne se reconnaît pas : {written:?}");
    }

    #[test]
    fn given_a_block_where_someone_added_a_sentence_when_it_is_read_then_ash_does_not_claim_it() {
        // Given — quelqu'un a annoté le journal. C'est exactement le cas que la spec §10
        // couvre : Ash ne réécrit pas silencieusement.
        let edited = "| agent | work | when |\n|---|---|---|\n| claude | 4 commits · 15m22s | now |\n\nnote : le dernier commit est un revert.\n";

        // When / Then
        assert!(!is_ours(edited));
    }

    #[test]
    fn given_a_block_the_user_replaced_with_his_own_table_when_it_is_read_then_ash_does_not_claim_it(
    ) {
        // Given — une table bien formée, mais qui n'est pas celle d'Ash : mêmes colonnes,
        // contenu libre.
        let theirs =
            "| agent | work | when |\n|---|---|---|\n| moi | relecture complète | hier soir |\n";

        // When / Then
        assert!(!is_ours(theirs));
    }
}
