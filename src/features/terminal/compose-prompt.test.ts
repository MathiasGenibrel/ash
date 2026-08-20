import { describe, expect, it } from "bun:test";

import type { ComposeOutcome, TabId } from "@/shared/ipc";

import { handOverConflictsToAgent, type HandOverDeps } from "./compose-prompt";
import type { GitBridge, PtyBridge } from "./ports";

/**
 * Test Data Builder : le décor d'un « passer à l'agent ».
 *
 * Défauts valides et déterministes — un rebase arrêté qui a un prompt, un onglet qui
 * l'accepte, et un enregistrement de **l'ordre** dans lequel les choses arrivent. C'est cet
 * ordre qui porte la règle d'ADR-0015 : sélectionner avant d'écrire.
 */
class Scene {
    readonly steps: string[] = [];
    private prompt: string | null = "The rebase of feat onto main stopped. Resolve them.";
    private outcome: ComposeOutcome = "written";

    stoppedWithNothing(): this {
        this.prompt = null;
        return this;
    }

    answering(outcome: ComposeOutcome): this {
        this.outcome = outcome;
        return this;
    }

    deps(): HandOverDeps {
        const git = {
            conflictPrompt: (worktreeRoot: string) => {
                this.steps.push(`asked ${worktreeRoot}`);
                return Promise.resolve(this.prompt);
            },
        } as unknown as GitBridge;

        const pty = {
            compose: (tabId: TabId, text: string) => {
                this.steps.push(`composed in ${tabId}: ${text}`);
                return Promise.resolve(this.outcome);
            },
        } as unknown as PtyBridge;

        return {
            git,
            pty,
            selectTab: (tabId: TabId) => {
                this.steps.push(`selected ${tabId}`);
            },
        };
    }
}

const handOver = { worktreeRoot: "/dev/ash", tabId: "01J0TAB" };

describe("passer un rebase arrêté à l'agent de l'onglet", () => {
    it("Given a stopped rebase and an agent tab, when the work is handed over, then the tab is selected before anything is written", async () => {
        // Given — ADR-0015 : « composer doit toujours sélectionner l'onglet de destination
        // — écrire dans un terminal qu'on ne regarde pas viole la première condition »
        const scene = new Scene();

        // When
        await handOverConflictsToAgent(handOver, scene.deps());

        // Then — l'ordre, et pas seulement la présence des deux gestes
        expect(scene.steps).toEqual([
            "asked /dev/ash",
            "selected 01J0TAB",
            "composed in 01J0TAB: The rebase of feat onto main stopped. Resolve them.",
        ]);
    });

    it("Given a prompt that ash has typed, when the notice is shown, then it says the text has not been sent", async () => {
        // Given — la franchise de ce moment est la décision elle-même : on voit ce qui va
        // partir avant que ça parte (ADR-0015)
        const scene = new Scene();

        // When
        const notice = await handOverConflictsToAgent(handOver, scene.deps());

        // Then
        expect(notice).toEqual({
            tone: "typed",
            message: "ash typed this for you — not sent yet",
        });
    });

    it("Given a tab busy with an agent turn, when the work is handed over, then the notice announces the wait rather than a failure", async () => {
        // Given — le corollaire de file d'attente : le texte partira, à la fin du tour
        const scene = new Scene().answering("queued");

        // When
        const notice = await handOverConflictsToAgent(handOver, scene.deps());

        // Then
        expect(notice?.tone).toBe("queued");
        expect(notice?.message).toContain("not sent yet");
    });

    it("Given a tab whose prompt is not empty, when the work is handed over, then the tab is still selected and the refusal is explained", async () => {
        // Given — le refus parlerait d'un prompt que l'utilisateur ne regarde pas si la
        // sélection était conditionnée à la réussite
        const scene = new Scene().answering("prompt-not-empty");

        // When
        const notice = await handOverConflictsToAgent(handOver, scene.deps());

        // Then
        expect(scene.steps).toContain("selected 01J0TAB");
        expect(notice).toEqual({
            tone: "refused",
            message: "there is already something in this prompt — ash wrote nothing",
        });
    });

    it("Given a worktree with nothing stopped, when the work is handed over, then no tab is selected and nothing is written", async () => {
        // Given — le cas courant : il n'y a rien à passer, donc rien à dire
        const scene = new Scene().stoppedWithNothing();

        // When
        const notice = await handOverConflictsToAgent(handOver, scene.deps());

        // Then — sélectionner un onglet pour ne rien y écrire déplacerait l'utilisateur
        // sans raison
        expect(notice).toBeNull();
        expect(scene.steps).toEqual(["asked /dev/ash"]);
    });
});
