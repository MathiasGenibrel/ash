//! Une combinaison de touches — la **valeur** que toute cette feature manipule.
//!
//! Elle a trois écritures, et une seule d'entre elles est la vérité :
//!
//! | Écriture | Qui la lit | Exemple |
//! |---|---|---|
//! | canonique (ce type) | Ash | `⌘` + `⇧` + `KeyT` |
//! | accélérateur | l'analyseur de `muda`, donc le menu natif | `Cmd+Shift+KeyT` |
//! | glyphes | l'utilisateur, dans le menu et dans les réglages | `⇧⌘T` |
//!
//! Les deux dernières se **dérivent** de la première, et jamais l'inverse : c'est ce qui
//! permet de ne stocker qu'une liaison et d'en tirer aussi bien l'entrée de menu que la
//! ligne de l'écran.
//!
//! **Le nom de touche canonique est celui du W3C** — `KeyT`, `Digit1`, `Comma` —, et ce
//! choix n'est pas cosmétique : c'est exactement ce que `KeyboardEvent.code` rend dans la
//! webview **et** ce que `parse_code` de `muda` accepte. Une capture faite au clavier
//! traverse donc la frontière sans table de traduction intermédiaire, et la disposition du
//! clavier ne change rien à ce qui est retenu.

use std::fmt;

use super::error::ShortcutError;

/// Les quatre modificateurs de macOS, dans l'ordre où macOS les écrit.
///
/// Nommés `command` / `option` et non `super` / `alt` : c'est ainsi que la spec §4.4, les
/// planches et le clavier de la machine les appellent. La traduction vers les noms de
/// `muda` (`Cmd`, `Alt`) est faite une fois, dans [`Combination::accelerator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub control: bool,
    pub option: bool,
    pub shift: bool,
    pub command: bool,
}

impl Modifiers {
    /// Aucun modificateur « fort » : une combinaison qui n'en porte pas prendrait une
    /// touche nue au shell.
    ///
    /// `Shift` n'en est pas un — `⇧A` est un `A` majuscule, pas un raccourci.
    fn bare(self) -> bool {
        !self.command && !self.control && !self.option
    }
}

/// Ce que la webview a vu passer sous les doigts, avant qu'Ash n'en fasse une combinaison.
///
/// C'est un **fait**, pas une décision : la webview rapporte le code physique de la touche
/// et l'état des quatre modificateurs, et c'est le backend qui dit si ça fait un raccourci
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Sans ce partage, la
/// table des noms de touches existerait des deux côtés de la frontière.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct KeyStroke {
    /// `KeyboardEvent.code` — le code **physique**, indépendant de la disposition.
    pub code: String,
    pub command: bool,
    pub control: bool,
    pub option: bool,
    pub shift: bool,
}

impl KeyStroke {
    fn modifiers(&self) -> Modifiers {
        Modifiers {
            control: self.control,
            option: self.option,
            shift: self.shift,
            command: self.command,
        }
    }
}

/// Une touche, sous son nom canonique du W3C.
///
/// Le type existe pour qu'un nom non reconnu ne puisse pas entrer : une chaîne libre aurait
/// fini dans un accélérateur que `muda` refuse, et une entrée de menu sans accélérateur
/// est une panne muette — l'entrée s'affiche, la touche ne fait rien.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key(&'static str);

