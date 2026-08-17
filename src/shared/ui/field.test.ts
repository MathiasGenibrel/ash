import { describe, expect, it } from "bun:test";

import { field } from "./field";
import { FOCUS_KEY } from "./node";

describe("un champ de saisie", () => {
    it("Given a field being typed into, when its input handler fires, then the view receives the typed value and never an event", () => {
        // Given — c'est ce qui rend une vue à champs testable : le composant ne lit pas
        // `event.target`, il reçoit une chaîne
        const typed: string[] = [];
        const described = field("config")
            .value("~/.claude")
            .onInput((value) => {
                typed.push(value);
            })
            .build();

        // When
        described.on["input"]?.({ value: "~/.claude/settings.json", key: "", shiftKey: false });

        // Then
        expect(typed).toEqual(["~/.claude/settings.json"]);
        expect(described.attrs["value"]).toBe("~/.claude");
    });

    it("Given a field that the view will rebuild, when it is given a focus key, then the key travels in its description", () => {
        // Given — sans cette clé, la relance différée redessinerait la carte au milieu d'un
        // mot et le curseur partirait avec l'ancien élément
        const described = field("config").focusKey("path:claude").build();

        // Then
        expect(described.attrs[FOCUS_KEY]).toBe("path:claude");
    });

    it("Given a field being typed into, when any key but Enter is pressed, then nothing says the typing is over", () => {
        // Given — `⏎` abrège l'attente de la relance différée. Le déclencher sur une autre
        // touche vérifierait à chaque frappe, ce que les 400 ms existent pour éviter
        const done: number[] = [];
        const described = field("config")
            .onSubmit(() => {
                done.push(1);
            })
            .build();

        // When
        described.on["keydown"]?.({ value: "~/.cl", key: "l", shiftKey: false });
        described.on["keydown"]?.({ value: "~/.claude", key: "Enter", shiftKey: false });

        // Then
        expect(done).toHaveLength(1);
    });

    it("Given a field whose submission has two directions, when Enter is pressed with and without Shift, then the same gesture is reported reversed", () => {
        // Given — `⇧⏎` n'est pas un second geste : c'est le même, pris à l'envers. La
        // recherche du scrollback (#79) en dépend, `⏎` allant à l'occurrence suivante et
        // `⇧⏎` à la précédente
        const directions: boolean[] = [];
        const described = field("search")
            .onSubmit(({ reversed }) => {
                directions.push(reversed);
            })
            .build();

        // When
        described.on["keydown"]?.({ value: "todo", key: "Enter", shiftKey: false });
        described.on["keydown"]?.({ value: "todo", key: "Enter", shiftKey: true });

        // Then
        expect(directions).toEqual([false, true]);
    });

    it("Given a field that both submits and cancels, when each of the two keys is pressed, then neither handler has replaced the other", () => {
        // Given — les gestionnaires sont indexés par nom d'événement, et `⏎` comme `⎋`
        // arrivent en `keydown` : sans table de touches, le second appel écraserait le
        // premier et la panne ne se verrait qu'au clavier
        const gestures: string[] = [];
        const described = field("search")
            .onSubmit(() => {
                gestures.push("submit");
            })
            .onCancel(() => {
                gestures.push("cancel");
            })
            .build();

        // When
        described.on["keydown"]?.({ value: "todo", key: "Escape", shiftKey: false });
        described.on["keydown"]?.({ value: "todo", key: "Enter", shiftKey: false });
        described.on["keydown"]?.({ value: "todo", key: "o", shiftKey: false });

        // Then
        expect(gestures).toEqual(["cancel", "submit"]);
    });

    it("Given a field, when it is described, then it names itself for a screen reader", () => {
        // Given — un champ posé dans une grille n'a pas de `<label for>` qui le désigne
        const described = field("config").build();

        // Then
        expect(described.attrs["aria-label"]).toBe("config");
        expect(described.tag).toBe("input");
    });
});
