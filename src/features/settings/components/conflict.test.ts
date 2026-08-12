import { describe, expect, it } from "bun:test";

import { findAll, plainText } from "@/shared/ui";

import { aHooksReport, aTool } from "../builders";
import { conflictScreen } from "./conflict";

const conflicting = aTool({
    hooks: aHooksReport({
        state: "conflict",
        summary: "the ash block was edited by hand",
        note: "ash writes nothing in this file until the block is settled.",
        file: "/home/someone/.claude/settings.json",
        action: "seeTheDiff",
        diff: "-  \"ash-event done\"\n+  \"ash-event done --quiet\"",
        backup: null,
    }),
});

describe("l'écran de conflit", () => {
    it("Given a hook block edited by hand, when the conflict screen is described, then the only thing it offers is to leave", () => {
        // Given — la spec §10 demande de signaler, de proposer le diff, et de demander. Ni
        // `replace` ni `merge` : l'un écraserait les lignes de l'utilisateur, l'autre
        // écrirait hors des marqueurs (ADR-0007)
        const described = conflictScreen(conflicting, () => undefined);

        // When
        const buttons = described.flatMap((child) => findAll(child, "ui-button")).map(plainText);

        // Then
        expect(buttons).toEqual(["← back to the list"]);
    });

    it("Given a conflict, when the screen is described, then the file to open in an editor is named", () => {
        // Given — le seul geste qui reste se fait ailleurs : sans le chemin, il ne se fait
        // pas
        const said = described().join("");

        // Then
        expect(said).toContain("/home/someone/.claude/settings.json");
    });

    it("Given a conflict, when the screen is described, then it shows the refusal itself and both sides of the diff", () => {
        // Given — c'est le refus qu'on affiche, pas un message à sa place
        const said = described().join("");

        // Then
        expect(said).toContain("edited by hand");
        expect(said).toContain("− the ash block");
        expect(said).toContain('+   "ash-event done --quiet"');
    });

    it("Given the back button, when it is pressed, then the screen asks to be closed and writes nothing", () => {
        // Given — `see the diff` ouvre cet écran sans rien écrire, et en sortir n'écrit pas
        // davantage
        let closed = 0;

        // When
        const screen = conflictScreen(conflicting, () => {
            closed += 1;
        });
        screen.flatMap((child) => findAll(child, "ui-button"))[0]?.on["click"]?.({
            value: "",
            key: "",
        });

        // Then
        expect(closed).toBe(1);
    });
});

function described(): string[] {
    return conflictScreen(conflicting, () => undefined).map(plainText);
}