/// Les touches qu'une combinaison peut porter, et le glyphe sous lequel macOS les écrit.
///
/// La table est **fermée**, et c'est le point : un nom qui n'y est pas ne peut pas devenir
/// un accélérateur, donc pas devenir une entrée de menu muette. Les modificateurs seuls
/// (`ShiftLeft`, `MetaRight`…) n'y sont pas — presser `⌘` n'est pas une combinaison.
///
/// `Escape`, `Backspace` et `Enter` y sont, mais ne se **capturent** pas : voir
/// [`CAPTURE_ISSUES`]. La différence compte, parce que `⌘⌥⎋` est l'une des combinaisons
/// réservées que la planche demande d'annoncer, et qu'on ne peut annoncer que ce qu'on sait
/// écrire.
const KEYS: &[(&str, &str)] = &[
    ("KeyA", "A"),
    ("KeyB", "B"),
    ("KeyC", "C"),
    ("KeyD", "D"),
    ("KeyE", "E"),
    ("KeyF", "F"),
    ("KeyG", "G"),
    ("KeyH", "H"),
    ("KeyI", "I"),
    ("KeyJ", "J"),
    ("KeyK", "K"),
    ("KeyL", "L"),
    ("KeyM", "M"),
    ("KeyN", "N"),
    ("KeyO", "O"),
    ("KeyP", "P"),
    ("KeyQ", "Q"),
    ("KeyR", "R"),
    ("KeyS", "S"),
    ("KeyT", "T"),
    ("KeyU", "U"),
    ("KeyV", "V"),
    ("KeyW", "W"),
    ("KeyX", "X"),
    ("KeyY", "Y"),
    ("KeyZ", "Z"),
    ("Digit0", "0"),
    ("Digit1", "1"),
    ("Digit2", "2"),
    ("Digit3", "3"),
    ("Digit4", "4"),
    ("Digit5", "5"),
    ("Digit6", "6"),
    ("Digit7", "7"),
    ("Digit8", "8"),
    ("Digit9", "9"),
    ("Comma", ","),
    ("Period", "."),
    ("Slash", "/"),
    ("Backslash", "\\"),
    ("Quote", "'"),
    ("Semicolon", ";"),
    ("BracketLeft", "["),
    ("BracketRight", "]"),
    ("Backquote", "`"),
    ("Minus", "-"),
    ("Equal", "="),
    // Pas le pavé numérique : c'est le seul nom que l'analyseur de `muda` traduise en `+`
    // sur le clavier principal. La note est dans `menu.rs`, au-dessus des pas de police.
    ("NumpadAdd", "+"),
    ("Space", "␣"),
    ("Tab", "⇥"),
    ("ArrowUp", "↑"),
    ("ArrowDown", "↓"),
    ("ArrowLeft", "←"),
    ("ArrowRight", "→"),
    ("Home", "↖"),
    ("End", "↘"),
    ("PageUp", "⇞"),
    ("PageDown", "⇟"),
    ("Delete", "⌦"),
    ("Escape", "⎋"),
    ("Backspace", "⌫"),
    ("Enter", "⏎"),
    ("F1", "F1"),
    ("F2", "F2"),
    ("F3", "F3"),
    ("F4", "F4"),
    ("F5", "F5"),
    ("F6", "F6"),
    ("F7", "F7"),
    ("F8", "F8"),
    ("F9", "F9"),
    ("F10", "F10"),
    ("F11", "F11"),
    ("F12", "F12"),
];

impl Key {
    /// La touche que ce nom désigne, ou `None` si Ash ne sait pas la lier.
    ///
    /// Deux écritures sont acceptées, et se ramènent à la même touche : le nom du W3C
    /// (`KeyT`, `Digit1`), qui vient d'une capture, et son raccourci d'écriture (`T`, `1`),
    /// qui vient des défauts déclarés dans `menu.rs`. Sans la seconde, chaque défaut du menu
    /// aurait à s'écrire `Cmd+KeyT`, ce qui ne se relit pas.
    pub fn parse(name: &str) -> Option<Self> {
        let sought = name.to_ascii_uppercase();
        KEYS.iter()
            .find(|(canonical, _)| {
                canonical.eq_ignore_ascii_case(&sought)
                    || short_name(canonical).eq_ignore_ascii_case(&sought)
            })
            .map(|(canonical, _)| Key(canonical))
    }

    /// Le nom canonique — celui que `muda` lit, et celui qui est écrit sur le disque.
    pub fn name(&self) -> &'static str {
        self.0
    }

    /// La touche telle qu'un menu macOS la montre.
    fn glyph(&self) -> &'static str {
        KEYS.iter()
            .find(|(canonical, _)| *canonical == self.0)
            .map(|(_, glyph)| *glyph)
            // Inatteignable : un `Key` ne se construit que depuis la table.
            .unwrap_or(self.0)
    }
}

