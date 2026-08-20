import { describe, expect, it } from "bun:test";

import type { StoppedOperation } from "@/shared/ipc";
import { find, findAll, plainText } from "@/shared/ui";

import { conflictsView } from "./conflicts";

const stopped: StoppedOperation = {
    operation: { kind: "rebase", branch: "feat", onto: "main", progress: { step: 2, total: 5 } },
    conflicts: ["src/probe.rs", "src/main.ts"],
    conflictedTotal: 2,
    stoppedAt: { commit: "1a2b3c4", subject: "add the probe" },
    origHead: "80eca44",
    testCommand: "cargo test",
    escapes: ["git rebase --abort", "git rebase --skip"],
};

describe("la porte d'entrée de l'onglet de merge", () => {
    it("Given a stopped rebase, when the conflicts view is drawn, then it offers to resolve in ash and runs nothing else", () => {
        // Given — la seconde route de la spec §7.4. `abort` et `skip` restent **visibles**
        // et ne sont pas exécutables : `--abort` jette le travail de l'utilisateur (ADR-0015).
        let opened = 0;

        // When
        const view = conflictsView(stopped, {
            resolveInAsh: () => {
                opened += 1;
            },
        });

        // Then — un seul bouton dans toute la vue, et c'est celui qui ouvre un onglet
        expect(findAll(view, "ui-button")).toHaveLength(1);
        find(view, "git-conflicts-open")?.on["click"]?.({ value: "", key: "", shiftKey: false });
        expect(opened).toBe(1);
        expect(plainText(find(view, "git-conflicts-escapes")!)).toContain("git rebase --abort");
        expect(plainText(view)).toContain("2/5");
        expect(plainText(view)).toContain("ORIG_HEAD 80eca44");
    });

    it("Given nothing in progress, when the conflicts view is drawn, then it offers nothing at all", () => {
        // Given — le cas courant, et de loin
        const view = conflictsView(null, { resolveInAsh: () => undefined });

        // When
        const buttons = findAll(view, "ui-button");

        // Then
        expect(buttons).toHaveLength(0);
        expect(plainText(view)).toBe("Nothing is stopped in this worktree.");
    });
});
