import { describe, expect, it } from "bun:test";

import { menuAccelerators, press } from "./builders";
import { applyKeyAction, resolveKeyAction, type ActionSurface } from "./key-actions";
import { resolveKeyBinding } from "./key-bindings";

/** Les quatre accords de la spec, dans l'ordre où l'issue #78 les écrit. */
const scrollingChords = [
    press("ArrowUp").withCommand(),
    press("ArrowDown").withCommand(),
    press("ArrowUp").withCommand().withShift(),
    press("ArrowDown").withCommand().withShift(),
];

/**
 * Une surface qui note ce qu'on lui demande — **en français, pas en signes**.
 *
 * La convention de xterm.js est écrite ici une fois, et à l'endroit où elle se vérifie :
 * `scrollLines(amount)` documente « the number of lines to scroll **down** (negative
 * scroll up) », et `scrollPages(pageCount)` de même (`@xterm/xterm` 6.0.0,
 * `typings/xterm.d.ts`). Un lecteur de l'assertion n'a donc pas à connaître xterm.js pour
 * juger : il lit « une page vers le haut » en face de `scroll-page-up`.
 *
 * La traduction n'affaiblit pas le test — c'est la seule chose qu'on veut protéger. Un
 * `-1` écrit `+1` dans `applyKeyAction` fait dire « une page vers le bas » à la surface,
 * et l'assertion tombe.
 */
class RecordingSurface implements ActionSurface {
    readonly moves: string[] = [];

    scrollPages(pageCount: number): void {
        this.moves.push(describeMove(pageCount, "page"));
    }

    scrollLines(amount: number): void {
        this.moves.push(describeMove(amount, "ligne"));
    }

    openSearch(): void {
        this.moves.push("ouvre la recherche");
    }
}

function describeMove(amount: number, unit: "page" | "ligne"): string {
    const direction = amount < 0 ? "vers le haut" : "vers le bas";
    return `${Math.abs(amount)} ${unit} ${direction}`;
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
        // et ce test est le garde-fou de la table qui grandit : `⌘F` s'y est ajouté sans
        // recouvrir aucun accélérateur, et une entrée qui en recouvrirait un casserait le
        // menu sans bruit.
        const accelerators = menuAccelerators();

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
        expect(surface.moves).toEqual([
            "1 page vers le haut",
            "1 page vers le bas",
            "1 ligne vers le haut",
            "1 ligne vers le bas",
        ]);
    });
});

describe("le raccourci de recherche dans le scrollback", () => {
    it("Given Cmd+F, when it is resolved, then it opens the search of the current tab", () => {
        // Given — `⌘F` est ce que tout le monde tape ; il n'est déclaré dans aucun menu
        // natif, donc c'est bien la webview qui le voit
        const chord = press("f").withCommand().build();

        // When
        const action = resolveKeyAction(chord);

        // Then
        expect(action).toBe("open-search");
    });

    it("Given Cmd+F, when the input table also sees it, then nothing is sent to the PTY", () => {
        // Given — la vue compose les deux résolveurs, saisie d'abord et action ensuite : une
        // entrée qui recouvrirait `⌘F` dans `key-bindings.ts` écrirait des octets dans le
        // shell avant même que le champ ne s'ouvre
        const chord = press("f").withCommand().build();

        // When / Then
        expect(resolveKeyBinding(chord)).toBeNull();
    });

    it("Given Cmd+F typed with caps lock on, when it is resolved, then it still opens the search", () => {
        // Given — `KeyboardEvent.key` porte le caractère produit : verrou majuscules
        // enfoncé, la frappe arrive en `"F"` sans que `shiftKey` ne soit vrai. Sans
        // normalisation, `⌘F` serait muet et rien ne dirait pourquoi
        const locked = press("F").withCommand().build();
        // Et `⇧⌘F` reste un accord distinct : ⇧ module la navigation dans le champ, pas son
        // ouverture
        const shifted = press("F").withCommand().withShift().build();

        // When
        const actions = [resolveKeyAction(locked), resolveKeyAction(shifted)];

        // Then
        expect(actions).toEqual(["open-search", null]);
    });

    it("Given the letter f typed on its own, when it is resolved, then it is left to the shell", () => {
        // Given — sans le modificateur, `f` est une lettre ; l'avaler rendrait le terminal
        // inutilisable
        const bare = [press("f"), press("f").withControl(), press("f").withOption()];

        // When
        const actions = bare.map((chord) => resolveKeyAction(chord.build()));

        // Then
        expect(actions).toEqual([null, null, null]);
    });

    it("Given the open-search action, when it is applied, then it opens the field and moves nothing", () => {
        // Given — chercher n'est pas défiler : l'affichage reste où il est jusqu'à ce qu'une
        // occurrence soit trouvée
        const surface = new RecordingSurface();

        // When
        applyKeyAction("open-search", surface);

        // Then
        expect(surface.moves).toEqual(["ouvre la recherche"]);
    });
});
