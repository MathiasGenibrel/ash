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
//! **Une combinaison retient le caractère produit, jamais la position physique de la
//! touche** (issue #133). C'est ce que macOS apparie : `-[NSMenuItem setKeyEquivalent:]`
//! prend un **caractère**, et `performKeyEquivalent:` le compare à celui que la frappe
//! produit. Le nom canonique reste celui du W3C — `KeyT`, `Digit1`, `Comma` —, parce que
//! c'est ce que `parse_code` de `muda` lit, mais il n'est plus qu'une **écriture** : il
//! nomme le caractère `t`, pas la troisième touche de la rangée du haut. `code_to_key` de
//! `muda` 0.19.3 le confirme — `Code::KeyT` devient `Key::Character("t")`, et c'est ce
//! caractère qui part dans le `NSMenuItem`.
//!
//! La conduite qui en découle, et qu'il faut savoir plutôt que découvrir :
//!
//! > **Un raccourci suit le caractère, donc changer de disposition clavier peut le déplacer
//! > de touche.** `⌘W` reste `⌘W` : sur un clavier US il se frappe à la troisième position
//! > de la rangée du haut, sur un AZERTY à la première. C'est la convention de macOS —
//! > toutes ses applications se comportent ainsi — et non une bizarrerie d'Ash.
//!
//! Le choix précédent — retenir `KeyboardEvent.code`, la position — tenait parce qu'une
//! capture traversait la frontière sans table intermédiaire. Il était faux sur macOS : sur
//! un AZERTY, `⌘` + la touche marquée `W` posait `Cmd+KeyZ`, le menu perdait son
//! accélérateur, et la touche pressée continuait de jouer l'action d'à côté.
//!
//! **Ce que `parse_code` accepte réellement**, et qui borne donc ce qu'une capture peut
//! poser : les 26 lettres, les 10 chiffres, ``` ` \\ [ ] , = - . ' ; / ``` et les touches
//! nommées (`Tab`, `Escape`, les flèches, `F1`…`F12`). Un caractère hors de cette liste —
//! `é`, `&`, `ç`, `µ`, que la rangée du haut d'un AZERTY produit sans `⇧` — n'a **aucune**
//! écriture d'accélérateur : voir [`Combination::from_stroke`] pour ce qu'Ash en fait.

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
/// Ce sont deux **faits**, et aucune décision : le caractère que la frappe a produit, la
/// position physique de la touche, et l'état des quatre modificateurs. C'est le backend qui
/// dit lequel des deux fait le raccourci
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — la webview n'a ni
/// table de touches, ni combinaison, ni règle de comparaison.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct KeyStroke {
    /// `KeyboardEvent.key` — le **caractère produit** (`w`, `W`, `&`, `,`), ou le nom de la
    /// touche quand elle n'en produit aucun (`Tab`, `ArrowUp`, `F5`).
    ///
    /// C'est **la** source du raccourci : macOS apparie par caractère.
    pub key: String,
    /// `KeyboardEvent.code` — la position physique, nommée d'après un clavier US.
    ///
    /// Un **repli**, et rien d'autre : il ne sert que lorsque le caractère n'a pas
    /// d'écriture d'accélérateur. Voir [`Combination::from_stroke`].
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
/// **La seconde colonne est aussi le caractère produit**, pour tout ce qui en produit un :
/// `KeyT` s'écrit `T` et se frappe sur la touche qui donne `t`. C'est ce qui permet à
/// [`Key::produced`] de lire la table dans l'autre sens sans qu'une seconde table ait à
/// exister — et, quand deux tables existent, c'est toujours celle qu'on ne regarde pas qui
/// se trompe. Les touches nommées (`⇥`, `⎋`, `↑`, `F1`) ne produisent aucun caractère : leur
/// glyphe n'est qu'un glyphe.
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

    /// La touche qu'un **caractère produit** désigne — `w`, `W`, `,`, `1`, `+`, l'espace.
    ///
    /// C'est l'entrée d'une capture, et c'est la comparaison que macOS fait lui-même. La
    /// casse est ignorée : `⇧` plus la touche `W` produit `W`, et ce doit être la même
    /// liaison que `w`, sans quoi une même combinaison en ferait deux.
    ///
    /// Le glyphe de la table **est** le caractère pour tout ce qui en produit un — c'est ce
    /// qui rend la comparaison directe, sans seconde table à tenir en face de la première.
    /// Les touches nommées (`⇥`, `⎋`, `F1`) n'en produisent aucun, et ne se trouvent donc
    /// pas par ici : leur nom passe par [`Key::parse`].
    fn produced(character: &str) -> Option<Self> {
        let mut letters = character.chars();
        let single = letters.next().filter(|_| letters.next().is_none())?;
        // L'espace est le seul caractère dont le glyphe de menu (`␣`) n'est pas lui-même.
        if single == ' ' {
            return Key::parse("Space");
        }
        let sought = single.to_ascii_uppercase();
        KEYS.iter()
            .find(|(_, glyph)| {
                let mut written = glyph.chars();
                matches!(
                    (written.next(), written.next()),
                    (Some(only), None) if only.to_ascii_uppercase() == sought
                )
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
        let key = Self::key_of(stroke)
            .filter(|key| !CAPTURE_ISSUES.contains(&key.name()))
            .ok_or_else(|| ShortcutError::UnusableKey {
                key: stroke.key.clone(),
            })?;
        let modifiers = stroke.modifiers();
        if modifiers.bare() {
            return Err(ShortcutError::BareKey);
        }
        Ok(Self { modifiers, key })
    }

    /// La touche d'une frappe : **son caractère**, et sa position seulement en dernier
    /// recours.
    ///
    /// L'ordre est la décision de l'issue #133, et il se lit comme une règle de préséance :
    ///
    /// 1. le **caractère produit** — `w` sur la touche marquée `W`, quelle que soit sa
    ///    position. C'est ce que macOS apparie, donc c'est la vérité ;
    /// 2. le **nom** de la touche, pour celles qui ne produisent aucun caractère — `Tab`,
    ///    `ArrowUp`, `F5`, `Delete`. `⌃⇥` passe par là, exactement comme avant ;
    /// 3. la **position physique**, et seulement si les deux premiers ne donnent rien.
    ///
    /// Le troisième point n'est pas un reste de l'ancien choix : c'est le seul chemin qui
    /// reste ouvert quand le caractère n'a **aucune** écriture d'accélérateur. Deux familles
    /// sont dans ce cas, et elles ne sont pas rares sur un clavier français :
    ///
    /// - la rangée du haut d'un AZERTY, qui produit `&`, `é`, `"` sans `⇧`. Le repli y rend
    ///   `Digit1`…`Digit9`, ce qui est **exactement** ce que macOS fait de son côté : le
    ///   menu affiche `⌘&` et répond à cette touche-là (c'est ce qui fait marcher `⌘1`…`⌘9`
    ///   sur cette machine) ;
    /// - `⌥` combiné à une lettre, qui compose un caractère (`⌥t` → `†`) là où AppKit, lui,
    ///   compare le caractère **sans** l'option. Sans le repli, `⌥` deviendrait un
    ///   modificateur qu'aucune capture ne pourrait plus poser.
    ///
    /// Son prix est écrit : sur une touche que les deux dispositions ne placent pas au même
    /// endroit, une combinaison à `⌥` posée par ce repli peut désigner une autre touche que
    /// celle qu'on a pressée. C'est moins bon qu'un refus franc si l'on ne regarde que ce
    /// cas, et bien meilleur que refuser les deux familles entières.
    fn key_of(stroke: &KeyStroke) -> Option<Key> {
        Key::produced(&stroke.key)
            .or_else(|| Key::parse(&stroke.key))
            .or_else(|| Key::parse(&stroke.code))
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
                key: accelerator.to_owned(),
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
                        key: other.to_owned(),
                    })
                }
            }
        }

        let key = Key::parse(key_name).ok_or_else(|| ShortcutError::UnusableKey {
            key: key_name.to_owned(),
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

    /// Une frappe, telle que la webview la rapporte. Par défaut : `⌘T` sur un clavier US.
    pub struct StrokeBuilder(KeyStroke);

    impl StrokeBuilder {
        pub fn new() -> Self {
            Self(KeyStroke {
                key: "t".to_owned(),
                code: "KeyT".to_owned(),
                command: true,
                control: false,
                option: false,
                shift: false,
            })
        }

        /// Le caractère produit **et** la position, quand les deux coïncident — le cas d'un
        /// clavier US, et de la plupart des touches partout ailleurs.
        pub fn typing(self, character: &str, code: &str) -> Self {
            self.key(character).code(code)
        }

        pub fn key(mut self, key: &str) -> Self {
            self.0.key = key.to_owned();
            self
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

        pub fn control(mut self) -> Self {
            self.0.control = true;
            self
        }

        pub fn build(self) -> KeyStroke {
            self.0
        }
    }

    #[test]
    fn given_a_combination_captured_at_the_keyboard_when_it_is_handed_to_the_menu_then_it_is_the_accelerator_muda_parses(
    ) {
        // Given — `⇧` plus la touche `T` : la webview rapporte le caractère produit, ici
        // en majuscule, et c'est `muda` qui relira le nom canonique
        let stroke = StrokeBuilder::new().typing("T", "KeyT").shift().build();

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
            .map(|named| {
                Combination::from_stroke(&StrokeBuilder::new().typing(named, named).build())
                    .is_err()
            })
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
            Combination::from_stroke(&StrokeBuilder::new().typing("t", "KeyT").build()).unwrap();

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
    fn given_an_azerty_keyboard_when_the_key_marked_w_is_pressed_then_the_binding_is_the_one_that_key_plays(
    ) {
        // Given — la touche marquée `W` d'un AZERTY est à la **position** `KeyZ` d'un
        // clavier US. C'est le cas qui a tout révélé (issue #133) : retenir la position
        // posait `⌘Z`, le menu n'apparaissait plus avec aucun accélérateur, et la touche
        // pressée continuait de fermer l'onglet — l'action devenait injoignable
        let azerty = StrokeBuilder::new().typing("w", "KeyZ").build();

        // When
        let captured = Combination::from_stroke(&azerty).unwrap();

        // Then — macOS apparie un équivalent clavier par **caractère** : c'est `⌘W` qu'il
        // faut poser pour que cette touche-là joue l'action, et c'est `⌘W` que l'écran doit
        // montrer
        assert_eq!(captured.accelerator(), "Cmd+KeyW");
        assert_eq!(captured.glyphs(), "⌘W");
        assert_eq!(captured, Combination::parse("Cmd+W").unwrap());
    }

    #[test]
    fn given_the_same_letter_with_and_without_shift_when_both_are_captured_then_they_are_two_bindings_not_four(
    ) {
        // Given — `⇧` plus une lettre produit une **majuscule**, et la webview la rapporte
        // telle quelle. Sans repli sur la casse, `⌘G` et `⌘g` seraient deux liaisons
        // différentes, et un conflit ne se verrait pas d'une écriture à l'autre
        let lower = StrokeBuilder::new().typing("g", "KeyG").build();
        let upper = StrokeBuilder::new().typing("G", "KeyG").shift().build();

        // When
        let read = [
            Combination::from_stroke(&lower).unwrap(),
            Combination::from_stroke(&upper).unwrap(),
        ];

        // Then — la seule différence entre les deux est le `⇧` que l'utilisateur a tenu
        assert_eq!(read[0].accelerator(), "Cmd+KeyG");
        assert_eq!(read[1].accelerator(), "Shift+Cmd+KeyG");
    }

    #[test]
    fn given_a_character_no_accelerator_can_write_when_it_is_captured_then_the_physical_key_answers_for_it(
    ) {
        // Given — la rangée du haut d'un AZERTY produit `&` sans `⇧`, et `parse_code` de
        // `muda` n'a aucune écriture pour `&`. Le refuser fermerait toute la rangée des
        // chiffres à un clavier français, alors que macOS, lui, répond à cette touche pour
        // un équivalent `1` — c'est ce qui fait marcher `⌘1`…`⌘9` sur cette machine
        let azerty = StrokeBuilder::new().typing("&", "Digit1").build();

        // When
        let captured = Combination::from_stroke(&azerty).unwrap();

        // Then — et le menu l'écrit `⌘1`, là où macOS affichera `⌘&`
        assert_eq!(captured.accelerator(), "Cmd+Digit1");
        assert_eq!(captured, Combination::parse("Cmd+1").unwrap());
    }

    #[test]
    fn given_a_key_that_produces_no_character_when_it_is_captured_then_it_is_still_named() {
        // Given — `⌃⇥` n'a pas de caractère à apparier, et c'est le seul raccourci que la
        // webview joue elle-même (en-tête de `src-tauri/src/menu.rs`) : le casser aurait
        // décroché la circulation entre onglets
        let named = StrokeBuilder::new()
            .typing("Tab", "Tab")
            .bare()
            .control()
            .build();

        // When
        let captured = Combination::from_stroke(&named).unwrap();

        // Then
        assert_eq!(captured.accelerator(), "Ctrl+Tab");
        assert_eq!(captured.glyphs(), "⌃⇥");
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
