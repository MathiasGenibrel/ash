import { describe, expect, it } from "bun:test";

import { find, findAll, plainText, text } from "@/shared/ui";

import { aHooksReport, aTool } from "../builders";
import { hooksNote, hooksRow, type HooksRowActions } from "./hooks-row";

/** Ce que la ligne a demandé — la seule chose qui distingue les trois gestes du bouton. */
function recorder(): { asked: string[] } & HooksRowActions {
    const asked: string[] = [];
    return {
        asked,
        installHooks: (command) => asked.push(`install ${command}`),
        removeHooks: (command) => asked.push(`remove ${command}`),
        openConflict: (command) => asked.push(`open ${command}`),
    };
}

describe("la ligne hooks d'une carte", () => {
    it("Given a blocked hooks line whose refusal already names its file, when the row is described, then the file is not written twice", () => {
        // Given — la vue supprimait ce nom de fichier de sa propre initiative (#16). La
        // règle appartient à la table de présentation, et la ligne ne fait que l'appliquer
        const tool = aTool({
            hooks: aHooksReport({
                state: "blocked",
                summary: "ash can't read /home/someone/.claude/settings.json",
                file: "/home/someone/.claude/settings.json",
                action: "install",
                enabled: false,
            }),
        });

        // When
        const described = hooksRow(tool, recorder()).build();

        // Then — la phrase du backend est intacte, et rien ne la double
        expect(plainText(find(described, "settings-hooks-reason") ?? described)).toBe(
            "ash can't read /home/someone/.claude/settings.json",
        );
        expect(findAll(described, "settings-hooks-file")).toHaveLength(0);
    });

    it("Given an installed line, when the row is described, then the file the backend names is shown beside its phrase", () => {
        // Given — l'écran ne cache pas ce que le backend envoie : c'est la première question
        // qu'on se pose devant une ligne installée
        const tool = aTool();

        // When
        const described = hooksRow(tool, recorder()).build();

        // Then
        expect(plainText(find(described, "settings-hooks-file") ?? described)).toBe(
            "/home/someone/.claude/settings.json",
        );
    });

    it("Given a hooks line the backend refuses to act on, when the row is described, then the button stays in place, off, with the reason", () => {
        // Given — « le masquer ferait croire que ça n'existe pas ». Le droit d'écrire est
        // calculé en Rust (ADR-0007) et n'est jamais rejoué ici
        const tool = aTool({
            hooks: aHooksReport({
                state: "missing",
                summary: "verify the entry first",
                action: "install",
                enabled: false,
            }),
        });

        // When
        const button = find(hooksRow(tool, recorder()).build(), "ui-button");

        // Then
        expect(plainText(button ?? text(""))).toBe("install");
        expect(button?.attrs["disabled"]).toBe("");
        expect(button?.attrs["title"]).toBe("verify the entry first");
    });

    it("Given a hooks line in conflict, when its button is pressed, then the diff is opened and nothing is written", () => {
        // Given — c'est le seul des quatre gestes de la ligne qui n'écrive pas dans un
        // fichier de l'utilisateur, et le confondre avec `install` serait la seule faute de
        // cette ligne qu'on ne pourrait pas défaire
        const actions = recorder();
        const tool = aTool({
            hooks: aHooksReport({ state: "conflict", action: "seeTheDiff", diff: "-a\n+b" }),
        });

        // When
        find(hooksRow(tool, actions).build(), "ui-button")?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(actions.asked).toEqual(["open claude"]);
    });

    it("Given a line about to write, when the row is described, then the diff can be opened before anything is written", () => {
        // Given — « un bouton ouvre le diff de ce qu'Ash écrirait, sur le fichier tel qu'il
        // est, avant toute écriture ». Ça ne vaut pas que pour un conflit : `install` écrit
        // aussi, et doit pouvoir dire ce qu'il écrira (ADR-0007, amendement du 2026-08-12)
        const actions = recorder();
        const tool = aTool({
            hooks: aHooksReport({
                state: "missing",
                summary: "no ash hooks in this file",
                action: "install",
                diff: "-a\n+b",
            }),
        });

        // When
        const buttons = findAll(hooksRow(tool, actions).build(), "ui-button");
        buttons[0]?.on["click"]?.({ value: "", key: "" });

        // Then — le diff est le premier des deux, et c'est lui qui n'écrit rien
        expect(buttons.map(plainText)).toEqual(["see the diff", "install"]);
        expect(actions.asked).toEqual(["open claude"]);
    });

    it("Given a hooks line whose action removes the block, when its button is pressed, then removal is what is asked for", () => {
        // Given — les trois autres actions passent par `installHooks` ; `remove` est la
        // seule qui retire, et elle n'est pas primaire à l'écran pour cette raison
        const actions = recorder();

        // When
        find(hooksRow(aTool(), actions).build(), "ui-button")?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(actions.asked).toEqual(["remove claude"]);
    });
});

describe("la prose sous la ligne hooks", () => {
    it("Given an action that will copy the file first, when the note is described, then the copy is announced before the gesture", () => {
        // Given — « bloc délimité, sauvegarde, jamais silencieux » (ADR-0007). La sauvegarde
        // annoncée après coup ne serait plus une promesse, seulement un constat
        const hooks = aHooksReport({
            note: "install writes the ash block between its markers.",
            backup: "/home/someone/.claude/settings.json.bak",
        });

        // When
        const said = plainText(hooksNote(hooks));

        // Then
        expect(said).toBe(
            "install writes the ash block between its markers.before writing: /home/someone/.claude/settings.json.bak",
        );
    });

    it("Given an action that copies nothing, when the note is described, then no backup is promised", () => {
        // Given — promettre une copie qui n'aura pas lieu est pire que de n'en promettre
        // aucune
        const hooks = aHooksReport({ note: "nothing to write here.", backup: null });

        // When
        const said = plainText(hooksNote(hooks));

        // Then
        expect(said).toBe("nothing to write here.");
    });
});
