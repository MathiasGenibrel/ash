import { describe, expect, it } from "bun:test";

import type { AgentState, BusyAgent } from "@/shared/ipc";

import { pauseOffers, warnAbout } from "./warning";

/**
 * L'avertissement est ce que ce popup a et qu'aucun client git n'a. Ces tests protègent la
 * seule chose qui le rend utile : **il nomme**. Un avertissement qui dit « un agent tourne »
 * ne permet pas de décider ; `claude` le permet.
 */

/** Quatre champs et un invariant — l'agent en pause ne travaille plus, mais reprendra. */
class BusyAgentBuilder {
    private agent: BusyAgent = {
        tabId: "01J0TAB",
        name: "claude",
        state: "working",
        paused: false,
    };

    named(name: string): this {
        this.agent = { ...this.agent, name, tabId: `tab-${name}` };
        return this;
    }

    inState(state: AgentState): this {
        this.agent = { ...this.agent, state };
        return this;
    }

    stopped(): this {
        this.agent = { ...this.agent, paused: true };
        return this;
    }

    build(): BusyAgent {
        return this.agent;
    }
}

function anAgent(): BusyAgentBuilder {
    return new BusyAgentBuilder();
}

describe("the warning that names the agent a checkout would disturb", () => {
    it("Given one agent writing in a worktree, when the popup warns, then it names the agent and the worktree", () => {
        // Given
        const claude = anAgent().named("claude").build();

        // When
        const warning = warnAbout([claude], "ash-sidebar");

        // Then — le nom, l'endroit, et la conséquence : les trois, ou la phrase ne sert à rien
        expect(warning).toBe(
            "claude is working in ash-sidebar — this would move files under it",
        );
    });

    it("Given two agents writing in the same worktree, when the popup warns, then both are named rather than counted", () => {
        // Given
        const agents = [anAgent().named("claude").build(), anAgent().named("codex").build()];

        // When
        const warning = warnAbout(agents, "ash");

        // Then — « 2 agents are working » ne dit pas lequel il faut arrêter
        expect(warning).toBe(
            "claude and codex are working in ash — this would move files under it",
        );
    });

    it("Given an agent that is waiting for an answer, when the popup warns, then it is named like one that writes", () => {
        // Given — la règle vient du backend (`at_risk`) : `waiting` reprendra dès qu'on
        // lui répond, et il reprendra sur un arbre qui n'est plus celui qu'il a lu
        const waiting = anAgent().named("claude").inState("waiting").build();

        // When
        const warning = warnAbout([waiting], "ash");

        // Then
        expect(warning).toContain("claude");
    });

    it("Given nobody working in this worktree, when the popup warns, then there is nothing to say", () => {
        // Given — le cas courant
        const nobody: BusyAgent[] = [];

        // When
        const warning = warnAbout(nobody, "ash");

        // Then — un avertissement qui sonne toujours ne se lit plus
        expect(warning).toBeNull();
    });

    it("Given an agent that is already paused, when the popup warns, then it is still named and said to be paused", () => {
        // Given
        const stopped = anAgent().named("claude").stopped().build();

        // When
        const warning = warnAbout([stopped], "ash-sidebar");

        // Then — le faire disparaître laisserait croire que ce worktree est vide, alors
        // qu'il reprendra dès qu'on le relancera
        expect(warning).toBe("claude is paused in ash-sidebar — nothing is writing");
    });

    it("Given one agent paused and another writing, when the popup warns, then the sentence separates them", () => {
        // Given
        const agents = [
            anAgent().named("claude").stopped().build(),
            anAgent().named("codex").build(),
        ];

        // When
        const warning = warnAbout(agents, "ash");

        // Then — la conséquence reste écrite : il reste quelqu'un pour la subir
        expect(warning).toBe(
            "codex is working, and claude is paused in ash — this would move files under it",
        );
    });
});

describe("what the confirmation offers to do with each agent", () => {
    it("Given an agent that is writing, when the confirmation offers a gesture, then it offers to pause that agent by name", () => {
        // Given
        const claude = anAgent().named("claude").build();

        // When
        const offers = pauseOffers([claude]);

        // Then — `SIGSTOP` sur son groupe, et rien d'autre (ADR-0015)
        expect(offers).toEqual([{ agent: claude, label: "Pause claude", resumes: false }]);
    });

    it("Given an agent that is already paused, when the confirmation offers a gesture, then it offers to resume it", () => {
        // Given — sans ce chemin de retour, un `SIGSTOP` serait un piège : l'agent n'émet
        // plus de hook, donc plus d'état, et rien d'autre qu'Ash ne sait qu'il attend
        const stopped = anAgent().named("claude").stopped().build();

        // When
        const offers = pauseOffers([stopped]);

        // Then
        expect(offers).toEqual([{ agent: stopped, label: "Resume claude", resumes: true }]);
    });
});
