import { describe, expect, it } from "bun:test";

import { matchShortcut, type KeyStroke } from "./shortcuts";

/**
 * Test Data Builder : une frappe part sans aucun modificateur, et n'en porte que ceux
 * qu'on lui ajoute. C'est la seule façon de lire d'un coup d'œil ce qui distingue `⇥` de
 * `⌃⇥` — la différence tient à un booléen, et c'est tout le sujet.
 */
class KeyStrokeBuilder {
    private stroke: KeyStroke = {
        key: "Tab",
        ctrlKey: false,
        shiftKey: false,
        metaKey: false,
        altKey: false,
    };

    static press(key: string): KeyStrokeBuilder {
        const builder = new KeyStrokeBuilder();
        builder.stroke = { ...builder.stroke, key };
        return builder;
    }

    withCtrl(): this {
        this.stroke = { ...this.stroke, ctrlKey: true };
        return this;
    }

    withShift(): this {
        this.stroke = { ...this.stroke, shiftKey: true };
        return this;
    }

    withCmd(): this {
        this.stroke = { ...this.stroke, metaKey: true };
        return this;
    }

    withOption(): this {
        this.stroke = { ...this.stroke, altKey: true };
        return this;
    }

    build(): KeyStroke {
        return this.stroke;
    }
}

describe("les raccourcis de cyclage", () => {
    it("Given Ctrl and Tab are pressed together, when the stroke is read, then it asks for the next tab", () => {
        // Given
        const stroke = KeyStrokeBuilder.press("Tab").withCtrl().build();

        // When
        const action = matchShortcut(stroke);

        // Then
        expect(action).toEqual({ kind: "next-tab" });
    });

    it("Given Shift is held too, when the stroke is read, then it asks for the previous tab", () => {
        // Given
        const stroke = KeyStrokeBuilder.press("Tab").withCtrl().withShift().build();

        // When
        const action = matchShortcut(stroke);

        // Then
        expect(action).toEqual({ kind: "previous-tab" });
    });
});

describe("ce que le terminal doit continuer de recevoir", () => {
    it("Given Tab is pressed on its own, when the stroke is read, then it is no shortcut and reaches the shell", () => {
        // Given — la complétion de `zsh`. Elle coûterait bien plus cher que le raccourci
        // ne rapporte, et c'est elle qui a dicté la forme de la règle.
        const stroke = KeyStrokeBuilder.press("Tab").build();

        // When
        const action = matchShortcut(stroke);

        // Then — `null` veut dire « laisse passer » : rien n'est ni arrêté ni annulé
        expect(action).toBeNull();
    });

    it("Given Tab is pressed with Cmd or with Option, when the stroke is read, then it is no shortcut either", () => {
        // Given — `⌘⇥` appartient au commutateur d'applications de macOS, et `⌥⇥` à la
        // saisie ; les revendiquer volerait une touche à quelqu'un d'autre
        const strokes = [
            KeyStrokeBuilder.press("Tab").withCmd().build(),
            KeyStrokeBuilder.press("Tab").withCtrl().withCmd().build(),
            KeyStrokeBuilder.press("Tab").withCtrl().withOption().build(),
        ];

        // When
        const actions = strokes.map(matchShortcut);

        // Then
        expect(actions).toEqual([null, null, null]);
    });

    it("Given Ctrl and a letter, when the stroke is read, then it is no shortcut and the line editing keys keep working", () => {
        // Given — `Ctrl+A`, `Ctrl+E`, `Ctrl+C` : le shell en a un usage quotidien
        const stroke = KeyStrokeBuilder.press("c").withCtrl().build();

        // When
        const action = matchShortcut(stroke);

        // Then
        expect(action).toBeNull();
    });

    it("Given the chords a text field owns, when they are read, then none of them is a shortcut", () => {
        // Given — cette écoute est posée **en capture sur le document**, donc elle voit
        // le clavier avant tout champ de saisie, et aucun `stopPropagation` posé sur un
        // champ ne peut l'arrêter. La boîte de recherche du terminal (`⌘F`) en est un.
        //
        // Aujourd'hui rien ne se recouvre : la règle n'accepte que `⌃⇥`, qui n'a aucun
        // sens d'édition de texte. Ce test existe pour le jour où quelqu'un ajoutera un
        // accord ici — coller un terme qu'on vient de copier est le geste le plus
        // fréquent d'une recherche, et le lui voler la rendrait inutilisable.
        const owned = [
            KeyStrokeBuilder.press("f").withCmd().build(),
            KeyStrokeBuilder.press("a").withCmd().build(),
            KeyStrokeBuilder.press("c").withCmd().build(),
            KeyStrokeBuilder.press("v").withCmd().build(),
            KeyStrokeBuilder.press("x").withCmd().build(),
            KeyStrokeBuilder.press("z").withCmd().build(),
            KeyStrokeBuilder.press("z").withCmd().withShift().build(),
            KeyStrokeBuilder.press("Escape").build(),
        ];

        // When
        const actions = owned.map(matchShortcut);

        // Then — si l'un d'eux cesse d'être `null`, il faudra d'abord décider ce qui
        // arrive quand un champ a le focus, et l'écrire ici.
        expect(actions).toEqual(owned.map(() => null));
    });
});
