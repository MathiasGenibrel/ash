//! Les marqueurs de conflit, lus et réécrits — une **règle pure**.
//!
//! Ce que git a laissé dans le fichier du worktree est déjà la totalité de l'état : les
//! trois côtés d'un hunk y sont écrits, dans l'ordre, entre `<<<<<<<`, `=======` et
//! `>>>>>>>`. Ash ne tient donc **aucun brouillon** — il relit le fichier, et le réécrit
//! quand l'utilisateur tranche un hunk. C'est ce qui fait que fermer l'onglet ne perd rien
//! (spec §7.4) : il n'y a rien à perdre qui ne soit pas sur le disque.
//!
//! Aucun verbe git n'est nécessaire pour **lire** un conflit. C'était le choix à faire :
//! `git show :1:`, `:2:`, `:3:` auraient donné les mêmes trois côtés au prix de trois
//! invocations par fichier, sur un chemin que la frontière de sécurité de
//! `features::git::git_cli` oblige à rejustifier. Le fichier du worktree porte déjà tout,
//! et git le relira lui-même au `git add`.
//!
//! # Ce que ce module ne fait pas
//!
//! Il ne **choisit** jamais un côté. Le panneau central part vide et l'utilisateur écrit
//! dedans : préremplir avec `ours` serait une décision prise à sa place, et c'est
//! exactement ce qu'ADR-0015 refuse ailleurs.

/// Le préfixe d'un marqueur, tel que git l'écrit — sept caractères, toujours.
const MARKER: usize = 7;

/// Un morceau de fichier : du texte que personne ne conteste, ou un conflit.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Common(String),
    Conflict(Hunk),
}

/// Un conflit, tel que git l'a écrit dans le fichier.
///
/// `ours` et `theirs` sont le **jargon de git**, et ils s'arrêtent ici : c'est
/// [`super::sides`] qui leur met un nom de branche, et rien de ce qui traverse la
/// frontière ne porte ces deux mots (spec §7.4).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    /// Le rang du hunk dans le fichier, à partir de zéro. C'est par lui qu'une résolution
    /// le désigne — un chemin plus une position, jamais un contenu à retrouver.
    pub index: u32,
    /// Le côté que le marqueur ouvrant porte.
    pub ours: String,
    /// La base commune, quand `merge.conflictStyle` vaut `diff3` ou `zdiff3`.
    ///
    /// `None` dans la configuration par défaut de git, et ce n'est pas un manque : les
    /// trois panneaux de la spec sont `ours` / résultat / `theirs`, pas la base.
    pub base: Option<String>,
    /// Le côté que le marqueur fermant porte.
    pub theirs: String,
}

/// Un fichier en conflit, tel que l'onglet le montre.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    /// Le chemin relatif à la racine du worktree, tel que git l'écrit.
    pub path: String,
    pub hunks: Vec<Hunk>,
    /// Plus aucun marqueur dans le fichier.
    ///
    /// Déduit du **fichier**, pas de `git status` : l'état de l'index est rafraîchi par la
    /// surveillance, qui se limite à une lecture toutes les cinq secondes, et un compte de
    /// conflits qui traînerait de cinq secondes derrière le geste ferait clignoter le
    /// bouton `continue` à contretemps.
    pub resolved: bool,
    /// Le fichier n'a pas pu être lu — binaire illisible en UTF-8, effacé entre-temps.
    ///
    /// Il reste **listé** : un conflit qu'Ash n'arrive pas à ouvrir doit se voir, sinon le
    /// compte à droite de `continue` ne s'expliquerait pas.
    pub unreadable: bool,
}

/// Découpe un fichier en morceaux, sans rien interpréter d'autre que les marqueurs.
///
/// Une ligne qui commence par sept `<` ouvre un conflit ; tout ce qui suit jusqu'au
/// `>>>>>>>` en fait partie. Un marqueur ouvrant sans fermant — un fichier tronqué, ou du
/// texte qui *parle* de marqueurs — rend le reste du fichier en texte ordinaire plutôt
/// qu'un hunk inventé : réécrire un fichier sur une lecture fausse est la seule façon de
/// perdre du travail ici.
fn segments(text: &str) -> Vec<Segment> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut segments = Vec::new();
    let mut common = String::new();
    let mut at = 0;
    let mut index = 0;

    while at < lines.len() {
        let line = lines[at];
        if !opens(line) {
            common.push_str(line);
            at += 1;
            continue;
        }
        match read_conflict(&lines[at..], index) {
            Some((hunk, consumed)) => {
                if !common.is_empty() {
                    segments.push(Segment::Common(std::mem::take(&mut common)));
                }
                segments.push(Segment::Conflict(hunk));
                index += 1;
                at += consumed;
            }
            None => {
                // Marqueur sans fermeture : le reste du fichier est du texte, et rien d'autre.
                common.push_str(line);
                at += 1;
            }
        }
    }

    if !common.is_empty() {
        segments.push(Segment::Common(common));
    }
    segments
}

