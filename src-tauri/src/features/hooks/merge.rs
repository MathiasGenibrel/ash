//! La fusion : poser les entrées d'Ash **à côté** de celles de l'utilisateur, et les
//! reprendre sans toucher aux siennes.
//!
//! C'est ce que l'amendement du 2026-08-12 d'[ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)
//! a rendu possible. Le bloc délimité savait poser une région entière ; il ne savait pas
//! cohabiter. Un `~/.claude/settings.json` qui portait déjà un `PreToolUse` — la
//! configuration de tout utilisateur qui outille déjà Claude Code, donc exactement le public
//! d'Ash — rendait la fonction centrale du produit inatteignable.
//!
//! Trois règles gouvernent ce fichier :
//!
//! ### 1. Ash insère **en tête** de conteneur
//!
//! Chaque insertion commence juste après l'accolade ou le crochet ouvrant. C'est ce qui rend
//! le retrait *exact* : le texte à retirer commence à un point qu'on retrouve sans rien
//! avoir mémorisé, et la virgule à poser ne dépend que d'une question — « quelque chose
//! suit-il ? ».
//!
//! ### 2. Le retrait ne supprime que ce qu'il sait **réécrire**
//!
//! Retirer se fait en recomposant le texte qu'Ash aurait écrit, à partir de ce que le
//! fichier porte, et en le comparant **octet par octet** à ce qui est là. Ce qui correspond
//! est à Ash, et part ; ce qui ne correspond pas reste. C'est ce qui permet de rendre le
//! fichier de l'utilisateur à l'octet près, y compris quand Ash avait dû créer la clé
//! `hooks` elle-même, ou une clé d'événement dans la sienne.
//!
//! ### 3. Rien n'est deviné
//!
//! Un chemin occupé par autre chose qu'un objet ou un tableau, un fichier qui n'est pas un
//! objet JSON : Ash refuse, et le dit. C'est le seul refus qui reste.

use std::ops::Range;

use super::document::{AshText, Document, Edit, Ours};
use super::json;
use crate::features::agents::{Instrumentation, HOOK_MARK};

/// Deux espaces par niveau — l'indentation du texte qu'Ash ajoute.
///
/// Fixe, et pas déduite du fichier : c'est ce qui rend le retrait exact, puisque la même
/// règle recompose plus tard les mêmes octets. Elle ne s'applique qu'au texte d'Ash ; celui
/// de l'utilisateur n'est jamais reformaté.
const STEP: usize = 2;

/// Ce que le fichier porte des entrées d'Ash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Aucune entrée d'Ash.
    Absent,
    /// Des entrées écrites par un Ash plus ancien. Se réécrivent sans rien demander.
    Older { version: u32 },
    /// Des entrées de la version courante, mais pas celles qu'Ash écrirait : une main est
    /// passée, ou l'une d'elles a été retirée.
    Changed,
}

/// Ce qu'il y aurait à faire dans ce fichier.
#[derive(Debug)]
pub enum Plan {
    /// Le fichier porte déjà exactement ce qu'Ash écrirait. **Rien à toucher.**
    Current { version: u32 },
    /// Ce qu'Ash écrirait, et ce que le fichier portait déjà.
    Write {
        document: Document,
        standing: Standing,
        /// Les hooks du fichier qui ne sont pas d'Ash. Zéro se lit « absent », le reste se
        /// lit « conflit » — et c'est l'écran qui le dit, pas ce module.
        others: usize,
    },
    /// Ash ne saurait pas où écrire, et ne devine pas.
    Unusable,
}

/// Ce que le fichier porte, pour une entrée de l'instrumentation.
#[derive(Debug)]
enum Sighting {
    /// Le tableau visé existe.
    Array {
        /// Juste après le `[`.
        anchor: usize,
        /// Les éléments qui portent le marqueur d'Ash.
        ours: Vec<Range<usize>>,
    },
    /// La chaîne de clés s'arrête avant le bout : il faudrait créer ce qui reste du
    /// chemin. Le niveau où elle s'arrête se relit dans [`Walk::anchors`].
    Missing {
        /// Juste après le `{` du dernier conteneur trouvé.
        anchor: usize,
    },
    /// Une clé du chemin est occupée par autre chose qu'un conteneur.
    Blocked,
}

