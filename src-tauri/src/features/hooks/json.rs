//! Lire la **structure** d'un fichier JSON sans jamais le relire en arbre.
//!
//! Toute la feature repose sur une règle que ce fichier rend possible : le fichier de
//! l'utilisateur est du texte, et Ash n'en récrit que les octets qui lui appartiennent
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), amendement du 2026-08-12). Un
//! aller-retour par `serde_json::Value` réordonnerait ses clés, normaliserait son
//! indentation et perdrait ses commentaires : « rien n'est modifié hors de ce qui est à
//! Ash » veut dire *pas un octet*.
//!
//! Ce lecteur ne rend donc **aucune valeur** : il rend des **plages**. Où commence l'objet
//! racine, où est la valeur de telle clé, où sont les éléments de tel tableau. C'est
//! exactement ce dont la fusion a besoin — un point d'insertion et des bornes — et rien de
//! plus. Il n'y a ici ni désérialisation, ni typage, ni validation : un fichier qu'il ne
//! sait pas lire rend `None`, et Ash refuse alors d'écrire plutôt que de deviner.
//!
//! Il remplace la détection grossière d'avant — « le mot `"hooks"` apparaît quelque part » —
//! qui refusait sur une occurrence dans une chaîne, et qui ne savait pas dire *quels* hooks
//! le fichier portait.

use std::ops::Range;

/// Une entrée d'objet JSON, telle qu'elle occupe le fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// La clé, déjà retirée de ses guillemets. Les échappements ne sont pas résolus : les
    /// clés qu'Ash cherche (`hooks`, `Stop`) n'en portent pas, et en résoudre demanderait
    /// de décider ce qu'on fait des autres.
    pub key: String,
    /// Du premier guillemet de la clé au dernier octet de la valeur.
    pub span: Range<usize>,
    /// La valeur seule.
    pub value: Range<usize>,
}

/// L'objet racine du fichier, s'il y en a un.
pub fn root_object(content: &str) -> Option<Range<usize>> {
    let start = content.find(|character: char| !character.is_whitespace())?;
    if content.as_bytes().get(start) != Some(&b'{') {
        return None;
    }
    Some(start..value_end(content, start)?)
}

/// Les entrées d'un objet, dans l'ordre du fichier.
///
/// `None` si la plage n'est pas un objet, ou si le fichier est tronqué : la feature refuse
/// alors d'écrire, ce qui est la seule conduite sûre devant un fichier qu'on ne comprend pas.
pub fn entries(content: &str, object: &Range<usize>) -> Option<Vec<Entry>> {
    let bytes = content.as_bytes();
    if bytes.get(object.start) != Some(&b'{') {
        return None;
    }

    let mut found = Vec::new();
    let mut at = object.start + 1;
    loop {
        at = skip_space(content, at);
        match bytes.get(at)? {
            b'}' => return Some(found),
            b',' => {
                at += 1;
                continue;
            }
            b'"' => {}
            _ => return None,
        }

        let key_end = string_end(content, at)?;
        let key = content.get(at + 1..key_end - 1)?.to_owned();
        let colon = skip_space(content, key_end);
        if bytes.get(colon) != Some(&b':') {
            return None;
        }
        let value_start = skip_space(content, colon + 1);
        let value_end = value_end(content, value_start)?;
        found.push(Entry {
            key,
            span: at..value_end,
            value: value_start..value_end,
        });
        at = value_end;
    }
}

/// Les éléments d'un tableau, dans l'ordre du fichier.
pub fn items(content: &str, array: &Range<usize>) -> Option<Vec<Range<usize>>> {
    let bytes = content.as_bytes();
    if bytes.get(array.start) != Some(&b'[') {
        return None;
    }

    let mut found = Vec::new();
    let mut at = array.start + 1;
    loop {
        at = skip_space(content, at);
        match bytes.get(at)? {
            b']' => return Some(found),
            b',' => {
                at += 1;
                continue;
            }
            _ => {}
        }
        let end = value_end(content, at)?;
        found.push(at..end);
        at = end;
    }
}

/// Est-ce une plage qui commence par `{` ?
pub fn is_object(content: &str, span: &Range<usize>) -> bool {
    content.as_bytes().get(span.start) == Some(&b'{')
}

/// Est-ce une plage qui commence par `[` ?
pub fn is_array(content: &str, span: &Range<usize>) -> bool {
    content.as_bytes().get(span.start) == Some(&b'[')
}

/// Un conteneur porte-t-il déjà quelque chose, vu depuis son accolade ouvrante ?
///
/// C'est la question de la virgule : Ash insère **en tête** de conteneur, donc ce qu'il
/// écrit est suivi d'une virgule si et seulement si quelque chose suit. En poser une de
/// trop devant l'accolade fermante produirait un fichier que l'outil refuserait de lire —
/// et l'utilisateur perdrait tous ses réglages, à cause d'Ash.
pub fn holds_something(content: &str, anchor: usize) -> bool {
    !matches!(
        content.as_bytes().get(skip_space(content, anchor)),
        Some(b'}') | Some(b']') | None
    )
}