/// `KeyT` → `T`, `Digit1` → `1`, tout le reste inchangé.
fn short_name(canonical: &str) -> &str {
    canonical
        .strip_prefix("Key")
        .or_else(|| canonical.strip_prefix("Digit"))
        .unwrap_or(canonical)
}

/// Une combinaison liable : des modificateurs, et une touche.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Combination {
    modifiers: Modifiers,
    key: Key,
}

/// Les trois touches qui sont les **issues** du bloc de capture (planche `3j`) : `esc`
/// annule, `⌫` retire le raccourci, `⏎` confirme.
///
/// Les lier rendrait la capture impossible à quitter — on ne pourrait plus ni annuler ni
/// confirmer ce qu'on est en train de faire. Elles restent écrivables comme combinaison,
/// parce que `⌘⌥⎋` est une combinaison réservée qu'il faut savoir nommer pour l'annoncer.
const CAPTURE_ISSUES: &[&str] = &["Escape", "Backspace", "Enter"];

impl Combination {
    /// La combinaison qu'une frappe désigne, ou la raison pour laquelle elle n'en est pas une.
    ///
    /// Trois refus, et ils protègent le shell et la capture plutôt que l'écran : une touche
    /// qu'Ash ne sait pas lier, une des trois issues du bloc de capture, et une combinaison
    /// **sans modificateur fort** — `T` seul comme raccourci rendrait impossible d'écrire un
    /// `t` dans un terminal.
    pub fn from_stroke(stroke: &KeyStroke) -> Result<Self, ShortcutError> {
        let key = Key::parse(&stroke.code)
            .filter(|key| !CAPTURE_ISSUES.contains(&key.name()))
            .ok_or_else(|| ShortcutError::UnusableKey {
                code: stroke.code.clone(),
            })?;
        let modifiers = stroke.modifiers();
        if modifiers.bare() {
            return Err(ShortcutError::BareKey);
        }
        Ok(Self { modifiers, key })
    }

    /// La combinaison qu'un accélérateur écrit — `Cmd+Shift+T`, `Ctrl+Tab`.
    ///
    /// C'est par là qu'entrent les **défauts** déclarés dans `menu.rs` et les liaisons
    /// relues du disque. Elle refuse ce que [`from_stroke`](Self::from_stroke) refuse, plus
    /// un modificateur inconnu : un fichier édité à la main ne doit pas pouvoir poser une
    /// liaison que le menu ne saurait pas jouer.
    pub fn parse(accelerator: &str) -> Result<Self, ShortcutError> {
        let mut modifiers = Modifiers::default();
        let mut tokens: Vec<&str> = accelerator.split('+').collect();
        // Le dernier segment est la touche. `Cmd+NumpadAdd` se découpe donc bien, alors
        // qu'un `+` littéral en fin d'accélérateur n'aurait aucun nom.
        let Some(key_name) = tokens.pop() else {
            return Err(ShortcutError::UnusableKey {
                code: accelerator.to_owned(),
            });
        };
        for token in tokens {
            match token {
                "Cmd" | "Command" | "Super" | "Meta" => modifiers.command = true,
                "Ctrl" | "Control" => modifiers.control = true,
                "Alt" | "Option" => modifiers.option = true,
                "Shift" => modifiers.shift = true,
                other => {
                    return Err(ShortcutError::UnusableKey {
                        code: other.to_owned(),
                    })
                }
            }
        }

        let key = Key::parse(key_name).ok_or_else(|| ShortcutError::UnusableKey {
            code: key_name.to_owned(),
        })?;
        if modifiers.bare() {
            return Err(ShortcutError::BareKey);
        }
        Ok(Self { modifiers, key })
    }

