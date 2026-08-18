import { describe, expect, it } from "bun:test";

import { instrumentationMark } from "./instrumentation";

describe("le marqueur d'un agent reconnu mais non instrumenté", () => {
    it("Given a recognized agent whose config carries no ash marker, when its row is composed, then it says why waiting will never show", () => {
        // Given — sans cette phrase, un agent qui ne demande jamais rien se lit comme une
        // panne d'Ash (ADR-0007 : `waiting` n'a d'autre source qu'un hook)
        const agent = { command: "claude", adapter: "claude-code", instrumented: "missing" } as const;

        // When
        const mark = instrumentationMark(agent);

        // Then
        expect(mark?.title).toContain("claude");
        expect(mark?.title).toContain("waiting never will");
        expect(mark?.actionable).toBe(true);
    });

    it("Given a recognized agent whose config carries the marker, when its row is composed, then nothing is signalled", () => {
        // Given — le cas nominal : signaler quoi que ce soit ici ferait du bruit sur toutes
        // les lignes d'agent de la colonne
        const agent = { command: "claude", adapter: "claude-code", instrumented: "installed" } as const;

        // When
        const mark = instrumentationMark(agent);

        // Then
        expect(mark).toBeNull();
    });

    it("Given a tool no adapter can instrument, when its row is composed, then it says so and offers no gesture", () => {
        // Given — `generic` ne pose aucun hook (ADR-0008). Un geste mènerait à un bouton
        // éteint, ce qui se lit comme un défaut plutôt que comme une limite
        const agent = { command: "kimi", adapter: "generic", instrumented: "unsupported" } as const;

        // When
        const mark = instrumentationMark(agent);

        // Then
        expect(mark?.actionable).toBe(false);
        expect(mark?.title).toContain("no adapter for kimi");
    });

    it("Given a tab that runs no recognized tool, when its row is composed, then it carries no mark", () => {
        // Given — un shell à son invite, ou un `vim` : un onglet n'est pas un agent
        // (ADR-0006)
        // When
        const mark = instrumentationMark(null);

        // Then
        expect(mark).toBeNull();
    });
});
