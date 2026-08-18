import { describe, expect, it } from "bun:test";

import { PinBuilder, TabBuilder } from "@/shared/ipc/builders";
import { pinMark, worktreeGesture } from "./pinning";
import { buildSidebar, type WorktreeNode } from "./tree";

const worktreeOf = (node: ReturnType<typeof buildSidebar>): WorktreeNode => {
    const group = node.groups[0];
    if (group === undefined) throw new Error("la colonne n'a aucune ligne");
    return group.kind === "repo" ? (group.worktrees[0] as WorktreeNode) : group.worktree;
};

describe("le geste d'une ligne de worktree", () => {
    it("Given a pinned worktree without a single tab, when its row is clicked, then it opens a tab instead of collapsing nothing", () => {
        // Given — la ligne que l'épingle fait exister (spec §5.2)
        const tree = buildSidebar([], {
            activeTabId: null,
            collapsed: new Set(),
            pinned: [PinBuilder.create("/wt/ash-sidebar").ofRepo("ash").build()],
        });

        // When
        const gesture = worktreeGesture(worktreeOf(tree));

        // Then
        expect(gesture).toBe("open-tab");
    });

    it("Given a worktree that hosts a tab, when its row is clicked, then it still collapses", () => {
        // Given — le geste historique, que l'épingle ne remplace pas
        const tree = buildSidebar([TabBuilder.create().inFlatWorktree("/dev/ash").build()], {
            activeTabId: null,
            collapsed: new Set(),
            pinned: [PinBuilder.create("/dev/ash").build()],
        });

        // When
        const gesture = worktreeGesture(worktreeOf(tree));

        // Then — épinglée ou non, une ligne habitée replie ses onglets
        expect(gesture).toBe("toggle-collapsed");
    });
});

describe("l'épingle d'une ligne", () => {
    it("Given a pinned worktree, when its mark is composed, then its gesture asks to unpin", () => {
        // Given
        const tree = buildSidebar([TabBuilder.create().inFlatWorktree("/dev/ash").build()], {
            activeTabId: null,
            collapsed: new Set(),
            pinned: [PinBuilder.create("/dev/ash").build()],
        });

        // When
        const mark = pinMark(worktreeOf(tree));

        // Then — le geste demande l'**inverse** de l'état affiché : une épingle qu'on ne
        // pourrait plus retirer serait une écriture définitive faite d'un clic
        expect(mark.pin).toBe(false);
        expect(mark.title).toContain("unpin");
    });

    it("Given a worktree that is not pinned, when its mark is composed, then its gesture asks to pin", () => {
        // Given
        const tree = buildSidebar([TabBuilder.create().inFlatWorktree("/dev/ash").build()], {
            activeTabId: null,
            collapsed: new Set(),
            pinned: [],
        });

        // When
        const mark = pinMark(worktreeOf(tree));

        // Then
        expect(mark.pin).toBe(true);
        expect(mark.title).toContain("pin");
    });
});
