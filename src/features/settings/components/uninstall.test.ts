import { describe, expect, it } from "bun:test";

import { findAll, plainText } from "@/shared/ui";

import type { PlannedRemoval, RemovalPlan, RemovalReport } from "../contract";
import { uninstallScreen, type RemovalStage, type UninstallActions } from "./uninstall";

/** Test Data Builder : un fichier de l'annonce, dont on ne dit que ce qui compte. */
function aPlannedFile(over: Partial<PlannedRemoval> = {}): PlannedRemoval {
    return {
        file: "/home/someone/.claude/settings.json",
        commands: ["claude"],
        entries: 5,
        deletesTheFile: false,
        handEdited: false,
        diff: '--- the file as it is\n+++ what ash would leave behind\n-  "ash-event waiting"',
        ...over,
    };
}

function aPlan(over: Partial<RemovalPlan> = {}): RemovalPlan {
    return {
        files: [aPlannedFile()],
        summary: "5 entries in /home/someone/.claude/settings.json",
        handEdited: false,
        kept: [
            "the .bak copies stay where they are.",
            "everything else ash keeps is under ~/.ash.",
        ],
        ...over,
    };
}

function aReport(over: Partial<RemovalReport> = {}): RemovalReport {
    return {
        files: [
            {
                file: "/home/someone/.claude/settings.json",
                entries: 5,
                outcome: { kind: "removed" },
            },
        ],
        summary: "removed 5 entries from 1 file",
        kept: ["the .bak copies stay where they are."],
        ...over,
    };
}

/** Ce que l'écran a demandé — la seule chose qui distingue les deux temps. */
function recorder(): { asked: string[] } & UninstallActions {
    const asked: string[] = [];
    return {
        asked,
        planRemoval: () => asked.push("plan"),
        removeEverything: () => asked.push("remove"),
        closeRemoval: () => asked.push("close"),
    };
}

/** Ce qu'un œil lirait à l'écran — les tests d'ici portent tous sur ce qui est **dit**. */
function described(stage: RemovalStage): { text: string } {
    return { text: uninstallScreen(stage, recorder()).map(plainText).join("\n") };
}

describe("l'écran « retirer ash de tous les fichiers »", () => {
    it("Given entries in two files, when the removal is announced, then every file and its entries are named before anything is written", () => {
        // Given — spec §10 : le geste dit ce qu'il va faire avant de le faire. Un bouton qui
        // écrit dans plusieurs fichiers de l'utilisateur sans les avoir nommés ne se prend
        // pas en connaissance de cause
        const plan = aPlan({
            files: [
                aPlannedFile(),
                aPlannedFile({
                    file: "/home/someone/.claude-perso/settings.json",
                    commands: ["claude-perso"],
                    entries: 5,
                }),
            ],
            summary: "10 entries in 2 files",
        });

        // When
        const shown = described({ step: "asked", plan });

        // Then
        expect(shown.text).toContain("/home/someone/.claude/settings.json");
        expect(shown.text).toContain("/home/someone/.claude-perso/settings.json");
        expect(shown.text).toContain("5 entries · claude");
        expect(shown.text).toContain("5 entries · claude-perso");
    });

    it("Given the announcement on screen, when it is described, then nothing has been written yet — only the second button writes", () => {
        // Given — les deux temps sont la règle, pas une politesse : l'annonce est une
        // lecture, et le seul geste qui écrit est celui qu'on prend devant elle
        const actions = recorder();

        // When
        const shown = uninstallScreen({ step: "asked", plan: aPlan() }, actions);
        const buttons = shown.flatMap((child) => findAll(child, "ui-button"));
        buttons.forEach((each) => each.on["click"]?.({ value: "", key: "", shiftKey: false }));

        // Then — l'écran ne demande d'écrire que sur un clic, et ce clic est le second
        expect(buttons.map(plainText)).toEqual(["← cancel", "remove ash's entries"]);
        expect(actions.asked).toEqual(["close", "remove"]);
    });

    it("Given an entry someone edited by hand, when the removal is announced, then it says so and shows the diff of what goes", () => {
        // Given — « Ash ne réécrit pas silencieusement : il signale, propose le diff, et
        // demande » (spec §10). Le retrait emporte l'entrée éditée, marqueur oblige : le
        // taire ferait perdre à l'utilisateur des lignes qu'il avait écrites lui-même
        const plan = aPlan({
            handEdited: true,
            files: [aPlannedFile({ handEdited: true, diff: '-  "mon-script --tab"' })],
        });

        // When
        const shown = described({ step: "asked", plan });

        // Then
        expect(shown.text).toContain("edited by hand");
        expect(shown.text).toContain("mon-script --tab");
    });

    it("Given a file ash created for itself, when the removal is announced, then it says the file goes with the entries", () => {
        // Given — « ce fichier disparaît » n'est pas la même promesse que « ces lignes
        // disparaissent », et c'est sur l'annonce que l'utilisateur tranche
        const plan = aPlan({ files: [aPlannedFile({ deletesTheFile: true })] });

        // When
        const shown = described({ step: "asked", plan });

        // Then
        expect(shown.text).toContain("it goes with them");
    });

    it("Given no file carrying ash's marker, when the removal is announced, then the button stays visible and dimmed with its reason", () => {
        // Given — la discipline de la maquette, répétée trois fois : le bouton reste
        // visible, éteint, avec sa raison. Le masquer ferait croire que la désinstallation
        // n'existe pas
        const plan = aPlan({
            files: [],
            summary: "nothing to remove — no file carries ash's marker",
        });

        // When
        const shown = uninstallScreen({ step: "asked", plan }, recorder());
        const write = shown
            .flatMap((child) => findAll(child, "ui-button"))
            .find((each) => plainText(each) === "remove ash's entries");

        // Then
        expect(write?.attrs["disabled"]).toBe("");
        expect(write?.attrs["title"]).toBe("nothing to remove — no file carries ash's marker");
    });

    it("Given a removal that took place, when it is reported, then it still says what ash kept", () => {
        // Given — « les .bak sont conservés », et c'est après coup qu'on a besoin de
        // l'entendre. La phrase vient du backend : la vue qui la laisserait tomber ferait
        // croire qu'il ne reste rien, alors qu'il reste la seule copie d'avant Ash
        const report = aReport();

        // When
        const shown = described({ step: "done", report });

        // Then
        expect(shown.text).toContain("the .bak copies stay where they are.");
        expect(shown.text).toContain("5 entries removed");
    });

    it("Given a file the disk refused, when the removal is reported, then it says that file was left untouched and why", () => {
        // Given — un compte rendu qui arrondit est pire que pas de compte rendu : celui-là
        // ferait croire qu'Ash a quitté un fichier qui porte encore son marqueur
        const report = aReport({
            files: [
                {
                    file: "/home/someone/.claude/settings.json",
                    entries: 5,
                    outcome: { kind: "refused", why: "read-only file system" },
                },
            ],
            summary: "nothing was removed · 1 file left untouched",
        });

        // When
        const shown = described({ step: "done", report });

        // Then
        expect(shown.text).toContain("left untouched — read-only file system");
    });
});
