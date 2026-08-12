import { describe, expect, it } from "bun:test";

import { FOCUS_KEY, field } from "./field";

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
        described.on["input"]?.({ value: "~/.claude/settings.json" });

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

    it("Given a field, when it is described, then it names itself for a screen reader", () => {
        // Given — un champ posé dans une grille n'a pas de `<label for>` qui le désigne
        const described = field("config").build();

        // Then
        expect(described.attrs["aria-label"]).toBe("config");
        expect(described.tag).toBe("input");
    });
});