/// Lit un conflit complet à partir de son marqueur ouvrant, ou rien.
fn read_conflict(lines: &[&str], index: u32) -> Option<(Hunk, usize)> {
    let mut ours = String::new();
    let mut base: Option<String> = None;
    let mut theirs = String::new();
    // Trois phases : après `<<<<<<<`, après `|||||||`, après `=======`.
    let mut side = 0u8;

    for (offset, line) in lines.iter().enumerate().skip(1) {
        if marker(line, '>') {
            return Some((
                Hunk {
                    index,
                    ours,
                    base,
                    theirs,
                },
                offset + 1,
            ));
        }
        if marker(line, '|') && side == 0 {
            side = 1;
            base = Some(String::new());
            continue;
        }
        if marker(line, '=') && side < 2 {
            side = 2;
            continue;
        }
        // Un conflit imbriqué n'existe pas : git n'en écrit jamais. Un second `<<<<<<<`
        // avant la fermeture est donc du texte, et il est gardé tel quel.
        match side {
            0 => ours.push_str(line),
            1 => {
                if let Some(base) = base.as_mut() {
                    base.push_str(line);
                }
            }
            _ => theirs.push_str(line),
        }
    }
    None
}

fn opens(line: &str) -> bool {
    marker(line, '<')
}

/// Une ligne de marqueur : sept fois le même caractère, en tête de ligne.
///
/// Ce que git écrit ensuite — `HEAD`, un nom de branche, un identifiant de commit — est
/// ignoré : c'est **le nom de git**, et les côtés portent celui de leur branche (spec §7.4).
fn marker(line: &str, sign: char) -> bool {
    let trimmed = line.trim_end_matches(['\n', '\r']);
    trimmed.starts_with(&sign.to_string().repeat(MARKER))
        && !trimmed.chars().nth(MARKER).is_some_and(|next| next == sign)
}

/// Les hunks d'un fichier, dans l'ordre.
pub fn hunks(text: &str) -> Vec<Hunk> {
    segments(text)
        .into_iter()
        .filter_map(|segment| match segment {
            Segment::Conflict(hunk) => Some(hunk),
            Segment::Common(_) => None,
        })
        .collect()
}

/// Réécrit le fichier avec un hunk tranché — et **un seul**.
///
/// Rend `None` quand ce rang n'existe pas : le fichier a changé sous les doigts de
/// l'utilisateur, et écrire à l'aveugle sur un rang qui a bougé remplacerait le mauvais
/// conflit.
///
/// Le texte reçu gagne un saut de ligne final s'il n'en a pas : un hunk est fait de lignes
/// entières, et coller la ligne suivante à la dernière ligne du résultat produirait un
/// fichier que personne n'a écrit.
pub fn resolve(text: &str, index: u32, resolution: &str) -> Option<String> {
    let segments = segments(text);
    if !segments
        .iter()
        .any(|segment| matches!(segment, Segment::Conflict(hunk) if hunk.index == index))
    {
        return None;
    }

    let mut written = String::with_capacity(text.len());
    for segment in segments {
        match segment {
            Segment::Common(common) => written.push_str(&common),
            Segment::Conflict(hunk) if hunk.index == index => {
                written.push_str(&terminated(resolution));
            }
            Segment::Conflict(hunk) => written.push_str(&rewritten(&hunk)),
        }
    }
    Some(written)
}

/// Un hunk qu'on n'a pas tranché, réécrit **exactement** comme git l'avait posé.
///
/// Les libellés d'origine (`<<<<<<< HEAD`) ne sont pas conservés, et c'est sans effet :
/// git ne les relit pas — il relit l'index —, et personne d'autre qu'Ash ne lira ce
/// fichier entre deux gestes. Les garder demanderait de transporter deux chaînes de plus
/// dans la frontière pour un texte que la spec §7.4 dit justement de ne pas montrer.
fn rewritten(hunk: &Hunk) -> String {
    let mut written = String::new();
    written.push_str("<<<<<<< ours\n");
    written.push_str(&hunk.ours);
    if let Some(base) = &hunk.base {
        written.push_str("||||||| base\n");
        written.push_str(base);
    }
    written.push_str("=======\n");
    written.push_str(&hunk.theirs);
    written.push_str(">>>>>>> theirs\n");
    written
}

