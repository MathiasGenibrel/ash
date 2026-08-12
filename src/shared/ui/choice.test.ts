import { describe, expect, it } from "bun:test";

import { choice } from "./choice";
import { plainText } from "./read";

describe("un choix parmi une liste", () => {
    it("Given an entry whose adapter is the second of the list, when its menu is described, then that one is the one marked as chosen", () => {
        // Given — la faute que les deux menus écrits à la main pouvaient faire : proposer
        // les valeurs sans marquer celle qui est en vigueur, donc afficher la première en
        // prétendant que c'est celle de l'entrée
        const described = choice("adapter").options(["claude-code", "codex", "generic"], "codex").build();

        // When
        const chosen = described.children.filter(
            (option) => option.kind === "element" && option.attrs["selected"] !== undefined,
        );

        // Then
        expect(chosen.map(plainText)).toEqual(["codex"]);
    });

    it("Given a value the backend sends but the list does not offer, when the menu is described, then nothing is passed off as chosen", () => {
        // Given — inventer une sélection ferait lire un adaptateur qu'Ash n'embarque pas
        // comme s'il était celui de l'entrée
        const described = choice("adapter").options(["claude-code", "generic"], "codex").build();

        // When
        const chosen = described.children.filter(
            (option) => option.kind === "element" && option.attrs["selected"] !== undefined,
        );

        // Then
        expect(chosen).toEqual([]);
    });

    it("Given a menu being changed, when its handler fires, then the view receives the picked value and never an event", () => {
        // Given — c'est ce qui rend testable la carte qui relance sa vérification sur un
        // changement d'adaptateur
        const picked: string[] = [];
        const described = choice("adapter")
            .options(["claude-code", "generic"], "generic")
            .onSelect((value) => {
                picked.push(value);
            })
            .build();

        // When
        described.on["change"]?.({ value: "claude-code", key: "" });

        // Then
        expect(picked).toEqual(["claude-code"]);
    });
});
