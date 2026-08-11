import { describe, expect, it } from "bun:test";

import { MetadataBuilder } from "@/shared/ipc/builders";
import type { WorktreeMetadata, WorktreeMetadataChanged } from "@/shared/ipc";
import { WorktreeMetadataStore } from "./metadata-store";
import type { GitBridge, Unsubscribe } from "./ports";

/**
 * Un backend git qui répond quand on le lui dit.
 *
 * Les réponses sont **différées à la main** : le point de ce module est ce qui se passe
 * entre la demande et la réponse, et une promesse déjà tenue ne le montrerait pas.
 */
class FakeGit implements GitBridge {
    readonly asked: string[] = [];
    private answer: ((metadata: WorktreeMetadata | null) => void)[] = [];
    private push: ((changed: WorktreeMetadataChanged) => void) | null = null;

    metadata(worktreeRoot: string): Promise<WorktreeMetadata | null> {
        this.asked.push(worktreeRoot);
        return new Promise((resolve) => this.answer.push(resolve));
    }

    onMetadataChanged(handler: (changed: WorktreeMetadataChanged) => void): Promise<Unsubscribe> {
        this.push = handler;
        return Promise.resolve(() => undefined);
    }

    /** Le backend répond à la demande en attente. */
    async reply(metadata: WorktreeMetadata | null): Promise<void> {
        this.answer.shift()?.(metadata);
        await Promise.resolve();
    }

    /** La surveillance annonce un changement — un `git commit`, un rebase qui avance. */
    announce(worktreeRoot: string, metadata: WorktreeMetadata): void {
        this.push?.({ worktreeRoot, metadata });
    }
}

describe("l'état git des worktrees habités", () => {
    it("Given a worktree the probe keeps redrawing, when the line is composed again and again, then git is only asked once", async () => {
        // Given — la boucle de sonde provoque un rendu plusieurs fois par seconde, et
        // `git_metadata` peut coûter un `git status` : redemander à chaque rendu lancerait
        // un processus trois fois par seconde et par worktree
        const git = new FakeGit();
        const store = new WorktreeMetadataStore(git, () => undefined);

        // When
        store.of("/dev/ash");
        store.of("/dev/ash");
        await git.reply(MetadataBuilder.create().onBranch("main").build());
        store.of("/dev/ash");

        // Then
        expect(git.asked).toEqual(["/dev/ash"]);
        expect(store.of("/dev/ash")?.head).toEqual({ kind: "branch", name: "main" });
    });

    it("Given a worktree already read, when the watcher announces a commit, then the line is redrawn with the new state", async () => {
        // Given
        const git = new FakeGit();
        const seen: (WorktreeMetadata | null)[] = [];
        const store = new WorktreeMetadataStore(git, () => seen.push(store.of("/dev/ash")));
        store.of("/dev/ash");
        await git.reply(MetadataBuilder.create().onBranch("main").withTree({ added: 3 }).build());

        // When — c'est la surveillance de fichiers d'ADR-0011 qui pousse, jamais un sondage
        git.announce("/dev/ash", MetadataBuilder.create().onBranch("main").build());

        // Then
        expect(seen.map((metadata) => metadata?.status?.tree.added)).toEqual([3, 0]);
    });

    it("Given two worktrees of the same repository, when one of them rebases, then the other keeps its own state", async () => {
        // Given — l'état git est propre au worktree, jamais au dépôt (ADR-0012)
        const git = new FakeGit();
        const store = new WorktreeMetadataStore(git, () => undefined);
        store.of("/dev/ash");
        await git.reply(MetadataBuilder.create().onBranch("main").build());

        // When
        git.announce("/wt/ash-sidebar", MetadataBuilder.create().rebasing("feat", "main").build());

        // Then
        expect(store.of("/dev/ash")?.operation).toBeNull();
        expect(store.of("/wt/ash-sidebar")?.operation?.kind).toBe("rebase");
    });

    it("Given a worktree whose state is not known yet, when the line is composed, then it does not wait for git to answer", () => {
        // Given — la ligne se dessine à chaque rendu, synchrone : elle ne peut pas attendre
        // un `git status` qui peut prendre des secondes sur un gros dépôt
        const git = new FakeGit();
        const store = new WorktreeMetadataStore(git, () => undefined);

        // When
        const now = store.of("/dev/ash");

        // Then
        expect(now).toBeNull();
        expect(git.asked).toEqual(["/dev/ash"]);
    });
});
