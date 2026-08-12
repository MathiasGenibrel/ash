import { describe, expect, it } from "bun:test";

import { find, findAll, plainText, type UiElementNode } from "@/shared/ui";

import { composeSearchBox, type SearchBoxActions, type SearchBoxState } from "./search-box";

/**
 * La boîte de recherche, lue comme une description.
 *
 * Ce qui se vérifie ici est ce qui décide : le compteur d'occurrences, les boutons éteints
 * et leur raison, et le fait qu'une frappe dans le champ ne fasse rien d'autre que
 * chercher. Ce qui relève de la peinture — le focus rendu au terminal, le surlignage — est
 * dans `terminal-search.ts`, et se vérifie à la main : `bun test` n'a ni DOM ni xterm.js.
 */

/** Test Data Builder : l'état d'affichage de la boîte. Défauts : on vient de l'ouvrir. */
class SearchStateBuilder {
    private state: SearchBoxState = { query: "", matches: null };

    static opened(): SearchStateBuilder {
        return new SearchStateBuilder();
    }

    typing(query: string): this {
        this.state = { ...this.state, query };
        return this;
    }

    found(index: number, count: number): this {
        this.state = { ...this.state, matches: { index, count } };
        return this;
    }

    /** Le seuil de surlignage de l'addon est dépassé : il compte encore, sans savoir où. */
    beyondHighlightLimit(count: number): this {
        return this.found(-1, count);
    }

    build(): SearchBoxState {
        return this.state;
    }
}

const opened = (): SearchStateBuilder => SearchStateBuilder.opened();

/** Une paire d'actions qui note les gestes, sans rien exécuter. */
function recorder(): SearchBoxActions & { readonly gestures: string[] } {
    const gestures: string[] = [];
    return {
        gestures,
        search: (query) => gestures.push(`search:${query}`),
        findNext: () => gestures.push("next"),
        findPrevious: () => gestures.push("previous"),
        close: () => gestures.push("close"),
    };
}

const boxOf = (state: SearchBoxState, actions: SearchBoxActions): UiElementNode =>
    composeSearchBox(state, actions).build();

const tallyOf = (box: UiElementNode): string =>
    plainText(find(box, "terminal-search-tally") ?? box);

const stepsOf = (box: UiElementNode): readonly UiElementNode[] =>
    findAll(box, "terminal-search-step");

describe("le champ de recherche du scrollback", () => {
    it("Given a search that has walked into its third match out of twelve, when the box is composed, then the counter reads the position and the total", () => {
        // Given — le compteur est la seule chose qui dise si `⏎` a encore quelque part où
        // aller ; un total seul laisserait tourner en rond sans le savoir
        const state = opened().typing("todo").found(2, 12).build();

        // When
        const box = boxOf(state, recorder());

        // Then
        expect(tallyOf(box)).toBe("3/12");
    });

    it("Given a term the scrollback does not contain, when the box is composed, then it says so instead of showing an empty position", () => {
        // Given — `0/0` se lit comme un compteur en panne
        const state = opened().typing("nope").found(-1, 0).build();

        // When
        const box = boxOf(state, recorder());

        // Then
        expect(tallyOf(box)).toBe("no match");
    });

    it("Given more matches than the addon highlights, when the box is composed, then the total is shown without a position it no longer knows", () => {
        // Given — au-delà de son seuil, l'addon rend `resultIndex: -1` : afficher `0/2483`
        // dirait qu'on est à la première occurrence, ce qui est faux
        const state = opened().typing("e").beyondHighlightLimit(2483).build();

        // When
        const box = boxOf(state, recorder());

        // Then
        expect(tallyOf(box)).toBe("2483 matches");
    });

    it("Given a box just opened, when it is composed, then the counter stays silent rather than announcing no match", () => {
        // Given — rien n'a encore été cherché ; « no match » sur un champ vide accuserait le
        // scrollback de ne pas contenir ce qu'on n'a pas tapé
        const state = opened().build();

        // When
        const box = boxOf(state, recorder());

        // Then
        expect(tallyOf(box)).toBe("");
    });

    it("Given an empty field, when the box is composed, then the two arrows stay visible and say why they are off", () => {
        // Given — la règle de la maquette, tenue par le socle : éteint avec sa raison,
        // jamais masqué
        const state = opened().build();

        // When
        const steps = stepsOf(boxOf(state, recorder()));

        // Then
        expect(steps).toHaveLength(2);
        expect(steps.map((step) => step.attrs["disabled"])).toEqual(["", ""]);
        expect(steps.map((step) => step.attrs["title"])).toEqual([
            "type something to search the scrollback",
            "type something to search the scrollback",
        ]);
    });

    it("Given a term that matches something, when the box is composed, then the two arrows are live", () => {
        // Given
        const state = opened().typing("todo").found(0, 4).build();

        // When
        const steps = stepsOf(boxOf(state, recorder()));

        // Then
        expect(steps.map((step) => step.attrs["disabled"])).toEqual([undefined, undefined]);
    });

    it("Given a box with matches, when Enter is pressed with and without Shift, then the search walks both ways", () => {
        // Given — c'est le geste de la tâche : `⏎` en avant, `⇧⏎` en arrière
        const actions = recorder();
        const box = boxOf(opened().typing("todo").found(0, 4).build(), actions);
        const input = find(box, "terminal-search-field");

        // When
        input?.on["keydown"]?.({ value: "todo", key: "Enter", shiftKey: false });
        input?.on["keydown"]?.({ value: "todo", key: "Enter", shiftKey: true });

        // Then
        expect(actions.gestures).toEqual(["next", "previous"]);
    });

    it("Given a box being typed into, when Escape is pressed, then the box closes and nothing else has been triggered", () => {
        // Given — `⎋` referme ; c'est le contrôleur qui rend ensuite le focus au terminal
        const actions = recorder();
        const box = boxOf(opened().typing("todo").found(0, 4).build(), actions);
        const input = find(box, "terminal-search-field");

        // When
        input?.on["keydown"]?.({ value: "todo", key: "Escape", shiftKey: false });

        // Then
        expect(actions.gestures).toEqual(["close"]);
    });

    it("Given a character typed into the field, when the box reacts, then it searches and sends nothing else anywhere", () => {
        // Given — tant que le champ a le focus, **rien ne part vers le PTY** : la boîte ne
        // connaît aucun pont, et son seul geste sur une frappe est de chercher. La table de
        // saisie de `key-bindings.ts` n'est jamais consultée pour ce qu'on tape ici, parce
        // que xterm.js n'a plus le focus
        const actions = recorder();
        const box = boxOf(opened().build(), actions);

        // When
        find(box, "terminal-search-field")?.on["input"]?.({
            value: "tod",
            key: "d",
            shiftKey: false,
        });

        // Then
        expect(actions.gestures).toEqual(["search:tod"]);
    });

    it("Given a box, when its close button is clicked, then it closes like Escape does", () => {
        // Given — la souris et le clavier referment par le même chemin
        const actions = recorder();
        const box = boxOf(opened().typing("todo").build(), actions);

        // When
        find(box, "terminal-search-close")?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(actions.gestures).toEqual(["close"]);
    });
});
