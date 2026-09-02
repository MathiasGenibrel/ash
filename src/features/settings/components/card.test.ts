import { describe, expect, it } from "bun:test";

import { FOCUS_KEY, find, findAll, plainText, text } from "@/shared/ui";

import { aSnapshot, aTool, aVerification } from "../builders";
import { toolCard, type CardActions, type CardContext } from "./card";

function recorder(): { asked: string[] } & CardActions {
    const asked: string[] = [];
    return {
        asked,
        forgetTool: (command) => asked.push(`forget ${command}`),
        typePath: (command, value) => asked.push(`type ${command} ${value}`),
        commitPath: (command) => asked.push(`commit ${command}`),
        selectAdapter: (command, adapter) => asked.push(`adapter ${command} ${adapter}`),
        verifyTool: (command) => asked.push(`verify ${command}`),
        resetTool: (command) => asked.push(`reset ${command}`),
        undoReset: (command) => asked.push(`undo ${command}`),
        applyFix: (command) => asked.push(`fix ${command}`),
        focusPath: (command) => asked.push(`focus ${command}`),
        installHooks: (command) => asked.push(`install ${command}`),
        removeHooks: (command) => asked.push(`remove ${command}`),
        openConflict: (command) => asked.push(`open ${command}`),
    };
}

function context(edits: ReadonlyMap<string, string> = new Map()): CardContext {
    const snapshot = aSnapshot();
    return { adapters: snapshot.adapters, tests: snapshot.tests, edits };
}

describe("la carte d'une entrée déclarée", () => {
    it("Given an entry that is valid but doubled by another, when its card is described, then the duplicate is what the border says", () => {
        // Given — une entrée valide qu'une autre double n'écrira rien : c'est ça qu'il faut
        // voir en premier, pas le vert de sa vérification
        const tool = aTool({ duplicates: ["claude-perso"], verification: aVerification("valid") });

        // When
        const described = toolCard(tool, context(), recorder()).build();

        // Then
        expect(described.classes).toEqual(["settings-card", "is-duplicate"]);
        expect(plainText(find(described, "settings-duplicate-tag") ?? described)).toBe(
            "duplicate · also claude-perso",
        );
    });

    it("Given a folder being typed into, when the card is redrawn, then the field shows what is typed and carries the key that gets the cursor back", () => {
        // Given — la relance à 400 ms refait la carte pendant la frappe. Sans la clé, le
        // curseur partirait avec l'ancien élément, au milieu d'un mot
        const tool = aTool({ config: "~/.claude" });
        const edits = new Map([["claude", "~/.claude/pro"]]);

        // When
        const field = find(toolCard(tool, context(edits), recorder()).build(), "settings-path");

        // Then
        expect(field?.attrs["value"]).toBe("~/.claude/pro");
        expect(field?.attrs[FOCUS_KEY]).toBe("path:claude");
    });

    it("Given a folder field, when Enter is pressed, then the wait is cut short and nothing is submitted on the user's behalf", () => {
        // Given — `⏎` dit « j'ai fini de taper » et rien d'autre (ADR-0015) : il abrège les
        // 400 ms, il ne valide pas
        const actions = recorder();
        const field = find(toolCard(aTool(), context(), actions).build(), "settings-path");

        // When
        field?.on["input"]?.({ value: "~/.claude/pro", key: "o", shiftKey: false });
        field?.on["keydown"]?.({ value: "~/.claude/pro", key: "Enter", shiftKey: false });

        // Then
        expect(actions.asked).toEqual(["type claude ~/.claude/pro", "commit claude"]);
    });

    it("Given an entry that never proved a folder, when its reset button is described, then it stays visible, off, and says why", () => {
        // Given — « le masquer ferait croire que le geste n'existe pas ». La raison est celle
        // de `describeReset`, et la carte ne la réinvente pas
        const tool = aTool({ lastValidConfig: null });

        // When
        const reset = findAll(
            toolCard(tool, context(), recorder()).build(),
            "settings-icon-button",
        );

        // Then
        expect(plainText(reset[0] ?? text(""))).toBe("↺");
        expect(reset[0]?.attrs["disabled"]).toBe("");
        expect(reset[0]?.attrs["aria-label"]).toBe(
            "reset claude: no verified folder to go back to yet",
        );
    });

    it("Given an entry that can go back to a folder that worked, when its reset button is described, then it names that folder rather than staying silent", () => {
        // Given — un bouton allumé ne passe pas par `disabled(reason)`, donc rien ne pose sa
        // raison à sa place : `back to ~/.claude` est le seul endroit visible où le dossier
        // de destination est nommé, et il disparaîtrait au moment où il sert
        const tool = aTool({ config: "~/.claude/pro", lastValidConfig: "~/.claude" });

        // When
        const reset = findAll(
            toolCard(tool, context(), recorder()).build(),
            "settings-icon-button",
        );

        // Then
        expect(reset[0]?.attrs["disabled"]).toBeUndefined();
        expect(reset[0]?.attrs["title"]).toBe("back to ~/.claude");
    });

    it("Given an entry that has not been verified since it changed, when its card is described, then the dot says no hooks are written for it", () => {
        // Given — tant qu'une entrée n'a pas prouvé son dossier, elle vit en mémoire. La
        // pastille est la seule chose qui le dit
        const tool = aTool({ verification: aVerification("invalid"), verified: false });

        // When
        const dot = find(toolCard(tool, context(), recorder()).build(), "settings-unsaved");

        // Then
        expect(dot?.attrs["aria-label"]).toBe("not verified — no hooks written for this entry");
    });

    it("Given an entry whose adapter is picked from the menu, when the menu changes, then the card asks for the entry to be retargeted", () => {
        // Given — un contrôle qu'on peut bouger sans que rien ne re-juge dirait qu'Ash a
        // accepté le nouvel adaptateur
        const actions = recorder();
        const described = toolCard(aTool(), context(), actions).build();

        // When
        find(described, "settings-card-adapter")?.on["change"]?.({
            value: "claude-code",
            key: "",
            shiftKey: false,
        });

        // Then
        expect(actions.asked).toEqual(["adapter claude claude-code"]);
    });

    it("Given an entry that was just reset, when its card is described, then the folder it replaced can still be brought back", () => {
        // Given — la ligne `was` n'existe que juste après une réinitialisation, et elle est
        // indépendante de l'étiquette de doublon (§7.3)
        const actions = recorder();
        const tool = aTool({ resetFrom: "~/.claude/old", lastValidConfig: "~/.claude" });

        // When
        const was = find(toolCard(tool, context(), actions).build(), "settings-was");
        find(was ?? text(""), "settings-link")?.on["click"]?.({
            value: "",
            key: "",
            shiftKey: false,
        });

        // Then
        expect(plainText(find(was ?? text(""), "settings-was-path") ?? text(""))).toBe(
            "~/.claude/old",
        );
        expect(actions.asked).toEqual(["undo claude"]);
    });

    it("Given a card, when its three rows are described, then the grid keeps a label cell for every row that has one", () => {
        // Given — le corps est une grille `44px 1fr` : une ligne qui perdrait sa cellule de
        // libellé décalerait tout ce qui la suit d'une colonne
        const described = toolCard(aTool(), context(), recorder()).build();

        // When
        const keys = findAll(described, "settings-card-key").map(plainText);

        // Then
        expect(keys).toEqual(["config", "test", "hooks"]);
    });
});
