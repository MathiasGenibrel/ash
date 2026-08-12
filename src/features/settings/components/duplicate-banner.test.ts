import { describe, expect, it } from "bun:test";

import { find, plainText, text } from "@/shared/ui";

import { aTool } from "../builders";
import { duplicateBanner } from "./duplicate-banner";

describe("la bannière de doublon", () => {
    it("Given two entries pointing at the same folder, when the section is described, then one banner speaks for both", () => {
        // Given — le doublon n'appartient à aucune des deux cartes, il est de section (§3.7)
        const tools = [
            aTool({ command: "claude", config: "~/.claude", duplicates: ["claude-perso"] }),
            aTool({ command: "claude-perso", config: "~/.claude", duplicates: ["claude"] }),
        ];

        // When
        const banners = duplicateBanner(tools, { undoReset: () => undefined });

        // Then
        expect(banners).toHaveLength(1);
        expect(plainText(banners[0] ?? text(""))).toContain(
            "claude and claude-perso point at the same folder",
        );
    });

    it("Given a collision no reset created, when the banner is described, then it offers nothing to undo", () => {
        // Given — proposer d'annuler un geste qui n'a pas eu lieu ferait chercher lequel
        const tools = [
            aTool({ command: "claude", duplicates: ["claude-perso"] }),
            aTool({ command: "claude-perso", duplicates: ["claude"] }),
        ];

        // When
        const banner = duplicateBanner(tools, { undoReset: () => undefined })[0];

        // Then
        expect(find(banner ?? text(""), "ui-button")).toBeNull();
    });

    it("Given a collision a reset just created, when the undo is pressed, then it is that entry which is brought back", () => {
        // Given — c'est l'entrée réinitialisée qui a produit la collision, pas l'autre
        const asked: string[] = [];
        const tools = [
            aTool({ command: "claude", duplicates: ["claude-perso"] }),
            aTool({
                command: "claude-perso",
                duplicates: ["claude"],
                resetFrom: "~/.claude/perso",
            }),
        ];

        // When
        const banner = duplicateBanner(tools, {
            undoReset: (command) => asked.push(command),
        })[0];
        find(banner ?? text(""), "ui-button")?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(asked).toEqual(["claude-perso"]);
    });

    it("Given a single entry, when the section is described, then there is no banner at all", () => {
        // Given — `duplicates` peut être non vide sur une entrée seule le temps d'un
        // instantané ; une bannière qui ne nomme qu'une entrée ne dit rien
        const tools = [aTool({ duplicates: ["claude-perso"] })];

        // When
        const banners = duplicateBanner(tools, { undoReset: () => undefined });

        // Then
        expect(banners).toEqual([]);
    });
});