/// Ce qu'on a vu en descendant un chemin, y compris les conteneurs traversés.
#[derive(Debug)]
struct Walk {
    /// Le point d'insertion de chaque conteneur traversé, du plus haut au plus bas.
    anchors: Vec<usize>,
    sighting: Sighting,
}

/// Ce qu'il y aurait à écrire dans ce fichier — **pure**, elle ne touche à rien.
pub fn plan(content: &str, instrumentation: &Instrumentation) -> Plan {
    let Some(root) = json::root_object(content) else {
        return Plan::Unusable;
    };
    let walks: Vec<Walk> = instrumentation
        .entries
        .iter()
        .map(|entry| walk(content, &root, &entry.path))
        .collect();
    if walks
        .iter()
        .any(|seen| matches!(seen.sighting, Sighting::Blocked))
    {
        return Plan::Unusable;
    }

    let mut edits = Vec::new();
    let mut creations: Vec<(usize, usize)> = Vec::new(); // (anchor, index d'entrée)
    let mut found_versions = Vec::new();

    for (index, (entry, seen)) in instrumentation.entries.iter().zip(&walks).enumerate() {
        match &seen.sighting {
            Sighting::Blocked => return Plan::Unusable,
            Sighting::Missing { anchor, .. } => creations.push((*anchor, index)),
            Sighting::Array { anchor, ours } => {
                for (rank, span) in ours.iter().enumerate() {
                    let text = content.get(span.clone()).unwrap_or_default();
                    found_versions.push(version_in(text).unwrap_or(0));
                    if rank > 0 {
                        // Deux entrées d'Ash dans le même tableau ne sortent pas de la
                        // fusion : la seconde vient d'un copier-coller. On garde la
                        // première, à sa place, et on retire les autres.
                        if let Some(ours) = Ours::covering(content, with_separator(content, span)) {
                            edits.push(Edit::Remove(ours));
                        }
                    }
                }
                match ours.first() {
                    Some(span) if content.get(span.clone()) == Some(entry.item.as_str()) => {}
                    Some(span) => {
                        if let (Some(ours), Some(text)) = (
                            Ours::covering(content, span.clone()),
                            AshText::new(entry.item.clone()),
                        ) {
                            edits.push(Edit::Rewrite(ours, text));
                        }
                    }
                    None => {
                        if let Some(text) = AshText::new(format!(
                            "\n{}{}{}",
                            indent(entry.path.len() + 1),
                            entry.item,
                            comma(content, *anchor),
                        )) {
                            edits.push(Edit::Add(*anchor, text));
                        }
                    }
                }
            }
        }
    }

    edits.extend(creation_edits(content, instrumentation, &walks, &creations));

    let others = others_in(content, &root, instrumentation);
    if edits.is_empty() {
        return match found_versions.iter().max() {
            Some(version) => Plan::Current { version: *version },
            // Aucune entrée, aucune écriture : le fichier ne demande rien, ce qui ne peut
            // arriver que pour une instrumentation vide.
            None => Plan::Write {
                document: Document::edited(content, Vec::new()),
                standing: Standing::Absent,
                others,
            },
        };
    }

    let standing = match found_versions.iter().min() {
        None => Standing::Absent,
        Some(oldest) if *oldest != instrumentation.version => Standing::Older { version: *oldest },
        // Des entrées de la version courante, et pourtant il y a à écrire : quelqu'un les a
        // modifiées, ou en a retiré une.
        Some(_) => Standing::Changed,
    };

    Plan::Write {
        document: Document::edited(content, edits),
        standing,
        others,
    }
}

/// Le fichier qu'Ash écrit quand il n'y en avait pas.
pub fn fresh(instrumentation: &Instrumentation) -> Option<Document> {
    let pieces = created(instrumentation, &all_indexes(instrumentation), 0)?;
    let body: String = pieces
        .iter()
        .enumerate()
        .map(|(rank, piece)| {
            let last = rank + 1 == pieces.len();
            format!("\n{}{piece}{}", indent(1), if last { "" } else { "," })
        })
        .collect();
    AshText::new(body).map(|text| Document::fresh(&text))
}

