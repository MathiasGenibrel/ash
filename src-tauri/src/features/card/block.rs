//! Trouver `<!-- ash:log -->` … `<!-- /ash:log -->` dans un fichier dont Ash ne sait rien.
//!
//! Une fiche est écrite par l'utilisateur et par des agents. Elle peut donc être n'importe
//! quoi : vide, sans bloc, avec un bloc ouvert et jamais refermé, avec deux blocs après une
//! fusion, avec des marqueurs de conflit git dedans — et, le cas qui a motivé la moitié de
//! ce fichier, **avec ses propres marqueurs cités dans une clôture de code**, ce qui est
//! exactement ce que fait ADR-0013 quand elle montre le format attendu.
//!
//! D'où la lecture ligne à ligne plutôt qu'un `find` : un marqueur ne compte que s'il est
//! **seul sur sa ligne** et **hors d'une clôture**. Une fiche qui documente le format d'Ash
//! ne doit pas se faire réécrire au milieu de son exemple.

use std::ops::Range;

/// Ce qui ouvre la zone d'Ash. Contrat public avec ADR-0013, à l'octet près.
pub const OPEN: &str = "<!-- ash:log -->";
/// Ce qui la referme.
pub const CLOSE: &str = "<!-- /ash:log -->";

/// Les trois marqueurs qu'un conflit git laisse dans un fichier.
///
/// Ash ne cherche pas à savoir *qui* est en conflit avec *quoi* : la seule chose qu'il en
/// fait est de ne rien toucher (ADR-0013 — « Ash ne résout jamais ce conflit tout seul »).
const CONFLICT_MARKS: [&str; 4] = ["<<<<<<<", "=======", ">>>>>>>", "|||||||"];

/// Ce que le fichier porte, du point de vue de la seule zone qui appartient à Ash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Zone {
    /// Aucun marqueur : la fiche existe, mais n'a jamais eu de journal.
    Absent,
    /// Une ouverture sans fermeture. **Un fichier tronqué, ou un marqueur effacé à la
    /// main** : dans les deux cas, Ash ne sait pas où sa zone s'arrête, donc il n'écrit pas.
    Unterminated,
    /// Deux ouvertures. C'est la signature d'une fusion mal recollée, et le cas où écrire
    /// dans « la » zone reviendrait à choisir laquelle — donc à trancher un conflit.
    Duplicated,
    /// Des marqueurs de conflit **dans** la zone.
    Conflicted { body: String },
    /// La zone, et ce qu'elle contient aujourd'hui.
    Present { inner: Range<usize>, body: String },
}

/// Où est la zone d'Ash dans ce texte, et dans quel état.
pub fn locate(content: &str) -> Zone {
    let markers = marks(content);
    let opens = markers.iter().filter(|mark| mark.opening).count();
    if opens > 1 {
        return Zone::Duplicated;
    }

    let Some(open) = markers.iter().find(|mark| mark.opening) else {
        return Zone::Absent;
    };
    let Some(close) = markers
        .iter()
        .find(|mark| !mark.opening && mark.start >= open.end)
    else {
        return Zone::Unterminated;
    };

    let inner = open.end..close.start;
    let body = content.get(inner.clone()).unwrap_or_default().to_owned();
    if carries_a_conflict(&body) {
        return Zone::Conflicted { body };
    }
    Zone::Present { inner, body }
}

/// Vrai si ce texte porte les traces d'une fusion que git n'a pas su faire.
///
/// La reconnaissance est volontairement large — un marqueur en début de ligne suffit —
/// parce que la conduite qui en découle est de **ne rien faire**. Se tromper coûte un
/// journal qui ne se met pas à jour et une phrase à l'écran ; ne pas voir un conflit
/// coûterait sa résolution silencieuse par Ash, ce qu'ADR-0013 interdit.
pub fn carries_a_conflict(text: &str) -> bool {
    text.lines()
        .any(|line| CONFLICT_MARKS.iter().any(|mark| line.starts_with(mark)))
}

/// Le bloc entier, prêt à être posé dans un fichier qui n'en a pas.
pub fn block(body: &str) -> String {
    let body = body.trim_end_matches('\n');
    if body.is_empty() {
        format!("{OPEN}\n{CLOSE}\n")
    } else {
        format!("{OPEN}\n{body}\n{CLOSE}\n")
    }
}

