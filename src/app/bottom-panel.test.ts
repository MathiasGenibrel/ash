import { describe, expect, it } from "bun:test";

import { parseBottomPanel } from "./bottom-panel";

describe("what the backend announces about the bottom panel", () => {
    it("Given a panel as the backend serialises it, when it is read, then it crosses the boundary whole", () => {
        // Given
        const announced = { height: 260, open: true, view: "worktrees" };

        // When
        const read = parseBottomPanel(announced);

        // Then
        expect(read).toEqual({ height: 260, open: true, view: "worktrees" });
    });

    it("Given a view this webview does not know, when it is announced, then the last known panel stays", () => {
        // Given — une version d'Ash plus récente que la webview, ou un `theme.json` bricolé.
        // Ouvrir une surface dont on ne sait rien prendrait sa hauteur au terminal pour
        // montrer du vide.
        const fromLater = { height: 260, open: true, view: "blame" };

        // When
        const read = parseBottomPanel(fromLater);

        // Then — `null` veut dire « ignore », et l'appelant garde ce qu'il avait
        expect(read).toBeNull();
    });

    it("Given an announcement that is not a panel at all, when it is read, then nothing is applied", () => {
        // Given — les formes qu'une frontière rend quand quelque chose s'est mal passé
        const nonsense = [null, 42, "graph", {}, { height: 0, open: true, view: "graph" }];

        // When
        const read = nonsense.map(parseBottomPanel);

        // Then — une hauteur nulle coincerait le panneau ouvert sur rien
        expect(read).toEqual([null, null, null, null, null]);
    });
});
