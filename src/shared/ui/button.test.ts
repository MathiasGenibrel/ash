import { describe, expect, it } from "bun:test";

import { button } from "./button";
import { plainText } from "./read";

describe("un bouton", () => {
    it("Given a button that cannot be pressed, when it is disabled with its reason, then it keeps its label and carries the reason", () => {
        // Given — la maquette le répète trois fois : éteint, jamais masqué. « Le masquer
        // ferait croire que ça n'existe pas. »
        const built = button("install").disabled("cette entrée n'est pas vérifiée");

        // When
        const described = built.build();

        // Then
        expect(plainText(described)).toBe("install");
        expect(described.attrs["disabled"]).toBe("");
        expect(described.attrs["title"]).toBe("cette entrée n'est pas vérifiée");
        expect(described.attrs["aria-disabled"]).toBe("true");
    });

    it("Given a button, when it is disabled without a reason, then it does not compile", () => {
        // Given / When — la règle produit est dans la signature, pas dans une revue de
        // code. `@ts-expect-error` échoue à la compilation si l'appel devient légal : c'est
        // le test lui-même qui casse le jour où quelqu'un rend la raison facultative.
        // @ts-expect-error une raison est obligatoire pour éteindre un bouton
        const built = button("install").disabled();

        // Then — l'appel reste exécutable, il n'a simplement pas le droit d'être écrit
        expect(built.build().attrs["disabled"]).toBe("");
    });

    it("Given a button with a click handler, when its description is triggered, then the handler runs", () => {
        // Given — le geste se vérifie sans DOM : le gestionnaire est dans la description
        let pressed = 0;
        const described = button("add")
            .onClick(() => {
                pressed += 1;
            })
            .build();

        // When
        described.on["click"]?.({ value: "" });

        // Then
        expect(pressed).toBe(1);
    });
});
