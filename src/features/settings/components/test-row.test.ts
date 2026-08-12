import { describe, expect, it } from "bun:test";

import { find, findAll, plainText, type UiChild } from "@/shared/ui";

import { aTool, aVerification, FOUR_TESTS } from "../builders";
import { NOTHING_VERIFIED_YET } from "../model";
import { testDetail, testRow, type TestDetailActions } from "./test-row";

function recorder(): { asked: string[] } & TestDetailActions {
    const asked: string[] = [];
    return {
        asked,
        applyFix: (command, fix) => asked.push(`fix ${command} ${fix.kind}`),
        focusPath: (command) => asked.push(`focus ${command}`),
    };
}

/** Tout le texte d'une suite de nœuds — ce qu'un œil lirait sous la ligne `test`. */
function read(children: readonly UiChild[]): string {
    return children.map(plainText).join("");
}

describe("la ligne test", () => {
    it("Given a verification that stopped on a caveat, when the row is described, then it does not claim a test failed", () => {
        // Given — la séquence pose `stoppedAt` **aussi** sur une réserve. La vue le rendait
        // tel quel (#15) : « stopped at test 3 » à côté d'un dossier reconnu se lit comme un
        // échec. La règle est dans `describeStop`, et la ligne ne la rejoue pas
        const caveat = aVerification("caveat", {
            summary: "folder recognised · claude did not answer in time",
            stoppedAt: 3,
        });

        // When
        const described = testRow(caveat, FOUR_TESTS).build();

        // Then
        expect(findAll(described, "settings-stopped")).toHaveLength(0);
    });

    it("Given an invalid verification, when the row is described, then it says which test designates what to fix", () => {
        // Given — là, le numéro est ce qui désigne la chose à corriger
        const invalid = aVerification("invalid", { summary: "folder not found", stoppedAt: 1 });

        // When
        const described = testRow(invalid, FOUR_TESTS).build();

        // Then
        expect(plainText(find(described, "settings-stopped") ?? described)).toBe(
            "stopped at test 1",
        );
    });

    it("Given a sequence that has answered twice out of four, when the tiles are described, then the four keep their place", () => {
        // Given — une rangée qui raccourcit avec l'avancement ferait bouger la ligne sous
        // les yeux, et perdre le repère du test dont on attend la réponse
        const running = aVerification("verifying", { tests: ["passed", "running"] });

        // When
        const tiles = findAll(testRow(running, FOUR_TESTS).build(), "settings-tile");

        // Then — les deux tests sans réponse sont `pending`, pas absents
        expect(tiles.map(plainText)).toEqual(["1", "2", "3", "4"]);
        expect(tiles.map((tile) => tile.classes.at(-1))).toEqual([
            "is-passed",
            "is-running",
            "is-pending",
            "is-pending",
        ]);
    });

    it("Given a tile, when a screen reader reaches it, then it hears the test and its result rather than a number", () => {
        // Given — le chiffre seul ne dit ni de quel test il s'agit, ni ce qu'il a donné
        const tiles = findAll(testRow(NOTHING_VERIFIED_YET, FOUR_TESTS).build(), "settings-tile");

        // Then
        expect(tiles[2]?.attrs["aria-label"]).toBe(
            "test 3, the command exists in PATH: not run yet",
        );
    });
});

describe("ce qu'un résultat ajoute sous la ligne test", () => {
    it("Given a sequence waiting on the fourth test, when the detail is described, then the command ash launched by itself is readable", () => {
        // Given — c'est la contrepartie du fait qu'Ash lance un programme sans qu'on l'ait
        // tapé : ce qui part doit être lisible
        const tool = aTool({
            verification: aVerification("verifying", { launched: "claude --version" }),
        });

        // When
        const described = testDetail(tool, recorder());

        // Then
        expect(read(described)).toContain("claude --version");
    });

    it("Given a mismatch, when the detail is described, then what was expected and what was found are both there", () => {
        // Given — un écart dont on ne montre qu'une moitié ne se corrige pas
        const tool = aTool({
            verification: aVerification("invalid", {
                detail: { expected: "settings.json", found: "settings.local.json" },
            }),
        });

        // When
        const said = read(testDetail(tool, recorder()));

        // Then
        expect(said).toContain("expected: settings.json — found: settings.local.json");
    });

    it("Given a fix that would fall back to the generic adapter, when the detail is described, then the cost is written above the apply button", () => {
        // Given — `generic` est un mode dégradé, et l'écran le dit **avant** qu'on l'applique
        // (§3.6) : l'appuyer sans savoir ferait perdre `waiting` sans que rien ne l'annonce
        const tool = aTool({
            verification: aVerification("invalid", {
                fix: {
                    question: "use the generic adapter instead?",
                    apply: { kind: "useAdapter", adapter: "generic" },
                },
            }),
        });

        // When
        const said = read(testDetail(tool, recorder()));

        // Then
        expect(said).toContain("claude will show as ");
        expect(said).toContain("never ");
    });

    it("Given a refusal nothing can repair, when the detail is described, then the only offer left does not act for the user", () => {
        // Given — un dossier verrouillé ne se déverrouille pas depuis cette fenêtre. Le
        // bouton restant ramène le curseur dans le champ, il ne choisit pas de dossier
        // (ADR-0015)
        const actions = recorder();
        const tool = aTool({
            verification: aVerification("invalid", {
                fix: { question: "ash cannot read this folder — pick another one?", apply: null },
            }),
        });

        // When
        const described = testDetail(tool, actions);
        const buttons = described.flatMap((child) => findAll(child, "ui-button"));
        buttons[0]?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(buttons.map(plainText)).toEqual(["choose another folder…"]);
        expect(actions.asked).toEqual(["focus claude"]);
    });
});