struct Mark {
    opening: bool,
    /// Le début de la **ligne** du marqueur : c'est là que la zone s'arrête.
    start: usize,
    /// Ce qui suit le retour à la ligne du marqueur : c'est là que la zone commence.
    end: usize,
}

/// Les marqueurs seuls sur leur ligne, **hors clôtures de code**.
fn marks(content: &str) -> Vec<Mark> {
    let mut found = Vec::new();
    let mut fenced = false;
    let mut at = 0usize;

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        } else if !fenced && (trimmed == OPEN || trimmed == CLOSE) {
            found.push(Mark {
                opening: trimmed == OPEN,
                start: at,
                end: at + line.len(),
            });
        }
        at += line.len();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::card::fakes::CardBuilder;

    #[test]
    fn given_a_card_that_quotes_ashs_own_markers_in_a_code_fence_when_the_zone_is_located_then_the_quote_is_not_it(
    ) {
        // Given — une fiche qui documente le format, comme ADR-0013 le fait. Un `find` naïf
        // aurait pris l'exemple pour la zone, et Ash aurait réécrit la documentation de la
        // fiche à la place de son journal.
        let card = CardBuilder::new()
            .quoting_the_format()
            .logging(
                "| agent | work | when |\n|---|---|---|\n| claude | 4 commits · 15m22s | now |\n",
            )
            .build();

        // When
        let zone = locate(&card);

        // Then — c'est bien le vrai bloc, celui qui porte la table
        let Zone::Present { body, .. } = zone else {
            panic!("la zone hors clôture n'a pas été trouvée : {zone:?}");
        };
        assert!(body.contains("4 commits"), "trouvé : {body:?}");
    }

    #[test]
    fn given_a_card_without_any_marker_when_the_zone_is_located_then_it_is_absent() {
        // Given — la fiche que l'utilisateur ou un agent vient d'écrire à la main
        let card = CardBuilder::new().without_a_block().build();

        // When / Then
        assert_eq!(locate(&card), Zone::Absent);
    }

    #[test]
    fn given_a_card_whose_closing_marker_was_deleted_when_the_zone_is_located_then_ash_does_not_know_where_it_ends(
    ) {
        // Given — Ash ne sait plus jusqu'où va sa zone. Prendre « jusqu'à la fin du
        // fichier » effacerait tout ce que l'utilisateur a écrit dessous.
        let card = CardBuilder::new().build().replace(CLOSE, "");

        // When / Then
        assert_eq!(locate(&card), Zone::Unterminated);
    }

    #[test]
    fn given_a_card_carrying_two_blocks_after_a_merge_when_the_zone_is_located_then_ash_refuses_to_pick_one(
    ) {
        // Given — la forme qu'une fusion recollée à la main laisse : les deux journaux se
        // suivent. Écrire dans « le » bloc reviendrait à choisir lequel garder.
        let card = format!(
            "{}{}",
            CardBuilder::new().build(),
            CardBuilder::new().build()
        );

        // When / Then
        assert_eq!(locate(&card), Zone::Duplicated);
    }

    #[test]
    fn given_a_block_left_in_conflict_by_a_merge_when_the_zone_is_located_then_it_reads_as_conflicted(
    ) {
        // Given — le cas qu'ADR-0013 nomme : deux branches, deux journaux, une fusion.
        let card = CardBuilder::new()
            .logging(
                "<<<<<<< HEAD\n| claude | 4 commits · 15m22s | now |\n=======\n| codex | 1 commit · 2m | now |\n>>>>>>> other\n",
            )
            .build();

        // When
        let zone = locate(&card);

        // Then
        assert!(
            matches!(zone, Zone::Conflicted { .. }),
            "un conflit doit être reconnu comme tel, pas écrasé : {zone:?}"
        );
    }

    #[test]
    fn given_an_empty_file_when_the_zone_is_located_then_it_is_absent_rather_than_broken() {
        // Given — un `.ash/worktree.md` créé par un `touch`, ou vidé
        // When / Then
        assert_eq!(locate(""), Zone::Absent);
    }
}
