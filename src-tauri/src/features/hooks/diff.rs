//! Le diff que la spec §10 exige **avant** qu'Ash n'écrive.
//!
//! « Il signale, propose le diff, et demande. » Écrire — ou refuser — sans montrer ce qui
//! changerait laisse l'utilisateur devant un choix aveugle. Depuis l'amendement du
//! 2026-08-12 d'[ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), le diff n'est plus
//! seulement la forme d'un refus : c'est **ce sur quoi l'utilisateur tranche**, et il porte
//! le fichier tel qu'il est face au fichier tel qu'Ash le laisserait.
//!
//! Il est écrit ici plutôt que pris dans une bibliothèque : deux blocs de vingt lignes ne
//! justifient pas une dépendance, et la comparaison est celle du manuel — plus longue
//! sous-séquence commune, puis restitution.

/// Les deux versions du fichier, ligne à ligne, dans la forme que tout le monde sait lire.
///
/// `-` est le fichier tel qu'il est, `+` tel qu'Ash le laisserait : c'est le sens de lecture
/// d'un diff que l'on s'apprête à appliquer, et celui de la question posée à l'utilisateur —
/// « voici ce que j'ajouterais, et où ».
///
/// Les deux en-têtes sont en anglais parce qu'ils s'affichent (#68) ; le reste du fichier
/// est commenté en français, comme tout le dépôt.
pub fn preview(the_file_carries: &str, ash_would_leave: &str) -> String {
    between(the_file_carries, ash_would_leave, "what ash would write")
}

/// Le même diff, pour le geste inverse — ce que le fichier redevient quand Ash s'en va.
///
/// Seul l'en-tête change, et c'est tout ce qui doit changer : le sens de lecture est le
/// même — `-` le fichier tel qu'il est, `+` tel qu'Ash le laisserait — mais « ce qu'ash
/// écrirait » devant un retrait ferait lire un ajout là où il n'y a qu'une reprise.
pub fn preview_removal(the_file_carries: &str, ash_would_leave: &str) -> String {
    between(
        the_file_carries,
        ash_would_leave,
        "what ash would leave behind",
    )
}

fn between(the_file_carries: &str, ash_would_leave: &str, header: &str) -> String {
    let expected: Vec<&str> = the_file_carries.lines().collect();
    let found: Vec<&str> = ash_would_leave.lines().collect();

    let mut lines = vec!["--- the file as it is".to_owned(), format!("+++ {header}")];
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
    fn given_a_file_whose_middle_line_would_change_when_the_write_is_previewed_then_only_that_line_shows_up(
    ) {
        // Given — Ash réécrirait une commande de hook. Le diff n'a de valeur que s'il montre
        // *cette* ligne : un diff qui réaffiche les vingt lignes du fichier ne dit pas ce
        // qui va changer, et l'utilisateur l'acceptera sans regarder.
        let the_file_carries = "  \"hooks\": {\n    \"Stop\": \"mon script\"\n  }";
        let ash_would_leave = "  \"hooks\": {\n    \"Stop\": \"ash-event done\"\n  }";

        // When
        let diff = preview(the_file_carries, ash_would_leave);

        // Then
        assert_eq!(
            diff.lines().skip(2).collect::<Vec<_>>(),
            [
                "    \"hooks\": {",
                "-     \"Stop\": \"mon script\"",
                "+     \"Stop\": \"ash-event done\"",
                "    }",
            ]
        );
    }

    #[test]
    fn given_a_write_that_only_adds_a_line_when_it_is_previewed_then_the_untouched_lines_stay_common(
    ) {
        // Given — c'est exactement ce que fait la fusion : elle ajoute une entrée au milieu
        // de celles de l'utilisateur. La montrer comme un remplacement complet ferait croire
        // qu'Ash va écraser ce qui est là.
        let the_file_carries = "un\ndeux";
        let ash_would_leave = "un\nà moi\ndeux";

        // When
        let diff = preview(the_file_carries, ash_would_leave);

        // Then
        assert_eq!(
            diff.lines().skip(2).collect::<Vec<_>>(),
            ["  un", "+ à moi", "  deux"]
        );
    }
}
