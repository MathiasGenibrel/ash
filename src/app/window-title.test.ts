import { describe, expect, it } from "bun:test";

import { MetadataBuilder, TabBuilder } from "@/shared/ipc/builders";
import { windowTitle } from "./window-title";

describe("la bande de titre de la fenêtre", () => {
    it("Given a tab in a linked worktree on a branch, when the window is titled, then it names the repository and the branch", () => {
        // Given
        const tab = TabBuilder.create()
            .running("claude")
            .inWorktree("/wt/omelette-sidebar", "omelette-web")
            .build();
        const metadata = MetadataBuilder.create().onBranch("feat/agent-sidebar").build();

        // When
        const title = windowTitle({ tab, metadata });

        // Then
        expect(title).toBe("ash — omelette-web / feat/agent-sidebar");
    });

    it("Given a repository without any linked worktree, when the window is titled, then the worktree names the place", () => {
        // Given — la forme à plat d'ADR-0012 n'a pas de ligne de dépôt
        const tab = TabBuilder.create().inFlatWorktree("/dev/ash").build();
        const metadata = MetadataBuilder.create().onBranch("main").build();

        // When
        const title = windowTitle({ tab, metadata });

        // Then
        expect(title).toBe("ash — ash / main");
    });

    it("Given a tab opened at ~ before any cd, when the window is titled, then it says where it is without inventing a branch", () => {
        // Given — hors de tout dépôt : le backend situe le répertoire, la surveillance n'a
        // aucune branche à en dire
        const tab = TabBuilder.create().inFlatWorktree("/Users/mathias").build();

        // When
        const title = windowTitle({ tab, metadata: null });

        // Then — un titre plus court, jamais un titre vide : il s'allongera d'une branche
        // au premier `cd` dans un dépôt
        expect(title).toBe("ash — mathias");
    });

    it("Given a tab the backend could not locate, when the window is titled, then it falls back to its directory instead of losing its context", () => {
        // Given — un `.git` cassé, un dépôt disparu : la localisation est absente, pas vide
        const tab = TabBuilder.create().unlocated("/dev/broken").build();

        // When
        const title = windowTitle({ tab, metadata: null });

        // Then
        expect(title).toBe("ash — broken");
    });

    it("Given a stopped rebase, when the window is titled, then it keeps the branch the rebase moves instead of the detached HEAD", () => {
        // Given
        const tab = TabBuilder.create().inWorktree("/wt/omelette-sidebar", "omelette-web").build();
        const metadata = MetadataBuilder.create().rebasing("feat/toc", "main", 2, 5).build();

        // When
        const title = windowTitle({ tab, metadata });

        // Then
        expect(title).toBe("ash — omelette-web / feat/toc");
    });

    it("Given a detached HEAD outside any operation, when the window is titled, then the commit is unmistakable for a branch name", () => {
        // Given
        const tab = TabBuilder.create().inFlatWorktree("/dev/ash").build();
        const metadata = MetadataBuilder.create().detachedAt("a1b2c3d").build();

        // When
        const title = windowTitle({ tab, metadata });

        // Then
        expect(title).toBe("ash — ash / @a1b2c3d");
    });

    it("Given no tab at all, when the window is titled, then it carries the application alone", () => {
        // Given / When
        const title = windowTitle(null);

        // Then
        expect(title).toBe("ash");
    });
});