    /// L'accélérateur au format de `muda` — ce que l'entrée de menu reçoit.
    ///
    /// L'ordre des modificateurs est celui de l'analyseur, qui les veut tous avant la
    /// touche ; l'ordre **d'affichage**, lui, est celui de macOS et il est dans
    /// [`glyphs`](Self::glyphs).
    pub fn accelerator(&self) -> String {
        let mut written = String::new();
        for (present, name) in [
            (self.modifiers.control, "Ctrl"),
            (self.modifiers.option, "Alt"),
            (self.modifiers.shift, "Shift"),
            (self.modifiers.command, "Cmd"),
        ] {
            if present {
                written.push_str(name);
                written.push('+');
            }
        }
        written.push_str(self.key.name());
        written
    }

    /// La combinaison telle que macOS l'écrit — `⇧⌘T`, `⌃⇥`, `⌘+`.
    ///
    /// L'ordre `⌃⌥⇧⌘` est celui de macOS, et non celui dans lequel l'accélérateur déclare
    /// ses modificateurs : `Cmd+Shift+T` s'écrit `⇧⌘T`. C'est ici, et pas dans la fenêtre
    /// de réglages, parce qu'une table de traduction en TypeScript en serait la seconde
    /// copie — et c'est l'écran qu'on croit quand les deux ne disent pas la même chose.
    pub fn glyphs(&self) -> String {
        let mut written = String::new();
        for (present, glyph) in [
            (self.modifiers.control, "⌃"),
            (self.modifiers.option, "⌥"),
            (self.modifiers.shift, "⇧"),
            (self.modifiers.command, "⌘"),
        ] {
            if present {
                written.push_str(glyph);
            }
        }
        written.push_str(self.key.glyph());
        written
    }
}

impl fmt::Display for Combination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.glyphs())
    }
}

/// Sur le disque, une combinaison est son **accélérateur** — `"Cmd+Shift+KeyT"`.
///
/// Ni un objet à quatre booléens, ni des glyphes : l'accélérateur est ce que `muda` relit,
/// et il reste lisible à l'œil nu dans `~/.ash/shortcuts.json`.
impl serde::Serialize for Combination {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.accelerator())
    }
}

impl<'de> serde::Deserialize<'de> for Combination {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let written = String::deserialize(deserializer)?;
        Combination::parse(&written).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une frappe, telle que la webview la rapporte. Par défaut : `⌘T`.
    pub struct StrokeBuilder(KeyStroke);

    impl StrokeBuilder {
        pub fn new() -> Self {
            Self(KeyStroke {
                code: "KeyT".to_owned(),
                command: true,
                control: false,
                option: false,
                shift: false,
            })
        }

        pub fn code(mut self, code: &str) -> Self {
            self.0.code = code.to_owned();
            self
        }

        pub fn bare(mut self) -> Self {
            self.0.command = false;
            self.0.control = false;
            self.0.option = false;
            self
        }

        pub fn shift(mut self) -> Self {
            self.0.shift = true;
            self
        }

        pub fn build(self) -> KeyStroke {
            self.0
        }
    }

    #[test]
    fn given_a_combination_captured_at_the_keyboard_when_it_is_handed_to_the_menu_then_it_is_the_accelerator_muda_parses(
    ) {
        // Given — la webview rapporte le code physique du W3C, celui que `muda` accepte
        let stroke = StrokeBuilder::new().code("KeyT").shift().build();

        // When
        let combination = Combination::from_stroke(&stroke).unwrap();

        // Then — l'ordre est celui de l'analyseur, qui veut les modificateurs d'abord
        assert_eq!(combination.accelerator(), "Shift+Cmd+KeyT");
    }

    #[test]
    fn given_a_key_the_shell_needs_when_it_is_pressed_without_a_modifier_then_it_is_not_a_shortcut()
    {
        // Given — `T` seul : le lier rendrait impossible d'écrire un `t` dans un terminal.
        // `⇧` ne suffit pas non plus — `⇧A` est un `A` majuscule
        let bare = StrokeBuilder::new().bare().build();
        let shifted = StrokeBuilder::new().bare().shift().build();

        // When
        let refused = [
            Combination::from_stroke(&bare),
            Combination::from_stroke(&shifted),
        ];

        // Then
        assert_eq!(
            refused,
            [Err(ShortcutError::BareKey), Err(ShortcutError::BareKey)]
        );
    }

