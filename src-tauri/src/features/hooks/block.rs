//! Le bloc délimité : sa forme, sa pose, et sa relecture.
//!
//! Trois décisions gouvernent ce fichier, et aucune n'est un détail de formatage.
//!
//! ### 1. Le fichier est du **texte**, jamais un arbre relu et réécrit
//!
//! La tentation est d'ouvrir le `settings.json` avec `serde_json`, d'y insérer une clé et
//! de le réécrire. Ce serait un échec, même en produisant du JSON valide : un aller-retour
//! par `serde_json::Value` **réordonne les clés** (l'objet est trié), **normalise
//! l'indentation**, **perd les commentaires** que l'utilisateur y aurait mis, et récrit
//! chacun de ses nombres à sa façon. Le fichier appartient à l'utilisateur ; « rien n'est
//! modifié hors marqueurs » veut dire *pas un octet*.
//!
//! Ash découpe donc le fichier en trois : ce qui précède le bloc, le bloc, ce qui le suit.
//! Il ne recompose que le deuxième. Le seul JSON qu'il sérialise est le sien — le contenu
//! du bloc, produit par l'adaptateur, où l'échappement d'une chaîne ne se bricole pas.
//!
//! ### 2. Les marqueurs sont des **clés JSON**, pas des commentaires
//!
//! [ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md) esquisse `// ash:begin`. Un
//! commentaire serait plus lisible, mais il n'est pas du JSON : si le lecteur de l'outil ne
//! les tolère pas, poser le bloc rend la configuration de l'utilisateur **illisible en
//! entier**, et il perd tous ses réglages jusqu'à ce qu'il trouve pourquoi. Une clé
//! `"//ash:begin"` est du JSON valide partout ; au pire, un outil qui ne la connaît pas
//! l'ignore. L'asymétrie des deux risques tranche toute seule.
//!
//! ### 3. Le marqueur porte une **version** et une **empreinte**, qui ne servent pas à la
//! même chose
//!
//! - la **version** distingue « écrit par un Ash plus ancien » — qu'on réécrit sans rien
//!   demander — de « à jour » ;
//! - l'**empreinte** distingue ces deux cas-là de « quelqu'un y a touché », où Ash refuse
//!   d'écrire (spec §10).
//!
//! Sans empreinte, un bloc de la version courante modifié à la main passerait pour intact ;
//! sans version, une simple évolution du bloc ressemblerait à une édition de l'utilisateur
//! et bloquerait la mise à jour de tout le monde.

use std::ops::Range;

use crate::features::agents::Instrumentation;

/// La clé qui ouvre le bloc. Contient `ash:begin`, comme la spec §10 l'exige.
pub const BEGIN_KEY: &str = "\"//ash:begin\"";
/// La clé qui le ferme.
pub const END_KEY: &str = "\"//ash:end\"";

/// Ce qu'un fichier porte comme bloc d'Ash.
#[derive(Debug, PartialEq, Eq)]
pub enum Located {
    /// Aucun marqueur : Ash n'est jamais passé par ici.
    Absent,
    /// Un bloc qu'Ash reconnaît, intact ou non.
    Present(Block),
    /// Des marqueurs, mais pas un bloc lisible — un seul des deux, deux fois le même, un
    /// en-tête méconnaissable. C'est une **édition à la main** : personne d'autre qu'un
    /// humain ne produit ça.
    Damaged,
}

/// Un bloc trouvé dans un fichier.
#[derive(Debug, PartialEq, Eq)]
pub struct Block {
    /// Les octets qu'il occupe — exactement ceux que [`render`] avait insérés.
    ///
    /// C'est ce qui rend la désinstallation *exacte* : retirer cette plage rend le fichier
    /// d'avant, à l'octet près, sans ligne vide de reste.
    pub span: Range<usize>,
    /// La version d'Ash qui l'a écrit.
    pub version: u32,
    /// Ce que le bloc porte, tel que l'adaptateur l'avait composé.
    pub payload: String,
    /// L'empreinte inscrite dans le marqueur correspond-elle au contenu trouvé ?
    ///
    /// Faux = quelqu'un a édité le bloc à la main.
    pub intact: bool,
}

/// Le texte du bloc, prêt à être inséré ou à remplacer un bloc existant.
///
/// Il commence par un `\n` et ne finit **pas** par un : c'est ce qui fait que la plage
/// rendue par [`locate`] est exactement celle qui a été insérée, donc que la retirer restitue
/// le fichier d'origine au lieu d'y laisser une ligne vide.
///
/// `trailing_comma` dit si une entrée de l'utilisateur suit le bloc dans l'objet.
pub fn render(instrumentation: &Instrumentation, trailing_comma: bool) -> String {
    let comma = if trailing_comma { "," } else { "" };
    format!(
        "\n  {BEGIN_KEY}: \"{}\",\n{},\n  {END_KEY}: \"{}\"{comma}",
        header(instrumentation.version, fingerprint(&instrumentation.block)),
        instrumentation.block,
        footer(instrumentation.version),
    )
}