/// Le fichier sans rien d'Ash, ou `None` s'il n'y avait rien à lui.
///
/// **C'est l'inverse exact de la fusion**, et il ne retire que ce qu'il sait réécrire :
/// chaque candidat est recomposé à partir de ce que le fichier porte, puis comparé octet par
/// octet. Ce qui ne correspond pas reste — un fichier que l'utilisateur a réindenté garde
/// alors les entrées d'Ash plutôt que de perdre les siennes.
pub fn removal(content: &str, instrumentation: &Instrumentation) -> Option<Document> {
    let root = json::root_object(content)?;
    let walks: Vec<Walk> = instrumentation
        .entries
        .iter()
        .map(|entry| walk(content, &root, &entry.path))
        .collect();

    // Ce que le fichier porte de nous, entrée par entrée : c'est avec ce texte-là qu'on
    // recompose, et pas avec celui qu'Ash écrirait aujourd'hui — une entrée d'une version
    // antérieure se retire aussi.
    let held: Vec<Option<&str>> = walks
        .iter()
        .map(|seen| match &seen.sighting {
            Sighting::Array { ours, .. } => ours
                .first()
                .and_then(|span| content.get(span.clone()))
                .filter(|item| item.contains(HOOK_MARK)),
            _ => None,
        })
        .collect();

    if held.iter().all(Option::is_none) {
        return None;
    }

    let mut edits = Vec::new();
    let mut settled = vec![false; instrumentation.entries.len()];

    // D'abord les conteneurs qu'Ash a créés lui-même, du plus haut au plus bas : les
    // retirer emporte tout ce qu'ils portent, et évite de laisser une clé `hooks` vide.
    let depth = instrumentation
        .entries
        .iter()
        .map(|entry| entry.path.len())
        .max()
        .unwrap_or(0);
    for level in 0..depth {
        for anchor in anchors_at(&walks, level, &settled, &held) {
            let candidates: Vec<(Vec<usize>, String)> =
                creations_at(instrumentation, &walks, &held, &settled, level, anchor);
            if let Some((span, taken)) = greedy(content, anchor, level + 1, &candidates) {
                if let Some(ours) = Ours::covering(content, span) {
                    edits.push(Edit::Remove(ours));
                    for index in taken {
                        settled[index] = true;
                    }
                }
            }
        }
    }

    // Puis les entrées posées dans un tableau qui, lui, appartient à l'utilisateur.
    for (index, entry) in instrumentation.entries.iter().enumerate() {
        if settled[index] {
            continue;
        }
        let (Some(item), Sighting::Array { anchor, ours }) = (held[index], &walks[index].sighting)
        else {
            continue;
        };
        let span = greedy(
            content,
            *anchor,
            entry.path.len() + 1,
            &[(vec![index], item.to_owned())],
        )
        .map(|(span, _)| span)
        .or_else(|| ours.first().map(|span| with_separator(content, span)));
        if let Some(ours) = span.and_then(|span| Ours::covering(content, span)) {
            edits.push(Edit::Remove(ours));
        }
    }

    Some(Document::edited(content, edits))
}

/// Les points d'insertion à ce niveau, sans doublon, pour les entrées qui restent.
fn anchors_at(walks: &[Walk], level: usize, settled: &[bool], held: &[Option<&str>]) -> Vec<usize> {
    let mut found: Vec<usize> = Vec::new();
    for (index, seen) in walks.iter().enumerate() {
        if settled[index] || held[index].is_none() {
            continue;
        }
        if let Some(anchor) = seen.anchors.get(level) {
            if !found.contains(anchor) {
                found.push(*anchor);
            }
        }
    }
    found
}

/// Les textes qu'Ash aurait écrits à ce point d'insertion, recomposés depuis le fichier.
fn creations_at(
    instrumentation: &Instrumentation,
    walks: &[Walk],
    held: &[Option<&str>],
    settled: &[bool],
    level: usize,
    anchor: usize,
) -> Vec<(Vec<usize>, String)> {
    let mut keys: Vec<&str> = Vec::new();
    let mut grouped: Vec<Vec<usize>> = Vec::new();

    for (index, entry) in instrumentation.entries.iter().enumerate() {
        if settled[index]
            || held[index].is_none()
            || walks[index].anchors.get(level) != Some(&anchor)
        {
            continue;
        }
        let Some(key) = entry.path.get(level) else {
            continue;
        };
        match keys.iter().position(|known| *known == key.as_str()) {
            Some(rank) => grouped[rank].push(index),
            None => {
                keys.push(key);
                grouped.push(vec![index]);
            }
        }
    }

    grouped
        .into_iter()
        .filter_map(|group| {
            let text = create(instrumentation, &group, level, &|index| {
                held[index].map(str::to_owned)
            })?;
            Some((group, text))
        })
        .collect()
}

