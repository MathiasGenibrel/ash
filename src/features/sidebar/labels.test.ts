import { describe, expect, it } from "bun:test";

import { abbreviate, truncate } from "./labels";

describe("la lisibilité à 240 px", () => {
    it("Given a name longer than the column, when it is truncated, then it keeps its beginning and ends with an ellipsis", () => {
        // Given / When
        const cut = truncate("feat/a-really-long-branch-name-here", 12);

        // Then — c'est le début d'un nom qui l'identifie
        expect(cut).toBe("feat/a-real…");
    });

    it("Given a name that fits, when it is truncated, then it is left untouched", () => {
        // Given / When / Then
        expect(truncate("claude", 12)).toBe("claude");
    });
});

describe("le rail replié", () => {
    it("Given a project name, when the collapsed rail abbreviates it, then it keeps two letters that still tell projects apart", () => {
        // Given / When / Then — `⌘B` réduit la colonne à 46 px : deux lettres sont tout ce
        // qui reste pour reconnaître un projet en vision périphérique
        expect(abbreviate("omelette-web")).toBe("ow");
        expect(abbreviate("ash-core")).toBe("ac");
        expect(abbreviate("ash")).toBe("as");
    });
});
