import { describe, expect, it } from "bun:test";

import type { Branch, BranchGroup, BranchOverview, BusyAgent } from "@/shared/ipc";

import { keepSelection, moveSelection, selectedBranch, visibleRows } from "./branch-list";

/**
 * Ce qui se décide côté webview, et rien d'autre : le filtre et la sélection.
 *
 * Le groupement, l'ordre et le worktree qui détient chaque branche arrivent déjà tranchés du
 * backend (ADR-0009) — aucun de ces tests ne les recalcule, et c'est délibéré : les vérifier
 * ici serait vérifier une copie.
 */

/** Un aperçu de branches, tel que `features::git` le rend — six champs et un ordre. */
class OverviewBuilder {
    private readonly sections: { group: BranchGroup; branches: Branch[] }[] = [];
    private current: string | null = "main";
    private agents: BusyAgent[] = [];

    group(group: BranchGroup, ...names: readonly string[]): this {
        this.sections.push({ group, branches: names.map((name) => branch(name)) });
        return this;
    }

    /** Une branche prise par un autre worktree — la colonne de droite de la spec §7.1. */
    heldElsewhere(group: BranchGroup, name: string, worktree: string): this {
        this.sections.push({
            group,
            branches: [{ ...branch(name), worktree: { root: `/wt/${worktree}`, name: worktree } }],
        });
        return this;
    }

    withAgents(...agents: readonly BusyAgent[]): this {
        this.agents = [...agents];
        return this;
    }

    detached(): this {
        this.current = null;
        return this;
    }

    build(): BranchOverview {
        return {
            worktreeRoot: "/dev/ash",
            current: this.current,
            sections: this.sections,
            agentsAtRisk: this.agents,
        };
    }
}

function branch(name: string): Branch {
    return { name, kind: "local", tip: "a1b2c3d", committedAt: 1_700_000_000, worktree: null };
}

function anOverview(): OverviewBuilder {
    return new OverviewBuilder();
}

function shown(rows: ReturnType<typeof visibleRows>): string[] {
    return rows.map((row) => row.branch.name);
}

describe("the branch list the popup shows", () => {
    it("Given an unfiltered overview, when the rows are flattened, then they keep the order the backend fixed", () => {
        // Given — la courante en tête, jamais rangée dans l'ordre alphabétique (spec §7.1)
        const overview = anOverview()
            .group("current", "main")
            .group("recent", "feat/popup", "fix/probe")
            .build();

        // When
        const rows = visibleRows(overview, "");

        // Then
        expect(shown(rows)).toEqual(["main", "feat/popup", "fix/probe"]);
    });

    it("Given a filter typed into the popup, when it no longer matches any branch of a group, then that group's title goes with it", () => {
        // Given
        const overview = anOverview()
            .group("current", "main")
            .group("recent", "feat/popup")
            .build();

        // When
        const rows = visibleRows(overview, "feat");

        // Then — un titre `current` sous une liste vide ne dit rien
        expect(shown(rows)).toEqual(["feat/popup"]);
        expect(rows[0]?.opensGroup).toBe(true);
        expect(rows[0]?.group).toBe("recent");
    });

    it("Given a branch held by another worktree, when its worktree name is typed, then the branch is found by it", () => {
        // Given — la popup existe pour dire qu'une branche vit ailleurs : retrouver ce
        // qu'un worktree tient en tapant son nom est exactement le geste qu'elle sert
        const overview = anOverview()
            .group("current", "main")
            .heldElsewhere("recent", "feat/sidebar", "ash-sidebar")
            .build();

        // When
        const rows = visibleRows(overview, "sidebar");

        // Then
        expect(shown(rows)).toEqual(["feat/sidebar"]);
    });

    it("Given a filter in a different case, when the rows are computed, then it still matches", () => {
        // Given
        const overview = anOverview().group("recent", "Feat/Popup").build();

        // When
        const rows = visibleRows(overview, "POPUP");

        // Then
        expect(shown(rows)).toEqual(["Feat/Popup"]);
    });

    it("Given git could not be read, when the rows are computed, then there are none rather than an invented empty repository", () => {
        // Given
        const unreadable = null;

        // When
        const rows = visibleRows(unreadable, "");

        // Then
        expect(rows).toEqual([]);
    });
});

describe("the selection inside the popup", () => {
    it("Given the first row is selected, when the user moves up, then the selection wraps to the last", () => {
        // Given
        const rows = visibleRows(anOverview().group("recent", "a", "b", "c").build(), "");

        // When
        const moved = moveSelection(rows, 0, -1);

        // Then — la liste est courte, et remonter d'un cran depuis la première est le geste
        // qu'on fait sans y penser
        expect(moved).toBe(2);
    });

    it("Given an empty list, when the user moves, then nothing is selected", () => {
        // Given
        const rows = visibleRows(anOverview().build(), "");

        // When
        const moved = moveSelection(rows, 0, 1);

        // Then — rien à valider, donc rien à désigner
        expect(moved).toBe(-1);
    });

    it("Given a selected branch that survives the next keystroke, when the filter narrows, then the selection stays on it", () => {
        // Given — en tapant `fea`, la ligne visée doit rester visée entre `f` et `fe`
        const overview = anOverview().group("recent", "chore/deps", "feat/popup").build();
        const before = visibleRows(overview, "");
        const aimed = selectedBranch(before, 1);

        // When
        const after = visibleRows(overview, "fea");
        const selected = keepSelection(after, aimed);

        // Then
        expect(selectedBranch(after, selected)?.name).toBe("feat/popup");
    });

    it("Given a selected branch the next keystroke filters out, when the filter narrows, then the selection falls back to the first row", () => {
        // Given
        const overview = anOverview().group("recent", "chore/deps", "feat/popup").build();
        const aimed = selectedBranch(visibleRows(overview, ""), 0);

        // When
        const after = visibleRows(overview, "feat");
        const selected = keepSelection(after, aimed);

        // Then — jamais une sélection qui désigne une ligne disparue
        expect(selectedBranch(after, selected)?.name).toBe("feat/popup");
    });
});