/// Consomme, à partir du point d'insertion, tout ce qui correspond à un candidat.
///
/// Les candidats sont essayés dans l'ordre de l'instrumentation, et l'on saute ceux qui ne
/// correspondent pas : c'est exactement l'ordre dans lequel la fusion les avait écrits, et
/// elle n'écrit que ceux qui manquaient.
fn greedy(
    content: &str,
    anchor: usize,
    level: usize,
    candidates: &[(Vec<usize>, String)],
) -> Option<(Range<usize>, Vec<usize>)> {
    let prefix = format!("\n{}", indent(level));
    let mut at = anchor;
    let mut taken = Vec::new();
    let mut remaining: Vec<&(Vec<usize>, String)> = candidates.iter().collect();

    loop {
        let rest = content.get(at..)?;
        let Some(matched) = remaining.iter().position(|(_, text)| {
            rest.strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.starts_with(text.as_str()))
        }) else {
            break;
        };
        let (indexes, text) = remaining.remove(matched);
        at += prefix.len() + text.len();
        if content.get(at..at + 1) == Some(",") {
            at += 1;
        }
        taken.extend(indexes.iter().copied());
    }

    (at > anchor).then_some((anchor..at, taken))
}

/// Ce qu'Ash insère quand la clé n'existe pas — la sous-arborescence entière.
fn creation_edits(
    content: &str,
    instrumentation: &Instrumentation,
    walks: &[Walk],
    creations: &[(usize, usize)],
) -> Vec<Edit> {
    let mut anchors: Vec<usize> = Vec::new();
    for (anchor, _) in creations {
        if !anchors.contains(anchor) {
            anchors.push(*anchor);
        }
    }

    anchors
        .into_iter()
        .filter_map(|anchor| {
            let level = creations
                .iter()
                .find(|(at, _)| *at == anchor)
                .and_then(|(_, index)| walks[*index].anchors.len().checked_sub(1))?;
            let mine: Vec<usize> = creations
                .iter()
                .filter(|(at, _)| *at == anchor)
                .map(|(_, index)| *index)
                .collect();
            let mut keys: Vec<&str> = Vec::new();
            let mut grouped: Vec<Vec<usize>> = Vec::new();
            for index in mine {
                let key = instrumentation.entries[index].path.get(level)?;
                match keys.iter().position(|known| *known == key.as_str()) {
                    Some(rank) => grouped[rank].push(index),
                    None => {
                        keys.push(key);
                        grouped.push(vec![index]);
                    }
                }
            }

            let pieces: Vec<String> = grouped
                .iter()
                .filter_map(|group| {
                    create(instrumentation, group, level, &|index| {
                        Some(instrumentation.entries[index].item.clone())
                    })
                })
                .collect();
            let followed = json::holds_something(content, anchor);
            let mut text = String::new();
            for (rank, piece) in pieces.iter().enumerate() {
                let last = rank + 1 == pieces.len();
                text.push_str(&format!(
                    "\n{}{piece}{}",
                    indent(level + 1),
                    if last && !followed { "" } else { "," }
                ));
            }
            AshText::new(text).map(|text| Edit::Add(anchor, text))
        })
        .collect()
}