    #[test]
    fn given_the_three_keys_the_capture_itself_uses_when_they_are_pressed_then_none_of_them_can_be_bound(
    ) {
        // Given — `esc` annule, `⌫` retire le raccourci, `⏎` confirme (planche `3j`) : les
        // lier rendrait la capture impossible à quitter
        let issues = ["Escape", "Backspace", "Enter"];

        // When
        let refused: Vec<bool> = issues
            .iter()
            .map(|code| Combination::from_stroke(&StrokeBuilder::new().code(code).build()).is_err())
            .collect();

        // Then
        assert_eq!(refused, vec![true, true, true]);
    }

    #[test]
    fn given_the_accelerators_the_menu_declares_when_they_are_written_for_the_screen_then_they_read_as_macos_writes_them(
    ) {
        // Given — les deux pièges : l'ordre des modificateurs est celui de macOS et non
        // celui de la déclaration, et `NumpadAdd` n'est pas le pavé numérique mais le seul
        // nom que `muda` traduise en `+`
        let declared = [
            "Cmd+T",
            "Cmd+Shift+T",
            "Ctrl+Tab",
            "Ctrl+Shift+Tab",
            "Cmd+Comma",
            "Cmd+NumpadAdd",
            "Cmd+Minus",
            "Cmd+0",
        ];

        // When
        let written: Vec<String> = declared
            .iter()
            .map(|accelerator| Combination::parse(accelerator).unwrap().glyphs())
            .collect();

        // Then
        assert_eq!(written, ["⌘T", "⇧⌘T", "⌃⇥", "⌃⇧⇥", "⌘,", "⌘+", "⌘-", "⌘0"]);
    }

    #[test]
    fn given_a_default_written_the_short_way_and_the_same_one_captured_when_both_are_read_then_they_are_the_same_binding(
    ) {
        // Given — `menu.rs` déclare `Cmd+T`, une capture rend `KeyT` : si les deux ne se
        // ramenaient pas à la même valeur, chaque ligne s'annoncerait « modifiée » dès
        // qu'on lui redonne son défaut, et le compteur `n changed` mentirait
        let declared = Combination::parse("Cmd+T").unwrap();

        // When
        let captured =
            Combination::from_stroke(&StrokeBuilder::new().code("KeyT").build()).unwrap();

        // Then
        assert_eq!(declared, captured);
    }

    #[test]
    fn given_a_binding_written_to_disk_when_a_new_session_reads_it_back_then_it_is_the_same_combination(
    ) {
        // Given — le fichier est le seul lien entre deux sessions
        let chosen = Combination::parse("Cmd+Ctrl+KeyG").unwrap();

        // When
        let written = serde_json::to_string(&chosen).unwrap();
        let read: Combination = serde_json::from_str(&written).unwrap();

        // Then — et l'accélérateur reste lisible à l'œil nu dans `~/.ash/shortcuts.json`
        assert_eq!(written, "\"Ctrl+Cmd+KeyG\"");
        assert_eq!(read, chosen);
    }

    #[test]
    fn given_a_shortcuts_file_edited_by_hand_when_it_names_a_key_ash_cannot_bind_then_it_is_refused(
    ) {
        // Given — un accélérateur que `muda` ne saurait pas jouer : l'entrée de menu
        // s'afficherait, et la touche ne ferait rien
        let nonsense = ["Cmd+F13", "Hyper+KeyT", "Cmd+", ""];

        // When
        let read: Vec<bool> = nonsense
            .iter()
            .map(|written| Combination::parse(written).is_err())
            .collect();

        // Then
        assert_eq!(read, vec![true; nonsense.len()]);
    }
}
