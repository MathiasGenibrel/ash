import { describe, expect, it } from "bun:test";

import { TabBuilder } from "@/shared/ipc/builders";
import { tabTitle } from "./tab-bar";

/** Un onglet `claude` dans un worktree lié d'`omelette-web`. */
const claudeIn = (): TabBuilder =>
    TabBuilder.create().running("claude").inWorktree("/wt/omelette-sidebar", "omelette-web");

describe("le titre d'un onglet", () => {
    it("Given the sidebar is open, when a tab is titled, then it shows the program that holds its foreground", () => {
        // Given / When / Then — la sidebar porte déjà le contexte ; le répéter dans chaque
        // onglet ne dirait rien de plus
        expect(tabTitle(claudeIn().build(), false)).toBe("claude");
    });

    it("Given the sidebar is collapsed, when a tab is titled, then it carries its repository too", () => {
        // Given — `⌘B` : la colonne ne porte plus le contexte, donc l'onglet doit le porter
        // When / Then
        expect(tabTitle(claudeIn().build(), true)).toBe("omelette-web/claude");
    });

    it("Given a tab in a repository without any linked worktree, when the sidebar is collapsed, then it carries its worktree name", () => {
        // Given — la forme à plat n'a pas de ligne de dépôt : c'est le worktree qui nomme
        // le contexte
        const flat = TabBuilder.create().running("claude").inFlatWorktree("/dev/ash").build();

        // When / Then
        expect(tabTitle(flat, true)).toBe("ash/claude");
    });

    it("Given a tab the backend could not locate, when the sidebar is collapsed, then it falls back to its directory instead of losing its context", () => {
        // Given
        const lost = TabBuilder.create().running("claude").unlocated("/dev/broken").build();

        // When / Then
        expect(tabTitle(lost, true)).toBe("broken/claude");
    });
});