/// La ligne que l'utilisateur lit quand il ouvre son `settings.json`.
///
/// Elle dit qui écrit, quoi ne pas faire, et comment s'en débarrasser — dans cet ordre,
/// parce que c'est l'ordre des questions qu'on se pose en tombant dessus par surprise.
fn header(version: u32, fingerprint: u64) -> String {
    format!(
        "ash block v{version} · empreinte {fingerprint:016x} · \
         écrit par Ash, ne pas éditer — la désinstallation le retire sans laisser de trace"
    )
}

fn footer(version: u32) -> String {
    format!("ash block v{version}")
}

/// Où poser le bloc dans un objet JSON : juste après son accolade ouvrante.
///
/// **C'est le seul endroit de la feature qui sait que le fichier est un objet JSON.** Tous
/// les outils qu'Ash instrumente aujourd'hui rangent leurs réglages dans un objet JSON ;
/// le jour où l'un d'eux utilisera du TOML, ce point d'insertion deviendra une propriété
/// de l'[`Instrumentation`], que l'adaptateur connaît et que cette feature n'a pas à
/// deviner.
pub fn insertion_point(content: &str) -> Option<usize> {
    content.find('{').map(|brace| brace + 1)
}

/// Le fichier qu'Ash écrit quand il n'y en avait pas.
pub fn fresh_document(instrumentation: &Instrumentation) -> String {
    format!("{{{}\n}}\n", render(instrumentation, false))
}

/// Ne reste-t-il qu'un objet vide, une fois le bloc retiré ?
///
/// C'est la question de la désinstallation : un `settings.json` qui ne contient plus que
/// `{}` est un fichier qu'Ash a créé pour lui seul, et le laisser derrière serait une trace
/// de plus — la clé orpheline que le critère d'acceptation nomme.
pub fn is_an_empty_object(content: &str) -> bool {
    content.split_whitespace().collect::<String>() == "{}"
}

/// Faut-il une virgule après le bloc, vu ce qui le suit dans le fichier ?
pub fn is_followed_by_an_entry(rest: &str) -> bool {
    !rest.trim_start().starts_with('}')
}

/// Retrouve le bloc dans un fichier.
pub fn locate(content: &str) -> Located {
    let begins: Vec<usize> = content.match_indices(BEGIN_KEY).map(|(at, _)| at).collect();
    let ends: Vec<usize> = content.match_indices(END_KEY).map(|(at, _)| at).collect();

    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Located::Absent,
        ([begin], [end]) if begin < end => read_block(content, *begin, *end),
        // Un marqueur seul, deux blocs, ou l'ordre inversé : aucun de ces états ne sort de
        // `render`. Ils viennent d'une édition, d'un copier-coller, ou d'une fusion ratée.
        _ => Located::Damaged,
    }
}

fn read_block(content: &str, begin: usize, end: usize) -> Located {
    let (Some(head), Some(tail)) = (
        string_value_after(content, begin + BEGIN_KEY.len()),
        string_value_after(content, end + END_KEY.len()),
    ) else {
        return Located::Damaged;
    };

    let (Some(version), Some(recorded)) = (version_in(&head.text), fingerprint_in(&head.text))
    else {
        return Located::Damaged;
    };

    // Le début de la ligne du marqueur de fin, virgule de la charge utile comprise.
    let Some(payload_end) = line_start(content, end).checked_sub(1) else {
        return Located::Damaged;
    };
    let Some(payload) = content
        .get(head.after..payload_end)
        .and_then(|text| text.strip_prefix(",\n"))
        .and_then(|text| text.strip_suffix(','))
    else {
        return Located::Damaged;
    };

    Located::Present(Block {
        span: line_start(content, begin).saturating_sub(1)
            ..with_trailing_comma(content, tail.after),
        version,
        intact: fingerprint(payload) == recorded,
        payload: payload.to_owned(),
    })
}

/// Une valeur de chaîne JSON lue à la main, et pourquoi elle l'est.
///
/// Passer par un analyseur JSON complet demanderait de découper le fichier — donc de savoir
/// où il s'arrête — alors qu'on cherche justement à ne pas l'interpréter. Les deux seules
/// valeurs lues ici sont celles qu'Ash a lui-même écrites, et elles n'ont pas d'échappement.
struct StringValue {
    text: String,
    /// L'index juste après le guillemet fermant.
    after: usize,
}

