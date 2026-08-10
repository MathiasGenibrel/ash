import { describe, expect, it } from "bun:test";

import type { TabInfo } from "./ports";
import { tabTitle } from "./tab-bar";

const tab = (over: Partial<TabInfo> = {}): TabInfo => ({
    tabId: "A",
    cwd: "/wt/omelette-sidebar",
    process: "claude",
    state: "working",
    location: {
        worktreeRoot: "/wt/omelette-sidebar",
        worktreeName: "omelette-sidebar",
        repo: { id: "/dev/omelette-web/.git", name: "omelette-web" },
    },
    ...over,
});

describe("le titre d'un onglet", () => {
    it("Given the sidebar is open, when a tab is titled, then it shows the program that holds its foreground", () => {
        // Given / When / Then — la sidebar porte déjà le contexte ; le répéter dans chaque
        // onglet ne dirait rien de plus
        expect(tabTitle(tab(), false)).toBe("claude");
    });

    it("Given the sidebar is collapsed, when a tab is titled, then it carries its repository too", () => {
        // Given — `⌘B` : la colonne ne porte plus le contexte, donc l'onglet doit le porter
        // When / Then
        expect(tabTitle(tab(), true)).toBe("omelette-web/claude");
    });

    it("Given a tab in a repository without any linked worktree, when the sidebar is collapsed, then it carries its worktree name", () => {
        // Given — la forme à plat n'a pas de ligne de dépôt : c'est le worktree qui nomme
        // le workspace
        const flat = tab({
            location: { worktreeRoot: "/dev/ash", worktreeName: "ash", repo: null },
        });

        // When / Then
        expect(tabTitle(flat, true)).toBe("ash/claude");
    });

    it("Given a tab the backend could not locate, when the sidebar is collapsed, then it falls back to its directory instead of losing its context", () => {
        // Given
        const lost = tab({ cwd: "/dev/broken", location: null });

        // When / Then
        expect(tabTitle(lost, true)).toBe("broken/claude");
    });
});
