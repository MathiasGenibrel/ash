import { describe, expect, it } from "bun:test";

import { menuAccelerators, press } from "./builders";
import { resolveKeyBinding } from "./key-bindings";

describe("les raccourcis d'édition de ligne", () => {
    it("Given the six line editing chords of the spec, when they are resolved, then each one sends the sequence readline expects", () => {
        // Given — la table de l'issue #75, dans l'ordre où elle y est écrite.
        const chords = [
            press("ArrowLeft").withOption(),
            press("ArrowRight").withOption(),
            press("ArrowLeft").withCommand(),
            press("ArrowRight").withCommand(),
            press("Backspace").withOption(),
            press("Delete").withOption(),
            press("Backspace").withCommand(),
            press("Delete").withCommand(),
        ];

        // When
        const sent = chords.map((chord) => resolveKeyBinding(chord.build()));

        // Then — `ESC b`/`ESC f`, `Ctrl-A`/`Ctrl-E`, `ESC ⌫`/`ESC d`, `Ctrl-U`/`Ctrl-K`.
        expect(sent).toEqual([
            "\x1bb",
            "\x1bf",
            "\x01",
            "\x05",
            "\x1b\x7f",
            "\x1bd",
            "\x15",
            "\x0b",
        ]);
    });

    it("Given a character macOS composes with the option key, when it is resolved, then nothing is sent and xterm keeps the keystroke", () => {
        // Given — `|` est ⌥⇧L sur AZERTY, `{` est ⌥(, `~` passe par une touche morte. Le
        // défaut de #75 était de les intercepter : macOS n'avait plus rien à composer.
        const composed = [
            press("|").withOption().withShift(),
            press("{").withOption(),
            press("[").withOption().withShift(),
            press("Dead").withOption(),
            press("€").withOption(),
        ];

        // When
        const sent = composed.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(sent).toEqual([null, null, null, null, null]);
    });

    it("Given the control keys typed directly, when they are resolved, then the table adds a path and replaces none", () => {
        // Given — `Ctrl-A`, `Ctrl-E`, `Ctrl-U`, `Ctrl-K` sont déjà traités par xterm.js.
        const controls = ["a", "e", "u", "k"].map((key) => press(key).withControl());

        // When
        const sent = controls.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(sent).toEqual([null, null, null, null]);
    });

    it("Given a menu accelerator of the native menu, when it is resolved, then the table lets it through", () => {
        // Given — la liste de `src-tauri/src/menu.rs`. macOS les consomme avant la webview,
        // mais ce test est le garde-fou du jour où la table grandira (#77 à #80) : une
        // entrée qui recouvrirait un accélérateur casserait le menu sans bruit.
        const accelerators = menuAccelerators();

        // When
        const sent = accelerators.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(sent.filter((bytes) => bytes !== null)).toEqual([]);
    });

    it("Given a bound chord that is released, when it is resolved, then nothing is sent a second time", () => {
        // Given — xterm.js appelle le gestionnaire pour `keydown` **et** `keyup` ; sans le
        // filtre, chaque ⌥← enverrait `ESC b` deux fois. ⇧⏎ est le cas où ça se paierait le
        // plus cher : deux `ESC CR` pour une frappe, donc deux lignes dans le prompt.
        const released = [
            press("ArrowLeft").withOption().released(),
            press("Enter").withShift().released(),
        ];

        // When
        const sent = released.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(sent).toEqual([null, null]);
    });

    it("Given a bound key pressed with an extra modifier, when it is resolved, then it is left to the shell", () => {
        // Given — ⌃⌥← et ⇧⌥← ne sont pas des raccourcis d'Ash ; les avaler priverait le
        // shell de ce qu'il en aurait fait.
        const extra = [
            press("ArrowLeft").withOption().withControl(),
            press("ArrowLeft").withOption().withShift(),
            press("Backspace").withCommand().withShift(),
        ];

        // When
        const sent = extra.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(sent).toEqual([null, null, null]);
    });

    it("Given every line editing chord, when its sequence is inspected, then none of them submits a line", () => {
        // Given — ADR-0015 : Ash compose, l'utilisateur envoie. Un raccourci d'édition qui
        // glisserait un `\r` validerait une commande à la place de l'utilisateur. ⇧⏎ n'est
        // pas de la liste, et ne peut pas l'être : sa séquence contient un `\r` par
        // construction. Ce n'est pas la même chose — la touche pressée **est** ⏎, Ash
        // relaie une frappe au lieu d'en fabriquer une, et `ESC` devant valide moins que le
        // `CR` nu qui part aujourd'hui.
        const bound = [
            press("ArrowLeft").withOption(),
            press("ArrowRight").withOption(),
            press("ArrowLeft").withCommand(),
            press("ArrowRight").withCommand(),
            press("Backspace").withOption(),
            press("Delete").withOption(),
            press("Backspace").withCommand(),
            press("Delete").withCommand(),
        ];

        // When
        const sent = bound.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(sent.some((bytes) => bytes?.includes("\r") === true)).toBe(false);
        expect(sent.some((bytes) => bytes?.includes("\n") === true)).toBe(false);
    });
});

