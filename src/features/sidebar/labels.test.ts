import { describe, expect, it } from "bun:test";

import { abbreviate, newTabHint, truncate } from "./labels";

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

describe("ce que le pied de la colonne dit du raccourci", () => {
    it("Given new tab has been rebound, when the foot is written, then it announces the combination in force", () => {
        // Given — le pied annonçait `⌘T` en dur : il mentait dès qu'on déplaçait le
        // raccourci. La combinaison vient désormais du backend, qui la détient
        // ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md))
        const shown = newTabHint("⌘J");

        // Then — et l'infobulle la porte aussi : c'est elle qu'on lit à la souris
        expect(shown.hint).toBe("⌘J");
        expect(shown.title).toBe("Nouvel onglet dans le worktree courant (⌘J)");
    });

    it("Given new tab has no shortcut at all, when the foot is written, then it shows nothing rather than a placeholder", () => {
        // Given — `⌫` retire le raccourci sans en mettre d'autre. Un tiret ou un « aucun »
        // se lirait comme une touche, et l'infobulle n'a pas à porter une parenthèse vide
        const shown = newTabHint("");

        // Then — le bouton reste, et c'est lui qui fait l'action
        expect(shown.hint).toBe("");
        expect(shown.title).toBe("Nouvel onglet dans le worktree courant");
    });
});
