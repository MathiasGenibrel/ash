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
        // filtre, chaque ⌥← enverrait `ESC b` deux fois.
        const release = press("ArrowLeft").withOption().released().build();

        // When
        const sent = resolveKeyBinding(release);

        // Then
        expect(sent).toBeNull();
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

    it("Given every chord of the table, when its sequence is inspected, then none of them submits a line", () => {
        // Given — ADR-0015 : Ash compose, l'utilisateur envoie. Un raccourci d'édition qui
        // glisserait un `\r` validerait une commande à la place de l'utilisateur.
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
