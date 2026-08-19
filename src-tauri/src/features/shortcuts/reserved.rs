//! Les combinaisons qu'Ash ne recevra pas, ou pas toujours — et ce qu'il en dit.
//!
//! **Aucune n'est interdite.** C'est la règle de la planche, écrite en toutes lettres :
//! « une combinaison prise par macOS ou avalée par le terminal n'est pas interdite — elle
//! est annoncée comme inefficace, au moment de la capture ». Interdire demanderait à Ash de
//! savoir mieux que l'utilisateur ce que sa machine fait de ses touches, alors qu'un panneau
//! des Réglages Système peut libérer `⌘⌃D` pendant qu'Ash tourne. Annoncer ne coûte rien et
//! ne ferme rien.
//!
//! La table est **embarquée**, et c'est un choix : macOS n'expose aucune API pour demander
//! « qui a cette combinaison ». Les quatre premières viennent de la spec §4.4 et de la
//! planche `3j`, la cinquième du terminal lui-même.

use super::combination::Combination;

/// Qui prend la combinaison avant Ash.
///
/// Les deux cas ne se valent pas, et c'est pourquoi ils ne sont pas un seul booléen :
/// macOS prend la touche **toujours**, le terminal seulement quand c'est lui qu'on tape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum ReservedBy {
    Macos,
    Terminal,
}

/// Ce qu'Ash annonce d'une combinaison réservée — la phrase de la planche `3j`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Reservation {
    pub by: ReservedBy,
    /// La phrase, telle qu'elle s'affiche — `is reserved by macOS (force quit) — ash will
    /// never receive it`. Elle est ici et non dans la fenêtre de réglages parce que la
    /// raison est propre à **cette** combinaison : « force quit » n'est pas « emoji picker ».
    pub note: String,
}

/// La table : la combinaison, qui la prend, et ce qu'on en dit.
///
/// `⌃⇥` n'y est **pas**, et c'est délibéré : `Tab` seul complète dans `zsh`, mais
/// `Ctrl+Tab` porte le drapeau `Control` et n'est donc ni retenu par le shell ni confondu
/// avec une touche nue — Ash le reçoit vraiment, par `src/app/shortcuts.ts` (spec §4.4, et
/// l'en-tête de `src-tauri/src/menu.rs`). L'annoncer comme inefficace serait un mensonge sur
/// un raccourci qui marche.
const RESERVED: &[(&str, ReservedBy, &str)] = &[
    (
        "Cmd+Ctrl+KeyF",
        ReservedBy::Macos,
        "is reserved by macOS (full screen) — ash will never receive it",
    ),
    (
        "Cmd+Ctrl+KeyD",
        ReservedBy::Macos,
        "is reserved by macOS (look up) — ash will never receive it",
    ),
    (
        "Cmd+Ctrl+Space",
        ReservedBy::Macos,
        "is reserved by macOS (emoji picker) — ash will never receive it",
    ),
    (
        "Cmd+Alt+Escape",
        ReservedBy::Macos,
        "is reserved by macOS (force quit) — ash will never receive it",
    ),
    (
        "Cmd+KeyK",
        ReservedBy::Terminal,
        "swallowed by the terminal — never reaches ash",
    ),
];

/// Ce que la table dit de cette combinaison, ou `None` si personne ne la prend.
///
/// La comparaison porte sur la **valeur**, jamais sur son écriture : `Cmd+Ctrl+KeyF` et une
/// capture de `⌘⌃F` sont la même combinaison, et une table de chaînes aurait laissé passer
/// la seconde.
pub fn reservation(combination: &Combination) -> Option<Reservation> {
    RESERVED
        .iter()
        .filter_map(|(accelerator, by, note)| {
            Combination::parse(accelerator)
                .ok()
                .map(|reserved| (reserved, *by, *note))
        })
        .find(|(reserved, _, _)| reserved == combination)
        .map(|(_, by, note)| Reservation {
            by,
            note: note.to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_combination_macos_takes_when_it_is_captured_then_it_is_announced_as_ineffective_and_still_posable(
    ) {
        // Given — les trois de la spec §4.4, plus le `⌘⌥⎋` de la planche
        let taken = [
            "Cmd+Ctrl+KeyF",
            "Cmd+Ctrl+KeyD",
            "Cmd+Ctrl+Space",
            "Cmd+Alt+Escape",
        ];

        // When
        let announced: Vec<Option<ReservedBy>> = taken
            .iter()
            .map(|accelerator| {
                reservation(&Combination::parse(accelerator).unwrap()).map(|found| found.by)
            })
            .collect();

        // Then — une réservation est un **avertissement** : rien ici ne rend une
        // combinaison impossible à poser, et c'est la règle de la planche
        assert_eq!(announced, vec![Some(ReservedBy::Macos); taken.len()]);
    }

    #[test]
    fn given_the_combination_the_terminal_intercepts_when_it_is_read_then_it_is_told_apart_from_the_ones_macos_takes(
    ) {
        // Given / When — `⌘K` : le terminal a ses propres raccourcis et les intercepte
        // avant Ash quand on est dans le shell. Ce n'est pas la même chose qu'une touche
        // que macOS prend toujours, donc ça ne se dit pas de la même façon
        let swallowed = reservation(&Combination::parse("Cmd+KeyK").unwrap()).unwrap();

        // Then
        assert_eq!(swallowed.by, ReservedBy::Terminal);
        assert!(swallowed.note.contains("swallowed by the terminal"));
    }

    #[test]
    fn given_a_combination_nobody_takes_when_it_is_read_then_nothing_is_announced() {
        // Given / When — sans ça, le bloc de capture porterait un filet et un
        // avertissement vides à chaque frappe
        let free = reservation(&Combination::parse("Cmd+Ctrl+KeyJ").unwrap());

        // Then
        assert_eq!(free, None);
    }

    #[test]
    fn given_a_reserved_combination_written_another_way_when_it_is_looked_up_then_it_is_still_found(
    ) {
        // Given — la table écrit `Cmd+Ctrl+KeyF` ; une capture rend le même code, mais un
        // défaut du menu s'écrirait `Ctrl+Cmd+F`. Comparer des chaînes aurait laissé passer
        // la seconde, et l'avertissement ne serait sorti que pour une écriture sur deux
        let same = Combination::parse("Ctrl+Cmd+F").unwrap();

        // When / Then
        assert_eq!(
            reservation(&same).map(|found| found.by),
            Some(ReservedBy::Macos)
        );
    }
}
