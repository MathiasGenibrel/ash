import { describe, expect, it } from "bun:test";

import { findAll, plainText } from "@/shared/ui";

import { aHooksReport, aTool } from "../builders";
import { conflictScreen, type ConflictActions } from "./conflict";

const conflicting = aTool({
    hooks: aHooksReport({
        state: "conflict",
        summary: "1 hook here is not ash's",
        note: "ash wrote nothing yet. see the diff of what it would add, then choose.",
        file: "/home/someone/.claude/settings.json",
        action: "seeTheDiff",
        choices: [
            {
                action: "install",
                label: "merge, keeping every hook",
                note: "ash adds its own entries next to yours, in the same event arrays.",
            },
            {
                action: "remove",
                label: "remove ash's hooks",
                note: "the entries carrying ash's marker are taken out; yours stay.",
            },
        ],
        diff: '-  "rtk hook claude"\n+  "ash-event waiting"',
        backup: "/home/someone/.claude/settings.json.bak",
    }),
});

/** Ce que l'écran a demandé — la seule chose qui distingue les deux issues. */
function recorder(): { asked: string[] } & ConflictActions {
    const asked: string[] = [];
    return {
        asked,
        installHooks: (command) => asked.push(`install ${command}`),
        removeHooks: (command) => asked.push(`remove ${command}`),
    };
}

describe("l'écran du diff", () => {
    it("Given a file that already carries hooks of its own, when the diff screen is described, then it offers a way out instead of only a way back", () => {
        // Given — l'écran ne proposait que « ← back to the list » : montrer le conflit sans
        // rien offrir laissait l'utilisateur devant une impasse, et c'est exactement ce que
        // l'amendement du 2026-08-12 d'ADR-0007 est venu lever
        const described = conflictScreen(conflicting, recorder(), () => undefined);

        // When
        const buttons = described.flatMap((child) => findAll(child, "ui-button")).map(plainText);

        // Then
        expect(buttons).toEqual([
            "← back to the list",
            "merge, keeping every hook",
            "remove ash's hooks",
        ]);
    });

    it("Given the merge button, when it is pressed, then the write is asked for — and it is the user's click that asks", () => {
        // Given — « jamais silencieux » ne veut pas dire « jamais » : Ash montre, et
        // l'utilisateur tranche. C'est ce clic-ci qui écrit, pas Ash de lui-même
        const actions = recorder();

        // When
        const screen = conflictScreen(conflicting, actions, () => undefined);
        const buttons = screen.flatMap((child) => findAll(child, "ui-button"));
        buttons[1]?.on["click"]?.({ value: "", key: "", shiftKey: false });
        buttons[2]?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(actions.asked).toEqual(["install claude", "remove claude"]);
    });

    it("Given each choice, when the screen is described, then what it does to the file is written beside its button", () => {
        // Given — un bouton qui écrit dans le fichier de quelqu'un ne se lit pas sans savoir
        // ce qu'il y fait. Les phrases viennent du backend, seul à savoir ce qu'il préserve
        const said = described().join("");

        // Then
        expect(said).toContain("ash adds its own entries next to yours");
        expect(said).toContain("yours stay");
    });

    it("Given a conflict, when the screen is described, then the file it concerns is named", () => {
        // Given — c'est la première question devant un diff : dans quel fichier
        const said = described().join("");

        // Then
        expect(said).toContain("/home/someone/.claude/settings.json");
    });

    it("Given a conflict, when the screen is described, then it shows both sides of the write to come", () => {
        // Given — c'est le diff qu'on affiche, pas un message à sa place, et sa légende doit
        // dire dans quel sens il se lit
        const said = described().join("");

        // Then
        expect(said).toContain("− the file as it is");
        expect(said).toContain("+ what ash would write");
        expect(said).toContain('+   "ash-event waiting"');
    });

    it("Given the back button, when it is pressed, then the screen asks to be closed and writes nothing", () => {
        // Given — `see the diff` ouvre cet écran sans rien écrire, et en sortir n'écrit pas
        // davantage
        const actions = recorder();
        let closed = 0;

        // When
        const screen = conflictScreen(conflicting, actions, () => {
            closed += 1;
        });
        screen
            .flatMap((child) => findAll(child, "ui-button"))[0]
            ?.on["click"]?.({
                value: "",
                key: "",
                shiftKey: false,
            });

        // Then
        expect(closed).toBe(1);
        expect(actions.asked).toEqual([]);
    });
});

function described(): string[] {
    return conflictScreen(conflicting, recorder(), () => undefined).map(plainText);
}
