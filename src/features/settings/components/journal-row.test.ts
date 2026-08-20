import { describe, expect, it } from "bun:test";

import { findAll, plainText, type UiChild, type UiElementNode } from "@/shared/ui";

import { aJournalReport } from "../builders";
import { journalRow, type JournalActions } from "./journal-row";

function said(child: UiChild): string {
    return plainText(child);
}

function buttons(child: UiChild): readonly UiElementNode[] {
    return findAll(child, "ui-button");
}

const IDLE: JournalActions = { purgeJournal: () => undefined };

describe("la ligne du journal d'attribution", () => {
    it("Given a journal that has attributed commits, when the row is composed, then the button says what the click takes away", () => {
        // Given — le geste n'a pas d'écran d'annonce, contrairement au retrait des hooks :
        // c'est le bouton lui-même qui dit ce qui partira, sans quoi le clic exigé par la
        // spec §10 ne serait pas explicite
        const filled = aJournalReport({
            entries: 12,
            repos: 3,
            summary: "12 commits attributed, in 3 repositories",
        });

        // When
        const composed = journalRow(filled, IDLE);

        // Then
        expect(said(composed)).toContain("12 commits attributed, in 3 repositories");
        expect(buttons(composed)[0]?.attrs["disabled"]).toBeUndefined();
    });

    it("Given a journal that has recorded nothing, when the row is composed, then the button stays visible and off with its reason", () => {
        // Given — la promesse de la spec §10 ne dépend pas de ce qu'il y a dans le fichier.
        // Masquer le bouton apprendrait à ne plus chercher la purge le jour où elle sert.
        const empty = aJournalReport();

        // When
        const composed = journalRow(empty, IDLE);

        // Then
        const purge = buttons(composed)[0];
        expect(purge).toBeDefined();
        expect(purge?.attrs["disabled"]).toBe("");
        expect(purge?.attrs["title"]).toContain("not attributed any commit");
    });

    it("Given the journal row, when it is composed, then it shows where the file lives and what it never does", () => {
        // Given — le fichier contient des prompts. Cette ligne en dit le poids, l'endroit et
        // la promesse ; elle n'en lit aucun, et rien du contrat ne lui en donne un.
        const filled = aJournalReport({ entries: 3, repos: 1 });

        // When
        const composed = journalRow(filled, IDLE);

        // Then
        expect(said(composed)).toContain("~/.ash/journal");
        expect(said(composed)).toContain("not synced");
    });
});
