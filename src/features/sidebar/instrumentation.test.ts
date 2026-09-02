import { describe, expect, it } from "bun:test";

import { instrumentationMark } from "./instrumentation";

describe("le marqueur d'un agent reconnu mais non instrumenté", () => {
    it("Given a recognized agent whose config carries no ash marker, when its row is composed, then it says why waiting will never show", () => {
        // Given — sans cette phrase, un agent qui ne demande jamais rien se lit comme une
        // panne d'Ash (ADR-0007 : `waiting` n'a d'autre source qu'un hook)
        const agent = {
            command: "claude",
            adapter: "claude-code",
            instrumented: "missing",
        } as const;

        // When
        const mark = instrumentationMark(agent);

        // Then
        expect(mark?.title).toContain("claude");
        expect(mark?.title).toContain("waiting never will");
        expect(mark?.instrument).toEqual({ command: "claude", adapter: "claude-code" });
    });

    it("Given a recognized agent whose config carries the marker, when its row is composed, then nothing is signalled", () => {
        // Given — le cas nominal : signaler quoi que ce soit ici ferait du bruit sur toutes
        // les lignes d'agent de la colonne
        const agent = {
            command: "claude",
            adapter: "claude-code",
            instrumented: "installed",
        } as const;

        // When
        const mark = instrumentationMark(agent);

        // Then
        expect(mark).toBeNull();
    });

    it("Given an agent instrumented by an older ash, when its row is composed, then it names what stops coming back and offers to update", () => {
        // Given — le bloc `# ash:hook v1` pose `Stop` et `Notification`, donc `waiting`
        // remonte encore : ce qui manque est arrivé après. Ses lignes filles ne se ferment
        // jamais (#179), et la colonne n'en disait rien (#197)
        const agent = { command: "claude", adapter: "claude-code", instrumented: "outdated" } as const;

        // When
        const mark = instrumentationMark(agent);

        // Then — la conséquence avant la cause, et un geste : la sidebar informe, l'écran
        // agit (ADR-0010)
        expect(mark?.title).toContain("subagent rows never close");
        expect(mark?.instrument).toEqual({ command: "claude", adapter: "claude-code" });
    });

    it("Given an agent whose hooks are outdated and one whose hooks are missing, when both rows are composed, then the two marks do not look alike", () => {
        // Given — « pas à jour » et « rien de posé » ne se corrigent pas de la même façon,
        // et un état doit se distinguer **sans la couleur** (`shared/agent-state`)
        const outdated = { command: "claude", adapter: "claude-code", instrumented: "outdated" } as const;
        const missing = { command: "claude", adapter: "claude-code", instrumented: "missing" } as const;

        // When
        const marks = [instrumentationMark(outdated), instrumentationMark(missing)];

        // Then
        expect(marks[0]?.glyph).not.toBe(marks[1]?.glyph);
    });

    it("Given a tool no adapter can instrument, when its row is composed, then it says so and offers no gesture", () => {
        // Given — `generic` ne pose aucun hook (ADR-0008). Un geste mènerait à un bouton
        // éteint, ce qui se lit comme un défaut plutôt que comme une limite
        const agent = { command: "kimi", adapter: "generic", instrumented: "unsupported" } as const;

        // When
        const mark = instrumentationMark(agent);

        // Then
        expect(mark?.instrument).toBeNull();
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