describe("le retour à la ligne dans un prompt", () => {
    it("Given Shift+Enter, when it is resolved, then it sends ESC+CR so an agent can tell it from a bare Enter", () => {
        // Given — xterm.js n'a que `result.key = ev.altKey ? ESC + CR : CR` pour le keyCode
        // 13 (`@xterm/xterm` 6.0.0, `src/common/input/Keyboard.ts:102`) : `⇧⏎` et `⏎` y
        // sont le même octet, et l'agent à l'autre bout envoie donc le prompt dans les deux
        // cas. `ESC`+`CR` est la séquence que les terminaux configurés par
        // `claude /terminal-setup` envoient pour `⇧⏎`.
        const chord = press("Enter").withShift().build();

        // When
        const sent = resolveKeyBinding(chord);

        // Then
        expect(sent).toBe("\x1b\r");
    });

    it("Given a bare Enter, when it is resolved, then the table leaves it alone and the command is still submitted", () => {
        // Given — c'est la moitié qu'il ne faut surtout pas casser : `⏎` doit continuer de
        // partir en `CR` par le chemin de xterm.js. Le type `Chord` interdit d'ailleurs
        // d'écrire `"Enter"` dans la table, mais c'est le comportement qu'on protège ici,
        // pas le type.
        const chord = press("Enter").build();

        // When
        const sent = resolveKeyBinding(chord);

        // Then
        expect(sent).toBeNull();
    });

    it("Given Enter pressed with any other modifier, when it is resolved, then xterm keeps the keystroke it already handles", () => {
        // Given — `⌥⏎` envoie déjà `ESC`+`CR` par xterm.js, et `macOptionIsMeta: false` ne
        // le détourne pas : le chemin « third level shift » exige un keyCode > 47, et ⏎ a
        // le 13. Le recouvrir ici ne changerait rien pour l'utilisateur et ferait deux
        // sources pour une même séquence. `⌃⏎` et `⌘⏎` ne sont pas des accords d'Ash.
        const others = [
            press("Enter").withOption(),
            press("Enter").withOption().withShift(),
            press("Enter").withControl(),
            press("Enter").withCommand(),
            press("Enter").withShift().withCommand(),
        ];

        // When
        const sent = others.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(sent).toEqual([null, null, null, null, null]);
    });

    it("Given a shifted line editing chord, when it is resolved, then adding Shift to the table did not widen the others", () => {
        // Given — `⇧` était un refus sec avant #91 ; il est maintenant écrit dans l'accord
        // cherché. Le risque du changement est là, et pas ailleurs : que `⇧⌥←` se mette à
        // envoyer ce que `⌥←` envoie, et prive le shell de sa sélection par mot.
        const shifted = [
            press("ArrowLeft").withOption().withShift(),
            press("ArrowRight").withOption().withShift(),
            press("Backspace").withCommand().withShift(),
            press("Delete").withOption().withShift(),
        ];

        // When
        const sent = shifted.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(sent).toEqual([null, null, null, null]);
    });
});