/// Le texte d'une clé qu'Ash crée, avec tout ce qu'elle porte.
///
/// `item` dit, pour chaque entrée, quel texte poser : celui de l'instrumentation quand on
/// fusionne, celui que le fichier porte quand on recompose pour retirer.
fn create(
    instrumentation: &Instrumentation,
    group: &[usize],
    level: usize,
    item: &dyn Fn(usize) -> Option<String>,
) -> Option<String> {
    let key = instrumentation
        .entries
        .get(*group.first()?)?
        .path
        .get(level)?;

    // Le bout du chemin : la clé porte le tableau, et le tableau porte l'entrée.
    if instrumentation.entries[group[0]].path.len() == level + 1 {
        return Some(format!(
            "\"{key}\": [\n{}{}\n{}]",
            indent(level + 2),
            item(group[0])?,
            indent(level + 1),
        ));
    }

    let mut keys: Vec<&str> = Vec::new();
    let mut grouped: Vec<Vec<usize>> = Vec::new();
    for index in group {
        let next = instrumentation.entries[*index].path.get(level + 1)?;
        match keys.iter().position(|known| *known == next.as_str()) {
            Some(rank) => grouped[rank].push(*index),
            None => {
                keys.push(next);
                grouped.push(vec![*index]);
            }
        }
    }
    let children: Vec<String> = grouped
        .iter()
        .filter_map(|child| create(instrumentation, child, level + 1, item))
        .collect();
    if children.len() != grouped.len() {
        return None;
    }

    let body = children
        .iter()
        .map(|child| format!("{}{child}", indent(level + 2)))
        .collect::<Vec<_>>()
        .join(",\n");
    Some(format!("\"{key}\": {{\n{body}\n{}}}", indent(level + 1)))
}

fn all_indexes(instrumentation: &Instrumentation) -> Vec<usize> {
    (0..instrumentation.entries.len()).collect()
}

/// Les textes de premier niveau d'un fichier qu'Ash écrit seul.
fn created(
    instrumentation: &Instrumentation,
    group: &[usize],
    level: usize,
) -> Option<Vec<String>> {
    let mut keys: Vec<&str> = Vec::new();
    let mut grouped: Vec<Vec<usize>> = Vec::new();
    for index in group {
        let key = instrumentation.entries.get(*index)?.path.get(level)?;
        match keys.iter().position(|known| *known == key.as_str()) {
            Some(rank) => grouped[rank].push(*index),
            None => {
                keys.push(key);
                grouped.push(vec![*index]);
            }
        }
    }
    grouped
        .iter()
        .map(|child| {
            create(instrumentation, child, level, &|index| {
                Some(instrumentation.entries[index].item.clone())
            })
        })
        .collect()
}

/// Descend le chemin dans le fichier, en retenant les conteneurs traversés.
fn walk(content: &str, root: &Range<usize>, path: &[String]) -> Walk {
    let mut anchors = vec![root.start + 1];
    let mut container = root.clone();

    for (index, key) in path.iter().enumerate() {
        let Some(entries) = json::entries(content, &container) else {
            return Walk {
                anchors,
                sighting: Sighting::Blocked,
            };
        };
        let Some(found) = entries.into_iter().find(|entry| &entry.key == key) else {
            let anchor = container.start + 1;
            return Walk {
                anchors,
                sighting: Sighting::Missing { anchor },
            };
        };

        if index + 1 == path.len() {
            let sighting = if json::is_array(content, &found.value) {
                let ours = json::items(content, &found.value)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|span| {
                        content
                            .get(span.clone())
                            .is_some_and(|item| item.contains(HOOK_MARK))
                    })
                    .collect();
                Sighting::Array {
                    anchor: found.value.start + 1,
                    ours,
                }
            } else {
                Sighting::Blocked
            };
            return Walk { anchors, sighting };
        }

        if !json::is_object(content, &found.value) {
            return Walk {
                anchors,
                sighting: Sighting::Blocked,
            };
        }
        container = found.value;
        anchors.push(container.start + 1);
    }

    Walk {
        anchors,
        sighting: Sighting::Blocked,
    }
}

/// Combien de hooks le fichier porte qui ne sont pas d'Ash.
///
/// On compte **tout** ce qui vit sous la première clé du chemin — `hooks` pour Claude Code —
/// et pas seulement sous les événements qu'Ash instrumente : un `SessionStart` que
/// l'utilisateur a posé lui-même est une chose qu'il n'a pas mise là par hasard, et l'écran
/// doit la nommer avant qu'Ash n'écrive à côté.
fn others_in(content: &str, root: &Range<usize>, instrumentation: &Instrumentation) -> usize {
    let mut roots: Vec<&str> = Vec::new();
    for entry in &instrumentation.entries {
        if let Some(key) = entry.path.first() {
            if !roots.contains(&key.as_str()) {
                roots.push(key);
            }
        }
    }

    let Some(entries) = json::entries(content, root) else {
        return 0;
    };
    entries
        .iter()
        .filter(|entry| roots.contains(&entry.key.as_str()))
        .map(|entry| count_theirs(content, &entry.value))
        .sum()
}