fn skip_space(content: &str, from: usize) -> usize {
    let bytes = content.as_bytes();
    let mut at = from;
    while matches!(bytes.get(at), Some(byte) if byte.is_ascii_whitespace()) {
        at += 1;
    }
    at
}

/// L'index juste après la valeur qui commence ici.
fn value_end(content: &str, from: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    match bytes.get(from)? {
        b'"' => string_end(content, from),
        b'{' | b'[' => nested_end(content, from),
        // Un nombre, `true`, `false`, `null` : tout ce qui s'arrête au premier séparateur.
        // On ne le valide pas — Ash n'a pas à juger la valeur, seulement à savoir où elle
        // finit pour ne pas la toucher.
        _ => {
            let mut at = from;
            while let Some(byte) = bytes.get(at) {
                if matches!(byte, b',' | b'}' | b']') || byte.is_ascii_whitespace() {
                    break;
                }
                at += 1;
            }
            (at > from).then_some(at)
        }
    }
}

/// L'index juste après le guillemet fermant.
///
/// Les échappements comptent : sans eux, un `\"` au milieu d'une valeur ferait croire à la
/// fin de la chaîne, et tout ce qui suit serait lu comme de la structure. C'est exactement
/// le genre de méprise qui ferait écrire Ash au milieu d'une valeur de l'utilisateur.
fn string_end(content: &str, from: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut at = from + 1;
    while let Some(byte) = bytes.get(at) {
        match byte {
            b'\\' => at += 2,
            b'"' => return Some(at + 1),
            _ => at += 1,
        }
    }
    None
}

fn nested_end(content: &str, from: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut depth = 0usize;
    let mut at = from;
    while let Some(byte) = bytes.get(at) {
        match byte {
            b'"' => {
                at = string_end(content, at)?;
                continue;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_settings_file_whose_values_contain_braces_and_quotes_when_its_entries_are_read_then_each_one_spans_exactly_its_own_text(
    ) {
        // Given — le fichier de l'utilisateur porte une accolade dans une chaîne et un
        // guillemet échappé. Un lecteur qui compte les accolades sans regarder les chaînes
        // croirait l'objet fini au milieu, et Ash écrirait alors dans une valeur à lui
        let content =
            "{\n  \"greeting\": \"a } and a \\\" inside\",\n  \"hooks\": {\"Stop\": []}\n}\n";
        let root = root_object(content).expect("la racine est un objet");

        // When
        let found = entries(content, &root).expect("les entrées se lisent");

        // Then
        let keys: Vec<&str> = found.iter().map(|entry| entry.key.as_str()).collect();
        assert_eq!(keys, ["greeting", "hooks"]);
        let hooks = &found[1];
        assert_eq!(
            content.get(hooks.value.clone()),
            Some("{\"Stop\": []}"),
            "la valeur de `hooks` s'arrête à sa propre accolade"
        );
    }

    #[test]
    fn given_a_hooks_key_that_is_only_a_word_in_a_string_when_the_entries_are_read_then_it_is_not_taken_for_a_key(
    ) {
        // Given — la détection d'avant refusait d'écrire sur toute occurrence du mot
        // `"hooks"`, y compris dans une valeur. L'utilisateur perdait l'installation pour
        // une phrase qu'il avait écrite
        let content = "{\n  \"note\": \"pense à tes \\\"hooks\\\"\"\n}\n";
        let root = root_object(content).expect("la racine est un objet");

        // When
        let found = entries(content, &root).expect("les entrées se lisent");

        // Then
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "note");
    }

    #[test]
    fn given_an_array_of_objects_on_one_line_when_its_items_are_read_then_each_item_keeps_its_own_bounds(
    ) {
        // Given — la forme réelle d'un `settings.json` écrit à la main : tout sur une ligne.
        // Les bornes de chaque élément sont ce qui permet d'en retirer un sans toucher aux
        // autres
        let content =
            "{\"hooks\": {\"PreToolUse\": [{\"matcher\": \"Bash\"}, {\"matcher\": \"Read\"}]}}";
        let root = root_object(content).expect("la racine est un objet");
        let hooks = &entries(content, &root).expect("les entrées")[0];
        let events = entries(content, &hooks.value).expect("hooks est un objet");

        // When
        let found = items(content, &events[0].value).expect("c'est un tableau");

        // Then
        let texts: Vec<&str> = found
            .iter()
            .filter_map(|item| content.get(item.clone()))
            .collect();
        assert_eq!(
            texts,
            ["{\"matcher\": \"Bash\"}", "{\"matcher\": \"Read\"}"]
        );
    }

    #[test]
    fn given_a_file_that_is_not_a_json_object_when_its_root_is_read_then_nothing_is_found() {
        // Given — un `settings.json` remplacé par une liste, ou par des notes. Deviner où
        // écrire produirait un fichier que l'outil ne lit plus (critère : ça reste un refus)
        let refused = ["[1, 2, 3]\n", "des notes\n", "", "{\"hooks\": {"];

        // When
        let read: Vec<Option<Range<usize>>> =
            refused.iter().map(|content| root_object(content)).collect();

        // Then
        assert!(read.iter().all(Option::is_none), "{read:?}");
    }
}