fn terminated(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        return text.to_owned();
    }
    format!("{text}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : un fichier en conflit, tel que git l'écrit.
    struct ConflictedFile {
        text: String,
    }

    impl ConflictedFile {
        fn new() -> Self {
            Self {
                text: "fn main() {\n\
                       <<<<<<< HEAD\n\
                       println!(\"main\");\n\
                       =======\n\
                       println!(\"feat\");\n\
                       >>>>>>> add the probe\n\
                       }\n"
                .to_owned(),
            }
        }

        fn text(text: &str) -> Self {
            Self {
                text: text.to_owned(),
            }
        }
    }

    #[test]
    fn given_a_file_git_left_in_conflict_when_reading_it_then_both_sides_come_out_whole() {
        // Given
        let file = ConflictedFile::new();

        // When
        let hunks = hunks(&file.text);

        // Then
        assert_eq!(
            hunks,
            vec![Hunk {
                index: 0,
                ours: "println!(\"main\");\n".to_owned(),
                base: None,
                theirs: "println!(\"feat\");\n".to_owned(),
            }]
        );
    }

    #[test]
    fn given_a_diff3_conflict_when_reading_it_then_the_base_is_kept_apart_from_the_two_sides() {
        // Given — `merge.conflictStyle = diff3` ajoute une troisième section, que le
        // découpage doit distinguer plutôt que de la coller à `ours`
        let file = ConflictedFile::text(
            "<<<<<<< HEAD\nmain\n||||||| base\nbase\n=======\nfeat\n>>>>>>> feat\n",
        );

        // When
        let hunks = hunks(&file.text);

        // Then
        assert_eq!(hunks[0].ours, "main\n");
        assert_eq!(hunks[0].base.as_deref(), Some("base\n"));
        assert_eq!(hunks[0].theirs, "feat\n");
    }

    #[test]
    fn given_a_file_with_two_conflicts_when_resolving_the_second_then_the_first_is_left_untouched()
    {
        // Given — c'est la garantie « hunk par hunk » : trancher l'un ne décide pas l'autre
        let file = ConflictedFile::text(
            "a\n<<<<<<< HEAD\none\n=======\nun\n>>>>>>> feat\n\
             b\n<<<<<<< HEAD\ntwo\n=======\ndeux\n>>>>>>> feat\nc\n",
        );

        // When
        let written = resolve(&file.text, 1, "deux").unwrap();

        // Then
        let left = hunks(&written);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].ours, "one\n");
        assert!(written.ends_with("b\ndeux\nc\n"));
    }

    #[test]
    fn given_a_resolution_typed_without_a_trailing_newline_when_writing_it_then_the_next_line_stays_a_line(
    ) {
        // Given — un panneau central est une zone de saisie : personne n'y tape le saut de
        // ligne final, et le coller à la ligne suivante produirait un fichier que personne
        // n'a écrit
        let file = ConflictedFile::new();

        // When
        let written = resolve(&file.text, 0, "println!(\"both\");").unwrap();

        // Then
        assert_eq!(written, "fn main() {\nprintln!(\"both\");\n}\n");
    }

    #[test]
    fn given_a_file_whose_hunk_has_already_moved_when_resolving_that_rank_then_nothing_is_written()
    {
        // Given — l'utilisateur a édité le fichier dans son éditeur pendant que l'onglet
        // était ouvert : écrire à l'aveugle sur un rang disparu remplacerait un autre
        // conflit que celui qu'on regardait
        let file = ConflictedFile::text("nothing to see\n");

        // When
        let written = resolve(&file.text, 0, "whatever");

        // Then
        assert_eq!(written, None);
    }

    #[test]
    fn given_a_marker_that_is_never_closed_when_reading_the_file_then_no_hunk_is_invented() {
        // Given — un fichier tronqué, ou du texte qui *parle* de marqueurs. Rendre un hunk
        // ici ferait réécrire un fichier sur une lecture fausse.
        let file = ConflictedFile::text("<<<<<<< HEAD\nonly one side\n");

        // When
        let hunks = hunks(&file.text);

        // Then
        assert!(hunks.is_empty());
    }

    #[test]
    fn given_a_resolved_conflict_when_the_file_is_read_again_then_the_other_hunks_are_still_git_markers(
    ) {
        // Given — le fichier réécrit est relu par `git add` : ce qui reste en conflit doit
        // rester un conflit *pour git*, pas seulement pour Ash
        let file = ConflictedFile::text(
            "<<<<<<< HEAD\none\n=======\nun\n>>>>>>> feat\n\
             <<<<<<< HEAD\ntwo\n=======\ndeux\n>>>>>>> feat\n",
        );

        // When
        let written = resolve(&file.text, 0, "one").unwrap();

        // Then
        assert!(written.contains("<<<<<<< "));
        assert!(written.contains("\n=======\n"));
        assert!(written.contains(">>>>>>> "));
        assert_eq!(hunks(&written).len(), 1);
    }
}