fn count_theirs(content: &str, span: &Range<usize>) -> usize {
    if json::is_array(content, span) {
        return json::items(content, span)
            .unwrap_or_default()
            .iter()
            .filter(|item| {
                !content
                    .get((*item).clone())
                    .is_some_and(|text| text.contains(HOOK_MARK))
            })
            .count();
    }
    if json::is_object(content, span) {
        return json::entries(content, span)
            .unwrap_or_default()
            .iter()
            .map(|entry| count_theirs(content, &entry.value))
            .sum();
    }
    0
}

/// La plage d'un élément **et** de son séparateur — le repli quand la recomposition échoue.
fn with_separator(content: &str, item: &Range<usize>) -> Range<usize> {
    let bytes = content.as_bytes();
    let mut after = item.end;
    while matches!(bytes.get(after), Some(byte) if byte.is_ascii_whitespace()) {
        after += 1;
    }
    if bytes.get(after) == Some(&b',') {
        return item.start..after + 1;
    }

    let mut before = item.start;
    while before > 0 && bytes.get(before - 1).is_some_and(u8::is_ascii_whitespace) {
        before -= 1;
    }
    if before > 0 && bytes.get(before - 1) == Some(&b',') {
        return before - 1..item.end;
    }
    item.clone()
}

fn comma(content: &str, anchor: usize) -> &'static str {
    if json::holds_something(content, anchor) {
        ","
    } else {
        ""
    }
}

fn indent(level: usize) -> String {
    " ".repeat(level * STEP)
}

