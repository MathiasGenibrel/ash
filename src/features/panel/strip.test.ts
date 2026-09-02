import { describe, expect, it } from "bun:test";

import { findAll, plainText } from "@/shared/ui";

import { type BottomPanelState, type PanelView } from "./layout";
import { panelStrip } from "./strip";

/** Test Data Builder : un panneau ouvert sur le graphe, dont on surcharge le nécessaire. */
function panel(overrides: Partial<BottomPanelState> = {}): BottomPanelState {
    return { height: 220, open: true, view: "graph", ...overrides };
}

/** Les vues marquées comme montrées, dans l'ordre de la barre. */
function active(node: ReturnType<typeof panelStrip>): readonly string[] {
    return findAll(node, "is-active").map((tab) => tab.attrs["data-view"] ?? "");
}

describe("the tab strip of the bottom panel", () => {
    it("Given a closed panel, when the strip is drawn, then it still names every view and marks none of them", () => {
        // Given — la barre reste visible quand le panneau rend sa hauteur au terminal :
        // c'est la seule porte du panneau tant que les raccourcis git ne sont pas déclarés
        const closed = panel({ open: false });

        // When
        const strip = panelStrip(closed, () => undefined);

        // Then
        expect(plainText(strip)).toBe("graphworktreesconflictsbranch");
        expect(active(strip)).toEqual([]);
    });

    it("Given a panel showing the worktrees, when the strip is drawn, then that view alone is marked", () => {
        // Given
        const showing = panel({ view: "worktrees" });

        // When
        const strip = panelStrip(showing, () => undefined);

        // Then
        expect(active(strip)).toEqual(["worktrees"]);
    });

    it("Given the view the panel already shows, when its tab is clicked, then the strip asks for that same view instead of deciding to close", () => {
        // Given — recliquer la vue montrée referme le panneau, mais c'est le backend qui le
        // décide sous son verrou : une bascule calculée ici ferait de la webview le second
        // détenteur de l'ouverture, et le raccourci et le clic finiraient par diverger
        // (ADR-0009)
        const asked: PanelView[] = [];
        const strip = panelStrip(panel({ view: "graph" }), (view) => asked.push(view));

        // When
        const graph = findAll(strip, "ash-panel-tab").find(
            (tab) => tab.attrs["data-view"] === "graph",
        );
        graph?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(asked).toEqual(["graph"]);
    });
});