fn string_value_after(content: &str, from: usize) -> Option<StringValue> {
    let rest = content.get(from..)?;
    let colon = rest.find(':')?;
    let opening = rest.get(colon + 1..)?.find('"')? + colon + 2;
    let closing = rest.get(opening..)?.find('"')? + opening;

    Some(StringValue {
        text: rest.get(opening..closing)?.to_owned(),
        after: from + closing + 1,
    })
}

/// Le début de la ligne qui contient cet index.
fn line_start(content: &str, at: usize) -> usize {
    content
        .get(..at)
        .and_then(|before| before.rfind('\n'))
        .map_or(0, |newline| newline + 1)
}

/// L'index après la virgule qui suit le bloc, s'il y en a une.
fn with_trailing_comma(content: &str, at: usize) -> usize {
    match content.get(at..at + 1) {
        Some(",") => at + 1,
        _ => at,
    }
}

fn version_in(header: &str) -> Option<u32> {
    let after = header.split_once("ash block v")?.1;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn fingerprint_in(header: &str) -> Option<u64> {
    let after = header.split_once("empreinte ")?.1;
    let digits: String = after
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .take(16)
        .collect();
    u64::from_str_radix(&digits, 16).ok()
}

/// L'empreinte d'un bloc — FNV-1a 64 bits.
///
/// **Elle détecte une modification, elle ne résiste pas à un adversaire**, et c'est
/// suffisant ici : quelqu'un capable de réécrire le `settings.json` de l'utilisateur y
/// contrôle déjà la ligne de commande que l'outil exécute à chaque hook. Une empreinte
/// cryptographique n'ajouterait aucune protection, seulement une dépendance de plus dans
/// l'arbre pour un fichier de vingt lignes.
///
/// FNV-1a est écrit ici en cinq lignes plutôt que pris ailleurs : `DefaultHasher` de la
/// bibliothèque standard **ne garantit pas la stabilité de ses valeurs entre versions de
/// Rust**, et une empreinte qui change à la mise à jour du compilateur ferait passer tous
/// les blocs installés pour édités à la main.
pub fn fingerprint(payload: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    payload.bytes().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Test Data Builder : une instrumentation dont on choisit ce qui compte au scénario.
    struct InstrumentationBuilder {
        block: String,
        version: u32,
    }

    impl InstrumentationBuilder {
        fn new() -> Self {
            Self {
                block: "  \"hooks\": {\n    \"Stop\": []\n  }".to_owned(),
                version: 1,
            }
        }

        fn version(mut self, version: u32) -> Self {
            self.version = version;
            self
        }

        fn carrying(mut self, block: &str) -> Self {
            self.block = block.to_owned();
            self
        }

        fn build(self) -> Instrumentation {
            Instrumentation {
                file: PathBuf::from("/home/someone/.claude/settings.json"),
                block: self.block,
                version: self.version,
            }
        }
    }

    /// Un fichier avec le bloc posé au bon endroit — comme `install` le ferait.
    fn with_block(document: &str, instrumentation: &Instrumentation) -> String {
        let at = insertion_point(document).unwrap();
        let (head, rest) = document.split_at(at);
        format!(
            "{head}{}{rest}",
            render(instrumentation, is_followed_by_an_entry(rest))
        )
    }

    #[test]
    fn given_a_settings_file_the_user_wrote_when_the_block_is_posed_and_removed_then_not_one_byte_of_it_moved(
    ) {
        // Given — la promesse la plus lourde du projet, prise à l'endroit où elle se casse :
        // clés dans un ordre choisi, indentation de quatre espaces, tableau sur une ligne.
        // Un aller-retour par `serde_json` rendrait tout cela « propre », donc différent.
        let theirs = "{\n    \"model\": \"opus\",\n    \"env\": {\"FOO\": \"bar\"},\n    \"permissions\": {\n        \"allow\": [\"Bash(ls:*)\"]\n    }\n}\n";
        let instrumentation = InstrumentationBuilder::new().build();

        // When
        let installed = with_block(theirs, &instrumentation);
        let Located::Present(block) = locate(&installed) else {
            panic!("le bloc posé doit se retrouver : {installed}");
        };
        let mut uninstalled = installed.clone();
        uninstalled.replace_range(block.span, "");

        // Then
        assert_eq!(uninstalled, theirs);
    }

    #[test]
    fn given_a_settings_file_with_a_single_entry_when_the_block_is_posed_then_the_json_stays_valid()
    {
        // Given — la virgule après le bloc dépend de ce qui le suit. L'oublier, ou en poser
        // une de trop devant l'accolade fermante, produit un fichier que Claude Code
        // refusera de lire : l'utilisateur perd tous ses réglages, à cause d'Ash.
        let documents = [
            "{}",
            "{\n}\n",
            "{\n  \"model\": \"opus\"\n}\n",
            "{ \"model\": \"opus\" }",
        ];
        let instrumentation = InstrumentationBuilder::new()
            .carrying("  \"hooks\": {\n    \"Stop\": []\n  }")
            .build();

        // When
        let posed: Vec<String> = documents
            .iter()
            .map(|document| with_block(document, &instrumentation))
            .collect();

        // Then
        for document in &posed {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(document);
            assert!(parsed.is_ok(), "JSON invalide :\n{document}");
            assert!(
                parsed.unwrap()["hooks"].is_object(),
                "les hooks ne sont pas là où l'outil les cherche :\n{document}"
            );
        }
    }

    #[test]
    fn given_a_block_someone_edited_by_hand_when_it_is_located_then_it_is_no_longer_intact() {
        // Given — l'utilisateur a changé la commande d'un hook « pour essayer ». C'est le
        // cas que la spec §10 protège : réécrire par-dessus effacerait son travail sans un
        // mot, et le fichier est le sien.
        let instrumentation = InstrumentationBuilder::new().build();
        let installed = with_block("{\n  \"model\": \"opus\"\n}\n", &instrumentation);
        let edited = installed.replace("\"Stop\": []", "\"Stop\": [\"mon script\"]");

        // When
        let located = locate(&edited);

        // Then
        let Located::Present(block) = located else {
            panic!("le bloc doit rester reconnaissable pour qu'on puisse en montrer le diff");
        };
        assert!(!block.intact);
    }

    #[test]
    fn given_a_block_written_by_an_older_ash_when_it_is_located_then_it_is_intact_but_out_of_date()
    {
        // Given — les deux situations ne demandent pas la même conduite : un bloc périmé se
        // réécrit sans rien demander, un bloc édité se signale. Les confondre, c'est soit
        // écraser le travail de l'utilisateur, soit ne plus jamais pouvoir mettre à jour.
        let older = InstrumentationBuilder::new().version(1).build();
        let installed = with_block("{\n  \"model\": \"opus\"\n}\n", &older);

        // When
        let located = locate(&installed);

        // Then
        let Located::Present(block) = located else {
            panic!("bloc introuvable");
        };
        assert!(block.intact, "il n'a pas été touché, seulement vieilli");
        assert_eq!(block.version, 1);
    }

    #[test]
    fn given_markers_the_user_broke_when_they_are_located_then_ash_treats_it_as_a_hand_edit() {
        // Given — un marqueur effacé, le bloc collé deux fois, l'en-tête réécrit. Aucun de
        // ces états ne sort d'Ash ; les traiter comme une absence de bloc ferait poser un
        // second bloc par-dessus le premier.
        let instrumentation = InstrumentationBuilder::new().build();
        let installed = with_block("{\n  \"model\": \"opus\"\n}\n", &instrumentation);
        let broken = [
            installed.replace(END_KEY, "\"//ash:fin\""),
            installed.replace(BEGIN_KEY, "\"//ash:debut\""),
            installed.replace("ash block v1 ·", "bloc ash ·"),
            format!("{installed}{installed}"),
        ];

        // When
        let located: Vec<Located> = broken.iter().map(|content| locate(content)).collect();

        // Then
        assert!(
            located.iter().all(|found| *found == Located::Damaged),
            "{located:?}"
        );
    }

    #[test]
    fn given_a_settings_file_ash_created_alone_when_its_block_is_removed_then_nothing_of_value_remains(
    ) {
        // Given — le dossier de configuration n'avait pas de `settings.json`. Retirer le
        // bloc doit rendre le fichier inutile, et c'est ce qui autorise `uninstall` à
        // l'effacer plutôt qu'à laisser une coquille vide (spec §10).
        let instrumentation = InstrumentationBuilder::new().build();
        let created = fresh_document(&instrumentation);

        // When
        let Located::Present(block) = locate(&created) else {
            panic!("bloc introuvable dans le fichier qu'Ash vient d'écrire :\n{created}");
        };
        let mut emptied = created.clone();
        emptied.replace_range(block.span, "");

        // Then
        assert!(
            serde_json::from_str::<serde_json::Value>(&created).is_ok(),
            "le fichier créé doit être du JSON valide :\n{created}"
        );
        assert!(is_an_empty_object(&emptied), "il reste : {emptied:?}");
    }

    #[test]
    fn given_two_blocks_differing_by_one_character_when_they_are_fingerprinted_then_the_two_prints_differ(
    ) {
        // Given — l'empreinte est la seule chose qui sépare « périmé » d'« édité ». Une
        // empreinte qui ne bouge pas sur un caractère laisserait Ash écraser une édition.
        let block = "  \"hooks\": {\n    \"Stop\": []\n  }";
        let touched = "  \"hooks\": {\n    \"Stop\": [] \n  }";

        // When
        let prints = (fingerprint(block), fingerprint(touched));

        // Then
        assert_ne!(prints.0, prints.1);
        assert_eq!(prints.0, fingerprint(block), "et elle est stable");
    }
}
