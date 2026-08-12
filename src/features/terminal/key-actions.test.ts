import { describe, expect, it } from "bun:test";

import { applyKeyAction, resolveKeyAction, type ScrollSurface } from "./key-actions";
import { resolveKeyBinding, type KeyChord } from "./key-bindings";

/**
 * Test Data Builder : un accord de touches, décrit par ce qu'on presse.
 *
 * Les défauts sont ceux d'une frappe nue — un `keydown`, aucun modificateur — parce que
 * c'est le cas qui doit rester intact : une flèche seule appartient à l'historique de
 * `zsh`, pas au défilement.
 *
 * Il est écrit ici plutôt que partagé avec `key-bindings.test.ts` : importer un fichier
 * `*.test.ts` depuis un autre ferait réenregistrer ses `describe` dans les deux, et
 * chaque test de la saisie tournerait deux fois.
 */
class ChordBuilder {
    private constructor(private readonly chord: KeyChord) {}

    static press(key: string): ChordBuilder {
        return new ChordBuilder({
            type: "keydown",
            key,
            altKey: false,
            ctrlKey: false,
            metaKey: false,
            shiftKey: false,
        });
    }

    withOption(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, altKey: true });
    }

    withCommand(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, metaKey: true });
    }

    withControl(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, ctrlKey: true });
    }

    withShift(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, shiftKey: true });
    }

    released(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, type: "keyup" });
    }

    build(): KeyChord {
        return this.chord;
    }
}

const press = (key: string): ChordBuilder => ChordBuilder.press(key);

/** Les quatre accords de la spec, dans l'ordre où l'issue #78 les écrit. */
const scrollingChords = [
    press("ArrowUp").withCommand(),
    press("ArrowDown").withCommand(),
    press("ArrowUp").withCommand().withShift(),
    press("ArrowDown").withCommand().withShift(),
];

/** Une surface qui note ce qu'on lui demande, pour lire le geste et son sens. */
class RecordingSurface implements ScrollSurface {
    readonly moves: string[] = [];

    scrollPages(pageCount: number): void {
        this.moves.push(`pages:${pageCount}`);
    }

    scrollLines(amount: number): void {
        this.moves.push(`lines:${amount}`);
    }
}

describe("les raccourcis de défilement du scrollback", () => {
    it("Given the four scrollback chords of the spec, when they are resolved, then each one moves the viewport", () => {
        // Given — `⌘↑`, `⌘↓`, `⌘⇧↑`, `⌘⇧↓`.

        // When
        const actions = scrollingChords.map((chord) => resolveKeyAction(chord.build()));

        // Then
        expect(actions).toEqual([
            "scroll-page-up",
            "scroll-page-down",
            "scroll-line-up",
            "scroll-line-down",
        ]);
    });

    it("Given the four scrollback chords, when the input table also sees them, then nothing is sent to the PTY", () => {
        // Given — la vue compose les deux résolveurs, saisie d'abord et action ensuite.
        // Une entrée qui viendrait recouvrir `⌘↑` dans `key-bindings.ts` enverrait des
        // octets **avant** que le défilement n'ait lieu : le shell recevrait une commande
        // pour un geste d'affichage.

        // When
        const sent = scrollingChords.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(sent).toEqual([null, null, null, null]);
    });

    it("Given a bare arrow key, when it is resolved, then it belongs to the shell history and not to the viewport", () => {
        // Given — ↑ et ↓ seules parcourent l'historique des commandes de `zsh`. Les capter
        // pour faire défiler priverait le shell de sa navigation la plus courante.
        const bare = [press("ArrowUp"), press("ArrowDown")];

        // When
        const actions = bare.map((chord) => resolveKeyAction(chord.build()));
        const sent = bare.map((chord) => resolveKeyBinding(chord.build()));

        // Then
        expect(actions).toEqual([null, null]);
        expect(sent).toEqual([null, null]);
    });

    it("Given an arrow pressed with an extra modifier, when it is resolved, then it is left alone", () => {
        // Given — `⌥⌘↑` et `⌃↑` ne sont pas des raccourcis d'Ash ; les avaler priverait le
        // shell, ou la composition de macOS, de ce qu'ils en auraient fait.
        const extra = [
            press("ArrowUp").withCommand().withOption(),
            press("ArrowUp").withCommand().withControl(),
            press("ArrowDown").withControl(),
            press("ArrowUp").withShift(),
        ];

        // When
        const actions = extra.map((chord) => resolveKeyAction(chord.build()));

        // Then
        expect(actions).toEqual([null, null, null, null]);
    });

    it("Given a scrollback chord that is released, when it is resolved, then the viewport does not move a second time", () => {
        // Given — xterm.js appelle le gestionnaire pour `keydown` **et** `keyup` ; sans le
        // filtre, chaque `⌘↑` paginerait deux fois.
        const release = press("ArrowUp").withCommand().released().build();

        // When
        const action = resolveKeyAction(release);

        // Then
        expect(action).toBeNull();
    });

    it("Given a menu accelerator of the native menu, when it is resolved, then the table lets it through", () => {
        // Given — la liste de `src-tauri/src/menu.rs`. macOS les consomme avant la webview,
        // mais ce test est le garde-fou du jour où la table grandira (#79) : une entrée qui
        // recouvrirait un accélérateur casserait le menu sans bruit.
        const accelerators = [
            press("n").withCommand(),
            press("n").withCommand().withShift(),
            press("w").withCommand(),
            press("k").withCommand(),
            press("b").withCommand(),
            press(",").withCommand(),
            press("c").withCommand(),
            press("v").withCommand(),
            ...["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((digit) =>
                press(digit).withCommand(),
            ),
        ];

        // When
        const actions = accelerators.map((chord) => resolveKeyAction(chord.build()));

        // Then
        expect(actions.filter((action) => action !== null)).toEqual([]);
    });

    it("Given each scrollback action, when it is applied, then it moves the viewport in the direction its name announces", () => {
        // Given — la convention de signe de xterm.js : négatif remonte, positif redescend.
        // C'est l'erreur qu'on veut voir tomber ici plutôt que sous les doigts.
        const surface = new RecordingSurface();

        // When
        applyKeyAction("scroll-page-up", surface);
        applyKeyAction("scroll-page-down", surface);
        applyKeyAction("scroll-line-up", surface);
        applyKeyAction("scroll-line-down", surface);

        // Then
        expect(surface.moves).toEqual(["pages:-1", "pages:1", "lines:-1", "lines:1"]);
    });
});
