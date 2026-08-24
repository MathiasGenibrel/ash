import { describe, expect, it } from "bun:test";

import { find, findAll, plainText, type UiChild } from "@/shared/ui";

import { aSuggestion } from "../builders";
import { suggestionList, type SuggestionActions } from "./suggestions";

/** Ce que le bloc a demandé — un seul geste existe, et c'est le point. */
function recorder(): { asked: string[] } & SuggestionActions {
    const asked: string[] = [];
    return {
        asked,
        declareSuggestion: (suggestion) => asked.push(`declare ${suggestion.command}`),
    };
}

/** Le bloc entier — `suggestionList` rend une section, ou rien. */
function block(children: readonly UiChild[]): UiChild {
    const section = children[0];
    if (section === undefined) throw new Error("le bloc est vide");
    return section;
}

describe("les outils qu'ash a vus tourner", () => {
    it("Given a tool ash saw running, when the block is described, then its name, its adapter and what its configuration carries are all there", () => {
        // Given — la fenêtre ouvrait sur « no tools declared » pendant qu'ash savait que
        // `claude` tenait l'avant-plan d'un onglet (ADR-0006). La ligne doit dire les trois :
        // ce que c'est, ce qui le traduit, et ce que son fichier porte
        const suggestions = [aSuggestion()];

        // When
        const described = suggestionList(suggestions, recorder());

        // Then
        const said = described.map(plainText).join("");
        expect(said).toContain("claude");
        expect(said).toContain("claude-code");
        expect(said).toContain("no ash hooks in this file");
    });

    it("Given a suggestion whose file already carries other hooks, when the block is described, then the conflict does not read like an absence", () => {
        // Given — les cinq états de `HookState`, pas les trois d'`Instrumented` : ce dernier
        // n'a pas de `conflict`, et un utilisateur qui outille déjà son agent lirait
        // « rien n'est posé » là où quelque chose l'est (ADR-0007)
        const suggestions = [
            aSuggestion({ hooks: "conflict", summary: "2 hooks here are not ash's" }),
        ];

        // When
        const described = suggestionList(suggestions, recorder());

        // Then — la forme aussi, pas seulement la phrase : c'est elle qui porte l'état
        const node = block(described);
        expect(plainText(find(node, "settings-hooks-reason") ?? node)).toBe(
            "2 hooks here are not ash's",
        );
        expect(findAll(node, "is-conflict").length).toBeGreaterThan(0);
    });

    it("Given a suggestion, when its only button is pressed, then it asks to declare it — and the block promises nothing is written", () => {
        // Given — le clic déclare, et **ne pose aucun hook** : l'entrée repart dans le flux
        // qui existe déjà, et l'installation reste une pression séparée (ADR-0007)
        const actions = recorder();
        const described = suggestionList([aSuggestion()], actions);
        const node = block(described);

        // When
        const press = findAll(node, "settings-button");
        expect(press).toHaveLength(1);
        press[0]?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(actions.asked).toEqual(["declare claude"]);
        expect(described.map(plainText).join("")).toContain("declaring one writes nothing");
    });

    it("Given nothing seen running, when the block is described, then it does not exist at all", () => {
        // Given — un en-tête « seen running » vide serait une promesse creuse sur une machine
        // où aucun agent n'a jamais été lancé
        // When
        const described = suggestionList([], recorder());

        // Then
        expect(described).toEqual([]);
    });
});