/// La version inscrite dans le marqueur d'une entrée.
fn version_in(item: &str) -> Option<u32> {
    let after = item.split_once(HOOK_MARK)?.1;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Le chemin et l'entrée d'une instrumentation, pour les tests des autres modules.
#[cfg(test)]
pub fn entry(path: &[&str], item: &str) -> crate::features::agents::HookEntry {
    crate::features::agents::HookEntry {
        path: path.iter().map(|key| (*key).to_owned()).collect(),
        item: item.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::features::agents::{hook_mark, HookEntry};

    /// Test Data Builder : ce qu'un outil veut voir écrit, réduit à ce que le scénario dit.
    struct InstrumentationBuilder {
        entries: Vec<HookEntry>,
        version: u32,
    }

    impl InstrumentationBuilder {
        fn new() -> Self {
            Self {
                entries: vec![entry(
                    &["hooks", "Stop"],
                    &format!(
                        "{{\"hooks\":[{{\"command\":\"ash waiting {}\"}}]}}",
                        hook_mark(1)
                    ),
                )],
                version: 1,
            }
        }

        fn carrying(mut self, entries: Vec<HookEntry>) -> Self {
            self.entries = entries;
            self
        }

        fn version(mut self, version: u32) -> Self {
            self.version = version;
            self
        }

        fn build(self) -> Instrumentation {
            Instrumentation {
                file: PathBuf::from("/home/someone/.claude/settings.json"),
                entries: self.entries,
                version: self.version,
            }
        }
    }

    fn written(content: &str, instrumentation: &Instrumentation) -> String {
        match plan(content, instrumentation) {
            Plan::Write { document, .. } => document.as_str().to_owned(),
            other => panic!("il y avait à écrire : {other:?}"),
        }
    }

    #[test]
    fn given_a_settings_file_that_already_carries_a_hook_of_its_own_when_ash_merges_then_that_hook_is_untouched_and_ash_sits_beside_it(
    ) {
        // Given — le fichier réel de l'utilisateur qui a signalé le défaut : un `PreToolUse`
        // posé par un autre outil. C'est ce fichier-là qui rendait la fonction centrale
        // d'Ash inatteignable, et « déplace-les toi-même » était la seule issue proposée
        let theirs = "{\n  \"hooks\": { \"PreToolUse\": [ { \"matcher\": \"Bash\",\n    \"hooks\": [ { \"type\": \"command\", \"command\": \"rtk hook claude\", \"timeout\": 5 } ] } ] }\n}\n";
        let instrumentation = InstrumentationBuilder::new()
            .carrying(vec![
                entry(&["hooks", "PreToolUse"], &item("working")),
                entry(&["hooks", "Stop"], &item("waiting")),
            ])
            .build();

        // When
        let merged = written(theirs, &instrumentation);

        // Then — le hook de l'utilisateur est là, au caractère près, et les deux d'Ash aussi
        assert!(
            merged.contains(
                "{ \"type\": \"command\", \"command\": \"rtk hook claude\", \"timeout\": 5 }"
            ),
            "le hook de l'utilisateur a bougé :\n{merged}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&merged)
            .unwrap_or_else(|why| panic!("le fichier n'est plus du JSON ({why}) :\n{merged}"));
        assert_eq!(
            parsed["hooks"]["PreToolUse"]
                .as_array()
                .map(Vec::len)
                .unwrap_or_default(),
            2,
            "les deux cohabitent dans le même tableau :\n{merged}"
        );
        assert!(parsed["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap_or_default()
            .contains("ash waiting"));
    }

    #[test]
    fn given_a_file_ash_merged_into_when_its_hooks_are_removed_then_the_file_is_back_to_the_byte() {
        // Given — le geste inverse, sur le même fichier. C'est la promesse la plus lourde du
        // projet : le fichier appartient à l'utilisateur, et « rien hors de ce qui est à
        // Ash » veut dire *pas un octet*
        let theirs = "{\n  \"hooks\": { \"PreToolUse\": [ { \"matcher\": \"Bash\",\n    \"hooks\": [ { \"type\": \"command\", \"command\": \"rtk hook claude\", \"timeout\": 5 } ] } ] }\n}\n";
        let instrumentation = InstrumentationBuilder::new()
            .carrying(vec![
                entry(&["hooks", "PreToolUse"], &item("working")),
                entry(&["hooks", "Stop"], &item("waiting")),
                entry(&["hooks", "SessionEnd"], &item("done")),
            ])
            .build();
        let merged = written(theirs, &instrumentation);

        // When
        let removed = removal(&merged, &instrumentation).expect("Ash y avait écrit");

        // Then
        assert_eq!(removed.as_str(), theirs);
    }

    #[test]
    fn given_a_settings_file_without_any_hooks_when_ash_merges_and_leaves_then_the_file_is_back_to_the_byte(
    ) {
        // Given — l'autre moitié du même aller-retour : la clé `hooks` n'existe pas, Ash la
        // crée entièrement. La retirer doit emporter la clé elle-même, sans laisser
        // l'orpheline `"hooks": {}` derrière
        let theirs = "{\n    \"model\": \"opus\",\n    \"env\": {\"FOO\": \"bar\"}\n}\n";
        let instrumentation = InstrumentationBuilder::new()
            .carrying(vec![
                entry(&["hooks", "Stop"], &item("waiting")),
                entry(&["hooks", "SessionEnd"], &item("done")),
            ])
            .build();

        // When
        let merged = written(theirs, &instrumentation);
        let removed = removal(&merged, &instrumentation).expect("Ash y avait écrit");

        // Then
        assert!(
            serde_json::from_str::<serde_json::Value>(&merged).is_ok(),
            "le fichier fusionné doit rester du JSON :\n{merged}"
        );
        assert!(merged.contains("    \"model\": \"opus\","), "{merged}");
        assert_eq!(removed.as_str(), theirs);
    }

    #[test]
    fn given_the_entries_already_in_place_when_the_file_is_planned_again_then_nothing_is_to_be_written(
    ) {
        // Given — Ash démarre plusieurs fois par jour, et l'écran de réglages relit le
        // fichier à chaque affichage. Réécrire un fichier identique réveillerait les
        // surveillances de l'utilisateur et ferait grossir un diff dans ses dotfiles
        let theirs = "{\n  \"model\": \"opus\"\n}\n";
        let instrumentation = InstrumentationBuilder::new().build();
        let merged = written(theirs, &instrumentation);

        // When
        let again = plan(&merged, &instrumentation);

        // Then
        assert!(
            matches!(again, Plan::Current { version: 1 }),
            "{again:?}\n{merged}"
        );
    }

    #[test]
    fn given_an_entry_of_ash_that_someone_edited_when_the_file_is_planned_then_it_is_read_as_a_hand_edit_and_not_as_an_older_version(
    ) {
        // Given — les deux situations ne demandent pas la même conduite : une entrée périmée
        // se réécrit sans rien demander, une entrée éditée se signale et se montre. Les
        // confondre écrase le travail de l'utilisateur, ou bloque toute mise à jour
        let theirs = "{\n  \"model\": \"opus\"\n}\n";
        let instrumentation = InstrumentationBuilder::new().build();
        let merged = written(theirs, &instrumentation).replace("ash waiting", "mon-script");

        // When
        let edited = plan(&merged, &instrumentation);

        // Then
        let Plan::Write { standing, .. } = edited else {
            panic!("une entrée modifiée demande une écriture");
        };
        assert_eq!(standing, Standing::Changed);
    }

    #[test]
    fn given_entries_written_by_an_older_ash_when_the_file_is_planned_then_it_names_the_version_in_place(
    ) {
        // Given — le marqueur porte la version, et c'est tout ce qui sépare « périmé »
        // d'« édité ». Sans lui, une simple évolution du bloc bloquerait la mise à jour de
        // tout le monde
        let theirs = "{\n  \"model\": \"opus\"\n}\n";
        let older = InstrumentationBuilder::new().build();
        let merged = written(theirs, &older);
        let newer = InstrumentationBuilder::new()
            .carrying(vec![entry(
                &["hooks", "Stop"],
                &format!(
                    "{{\"hooks\":[{{\"command\":\"ash waiting --neuf {}\"}}]}}",
                    hook_mark(2)
                ),
            )])
            .version(2)
            .build();

        // When
        let planned = plan(&merged, &newer);

        // Then
        let Plan::Write {
            standing, document, ..
        } = planned
        else {
            panic!("un bloc périmé se réécrit");
        };
        assert_eq!(standing, Standing::Older { version: 1 });
        assert!(
            document.as_str().contains("--neuf"),
            "{}",
            document.as_str()
        );
        assert!(
            !document.as_str().contains(&hook_mark(1)),
            "l'ancienne entrée a disparu :\n{}",
            document.as_str()
        );
    }

    #[test]
    fn given_a_file_whose_hooks_key_is_not_an_object_when_it_is_planned_then_ash_refuses_rather_than_guess(
    ) {
        // Given — `hooks` occupé par une chaîne, un `settings.json` remplacé par une liste :
        // deviner où écrire produirait un fichier que l'outil ne lit plus. Ça reste un refus
        let refused = [
            "{\n  \"hooks\": \"le mien\"\n}\n",
            "[1, 2, 3]\n",
            "des notes",
        ];
        let instrumentation = InstrumentationBuilder::new().build();

        // When
        let planned: Vec<Plan> = refused
            .iter()
            .map(|content| plan(content, &instrumentation))
            .collect();

        // Then
        assert!(
            planned.iter().all(|seen| matches!(seen, Plan::Unusable)),
            "{planned:?}"
        );
    }

    #[test]
    fn given_a_file_that_carries_hooks_of_its_own_when_it_is_planned_then_it_says_how_many_are_not_ash(
    ) {
        // Given — c'est ce chiffre qui fait la différence entre « il n'y a rien » et « il y
        // a quelque chose que je n'ai pas mis ». L'écran en tire un conflit, et le montre
        // avant d'écrire
        let theirs = "{\n  \"hooks\": {\n    \"SessionStart\": [{\"hooks\": []}],\n    \"PreToolUse\": [{\"a\": 1}, {\"b\": 2}]\n  }\n}\n";
        let instrumentation = InstrumentationBuilder::new().build();

        // When
        let planned = plan(theirs, &instrumentation);

        // Then
        let Plan::Write {
            others, standing, ..
        } = planned
        else {
            panic!("il y a à écrire : Ash n'est pas dans ce fichier");
        };
        assert_eq!(others, 3);
        assert_eq!(standing, Standing::Absent);
    }

    fn item(word: &str) -> String {
        format!(
            "{{\"hooks\":[{{\"command\":\"ash {word} {}\"}}]}}",
            hook_mark(1)
        )
    }
}
