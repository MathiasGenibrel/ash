import { describe, expect, it } from "bun:test";

import type { Subagent } from "@/shared/ipc";
import {
    ANONYMOUS_SUBAGENT,
    composeSubagentRow,
    MAX_SUBAGENT_LABEL,
    subagentNodes,
} from "./subagents";

/** Test Data Builder : un enfant tel que le backend le décrirait. */
function child(overrides: Partial<Subagent> = {}): Subagent {
    return {
        agentId: "agent-7",
        agentType: "explore",
        state: "working",
        since: 0,
        ...overrides,
    };
}

describe("ce qu'une ligne de sous-agent montre", () => {
    it("Given a subagent that has been working for a quarter of an hour, when its row is composed, then it reads the elapsed time from the entry date", () => {
        // Given — le backend envoie une **date** d'entrée, une seule fois ; la durée est un
        // fait d'affichage. Si elle traversait la frontière, la fiche de chaque onglet
        // portant un enfant changerait à chaque seconde.
        const [node] = subagentNodes([child({ since: 1_000_000 })]);

        // When
        const row = composeSubagentRow(node!, 1_000_000 + 922_000);

        // Then
        expect(row).toEqual({
            label: "explore",
            title: "explore",
            state: "working",
            elapsed: "15m22s",
        });
    });

    it("Given a subagent whose type the tool never named, when its row is composed, then the row still exists under a generic name", () => {
        // Given — rien ne garantit qu'un outil donne un `agentType`. Masquer l'enfant
        // effacerait un travail qui a bien lieu ; son `agentId` le distingue de ses frères,
        // et c'est seulement son libellé qui manque.
        const nodes = subagentNodes([child({ agentType: null })]);

        // When
        const row = composeSubagentRow(nodes[0]!, 0);

        // Then
        expect(row.label).toBe(ANONYMOUS_SUBAGENT);
    });

    it("Given a subagent whose type is longer than the column is wide, when its row is composed, then the name is cut and the whole of it stays in the tooltip", () => {
        // Given — la colonne fait 240 px, et une ligne fille est indentée d'un niveau de
        // plus. `Explore` tient ; un nom de sous-agent conventionnel, non.
        const long = "dev-integration-with-a-very-long-name";
        const nodes = subagentNodes([child({ agentType: long })]);

        // When
        const row = composeSubagentRow(nodes[0]!, 0);

        // Then
        expect(row.label.length).toBe(MAX_SUBAGENT_LABEL);
        expect(row.label.endsWith("…")).toBe(true);
        expect(row.title).toBe(long);
    });

    it("Given an entry date that is still in the future, when the row is composed, then it writes no duration at all", () => {
        // Given — une horloge recalée entre le backend et le rendu. Écrire `-3s` serait pire
        // que ne rien écrire.
        const nodes = subagentNodes([child({ since: 5_000 })]);

        // When
        const row = composeSubagentRow(nodes[0]!, 1_000);

        // Then
        expect(row.elapsed).toBeNull();
    });
});
