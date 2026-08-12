//! Le diff que la spec §10 exige quand Ash refuse d'écrire.
//!
//! « Il signale, propose le diff, et demande. » Refuser sans montrer ce qui diffère laisse
//! l'utilisateur devant un choix aveugle : garder un bloc dont il ne sait plus ce qu'il a
//! changé, ou tout effacer. Le diff est donc une partie du refus, pas un agrément.
//!
//! Il est écrit ici plutôt que pris dans une bibliothèque : deux blocs de vingt lignes ne
//! justifient pas une dépendance, et la comparaison est celle du manuel — plus longue
//! sous-séquence commune, puis restitution.

/// Les deux blocs, ligne à ligne, dans la forme que tout le monde sait lire.
///
/// `-` est ce qu'Ash écrirait, `+` ce que le fichier porte : le sens de lecture est celui
/// de la question posée à l'utilisateur — « voici ce que j'allais mettre, voici ce que tu
/// as mis ».
pub fn compare(ash_would_write: &str, the_file_carries: &str) -> String {
    let expected: Vec<&str> = ash_would_write.lines().collect();
    let found: Vec<&str> = the_file_carries.lines().collect();

    let mut lines = vec![
        "--- ce qu'Ash écrirait".to_owned(),
        "+++ ce que le fichier porte".to_owned(),
    ];
    lines.extend(walk(&expected, &found, &common_lengths(&expected, &found)));
    lines.join("\n")
}

/// La table des longueurs de plus longue sous-séquence commune.
fn common_lengths(expected: &[&str], found: &[&str]) -> Vec<Vec<usize>> {
    let mut table = vec![vec![0; found.len() + 1]; expected.len() + 1];
    for (row, left) in expected.iter().enumerate().rev() {
        for (column, right) in found.iter().enumerate().rev() {
            table[row][column] = if left == right {
                table[row + 1][column + 1] + 1
            } else {
                table[row + 1][column].max(table[row][column + 1])
            };
        }
    }
    table
}

fn walk(expected: &[&str], found: &[&str], table: &[Vec<usize>]) -> Vec<String> {
    let mut lines = Vec::new();
    let (mut row, mut column) = (0, 0);

    while row < expected.len() && column < found.len() {
        if expected[row] == found[column] {
            lines.push(format!("  {}", expected[row]));
            row += 1;
            column += 1;
        } else if table[row + 1][column] >= table[row][column + 1] {
            lines.push(format!("- {}", expected[row]));
            row += 1;
        } else {
            lines.push(format!("+ {}", found[column]));
            column += 1;
        }
    }

    lines.extend(expected[row..].iter().map(|line| format!("- {line}")));
    lines.extend(found[column..].iter().map(|line| format!("+ {line}")));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_block_whose_middle_line_was_changed_when_it_is_compared_then_only_that_line_shows_up(
    ) {
        // Given — l'utilisateur a retouché une commande de hook. Le diff n'a de valeur que
        // s'il montre *sa* ligne : un diff qui réaffiche les vingt lignes du bloc ne lui
        // dit pas ce qu'il a fait, et il l'effacera sans regarder.
        let ash_would_write = "  \"hooks\": {\n    \"Stop\": \"ash-event done\"\n  }";
        let the_file_carries = "  \"hooks\": {\n    \"Stop\": \"mon script\"\n  }";

        // When
        let diff = compare(ash_would_write, the_file_carries);

        // Then
        assert_eq!(
            diff.lines().skip(2).collect::<Vec<_>>(),
            [
                "    \"hooks\": {",
                "-     \"Stop\": \"ash-event done\"",
                "+     \"Stop\": \"mon script\"",
                "    }",
            ]
        );
    }

    #[test]
    fn given_a_block_with_a_line_added_by_hand_when_it_is_compared_then_the_untouched_lines_stay_common(
    ) {
        // Given — le cas le plus fréquent : l'utilisateur ajoute son propre hook dans le
        // bloc d'Ash. Le montrer comme un remplacement complet ferait croire à un conflit
        // là où il n'a fait qu'ajouter.
        let ash_would_write = "un\ndeux";
        let the_file_carries = "un\nà moi\ndeux";

        // When
        let diff = compare(ash_would_write, the_file_carries);

        // Then
        assert_eq!(
            diff.lines().skip(2).collect::<Vec<_>>(),
            ["  un", "+ à moi", "  deux"]
        );
    }
}
