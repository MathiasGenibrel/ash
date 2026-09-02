import { describe, expect, it } from "bun:test";

import type { ActionOffer, ActionOutcome, Branch, BranchOverview, BusyAgent } from "@/shared/ipc";
import { find, findAll, plainText } from "@/shared/ui";

import { visibleRows } from "./branch-list";
import { composeBranchPopup, type PopupActions, type PopupModel, type PopupStage } from "./popup";

/**
 * Ce que la popup **montre**, lu comme une valeur.
 *
 * Le socle de composants rend une description : ces tests lisent des classes et du texte,
 * sans monter de DOM. Ce qu'ils protègent est ce que la spec §7.1 exige et qu'un rendu
 * pourrait taire — la colonne de droite, l'avertissement nommé, les actions refusées mais
 * visibles, et les deux côtés dans un message d'erreur.
 */

function branch(name: string, worktree: string | null = null): Branch {
    return {
        name,
        kind: "local",
        tip: "a1b2c3d",
        committedAt: 1_700_000_000,
        worktree: worktree === null ? null : { root: `/wt/${worktree}`, name: worktree },
    };
}

function agent(name: string, paused = false): BusyAgent {
    return { tabId: `tab-${name}`, name, state: "working", paused };
}

function overviewOf(branches: readonly Branch[], agents: readonly BusyAgent[]): BranchOverview {
    return {
        worktreeRoot: "/dev/ash",
        current: "main",
        sections: [{ group: "recent", branches: [...branches] }],
        agentsAtRisk: [...agents],
    };
}

function modelOf(
    overview: BranchOverview | null,
    stage: PopupStage = { kind: "list" },
): PopupModel {
    return {
        overview,
        query: "",
        rows: visibleRows(overview, ""),
        selected: 0,
        stage,
        running: false,
    };
}

/** Des rappels muets : une description ne s'exécute pas, ces tests ne cliquent rien. */
const inert: PopupActions = {
    filter: () => undefined,
    move: () => undefined,
    choose: () => undefined,
    openActions: () => undefined,
    pick: () => undefined,
    proceed: () => undefined,
    pause: () => undefined,
    back: () => undefined,
    close: () => undefined,
};

const offer = (over: Partial<ActionOffer> = {}): ActionOffer => ({
    action: "checkout",
    label: "Check out feat/popup in ash, leaving main",
    refused: null,
    touchesTree: true,
    ...over,
});

describe("the branch list the popup paints", () => {
    it("Given a branch checked out in another worktree, when the popup lists it, then the right-hand column names that worktree", () => {
        // Given
        const overview = overviewOf([branch("feat/sidebar", "ash-sidebar")], []);

        // When
        const painted = composeBranchPopup(modelOf(overview), inert);

        // Then — c'est la première des deux choses qu'aucun client git n'a
        const elsewhere = find(painted, "branch-popup-elsewhere");
        expect(elsewhere).not.toBeNull();
        expect(plainText(elsewhere ?? { kind: "text", text: "" })).toBe("ash-sidebar");
    });

    it("Given a branch that lives in no other worktree, when the popup lists it, then nothing is written on the right", () => {
        // Given
        const overview = overviewOf([branch("feat/popup")], []);

        // When
        const painted = composeBranchPopup(modelOf(overview), inert);

        // Then — l'écrire toujours la rendrait illisible en la rendant constante
        expect(find(painted, "branch-popup-elsewhere")).toBeNull();
    });

    it("Given an agent working in this worktree, when the popup opens, then the list already names it", () => {
        // Given
        const overview = overviewOf([branch("feat/popup")], [agent("claude")]);

        // When
        const painted = composeBranchPopup(modelOf(overview), inert);

        // Then — l'avertissement est là avant tout geste, pas seulement dans la confirmation
        const warning = find(painted, "branch-popup-warning");
        expect(plainText(warning ?? { kind: "text", text: "" })).toContain("claude");
        expect(plainText(warning ?? { kind: "text", text: "" })).toContain("ash");
    });

    it("Given nobody working in this worktree, when the popup opens, then there is no warning at all", () => {
        // Given
        const overview = overviewOf([branch("feat/popup")], []);

        // When
        const painted = composeBranchPopup(modelOf(overview), inert);

        // Then
        expect(find(painted, "branch-popup-warning")).toBeNull();
    });

    it("Given git could not read the repository, when the popup opens, then it says so instead of showing an empty list", () => {
        // Given
        const unreadable = null;

        // When
        const painted = composeBranchPopup(modelOf(unreadable), inert);

        // Then — une liste vide se lirait comme un dépôt sans branche
        expect(plainText(painted)).toContain("not in a repository");
    });

    it("Given every branch is filtered out, when the popup paints, then it names the filter that emptied it", () => {
        // Given
        const overview = overviewOf([branch("feat/popup")], []);
        const model: PopupModel = { ...modelOf(overview), query: "zzz", rows: [], selected: -1 };

        // When
        const painted = composeBranchPopup(model, inert);

        // Then
        expect(plainText(painted)).toContain("zzz");
    });
});

describe("the action submenu that ⌘⏎ opens", () => {
    it("Given three actions, when the submenu opens, then each names both of its sides", () => {
        // Given
        const offers: ActionOffer[] = [
            offer(),
            offer({ action: "rebase", label: "Rebase main onto feat/popup" }),
            offer({ action: "merge", label: "Merge feat/popup into main" }),
        ];
        const stage: PopupStage = { kind: "actions", branch: branch("feat/popup"), offers };

        // When
        const painted = composeBranchPopup(modelOf(overviewOf([], []), stage), inert);

        // Then — « Rebase » tout seul est exactement ce que la spec interdit
        const labels = findAll(painted, "branch-popup-action").map(plainText);
        expect(labels).toEqual([
            "Check out feat/popup in ash, leaving main",
            "Rebase main onto feat/popup",
            "Merge feat/popup into main",
        ]);
    });

    it("Given a refused action, when the submenu opens, then it stays visible, disabled, and carries its reason", () => {
        // Given
        const refused = offer({
            refused:
                "feat/sidebar is checked out in ash-sidebar — a branch lives in one worktree at a time",
        });
        const stage: PopupStage = {
            kind: "actions",
            branch: branch("feat/sidebar", "ash-sidebar"),
            offers: [refused],
        };

        // When
        const painted = composeBranchPopup(modelOf(overviewOf([], []), stage), inert);

        // Then — la masquer ferait croire qu'elle n'existe pas
        const action = find(painted, "branch-popup-action");
        expect(action?.attrs["disabled"]).toBe("");
        expect(action?.attrs["title"]).toContain("ash-sidebar");
    });
});

describe("the confirmation an action that touches the tree triggers", () => {
    it("Given an agent writing here, when an action is confirmed, then the box names it and offers to pause it", () => {
        // Given
        const overview = overviewOf([branch("feat/popup")], [agent("claude")]);
        const stage: PopupStage = { kind: "confirm", branch: branch("feat/popup"), offer: offer() };

        // When
        const painted = composeBranchPopup(modelOf(overview, stage), inert);

        // Then — la pause est `SIGSTOP`, jamais une touche envoyée au PTY (ADR-0015)
        expect(
            plainText(find(painted, "branch-popup-warning") ?? { kind: "text", text: "" }),
        ).toContain("claude");
        expect(findAll(painted, "branch-popup-pause").map(plainText)).toEqual(["Pause claude"]);
    });

    it("Given a confirmation box that just appeared, when it asks for the focus, then the gesture that touches nothing gets it", () => {
        // Given
        const overview = overviewOf([branch("feat/popup")], [agent("claude")]);
        const stage: PopupStage = { kind: "confirm", branch: branch("feat/popup"), offer: offer() };

        // When
        const painted = composeBranchPopup(modelOf(overview, stage), inert);

        // Then — une touche entrée sur une boîte qui apparaît ne doit pas déplacer les
        // fichiers d'un agent en train d'écrire
        expect(find(painted, "branch-popup-cancel")?.attrs["data-focus-key"]).toBe(
            "branch-popup-cancel",
        );
    });

    it("Given an already paused agent, when the confirmation is shown, then it offers to resume it rather than a dead button", () => {
        // Given
        const overview = overviewOf([branch("feat/popup")], [agent("claude", true)]);
        const stage: PopupStage = { kind: "confirm", branch: branch("feat/popup"), offer: offer() };

        // When
        const painted = composeBranchPopup(modelOf(overview, stage), inert);

        // Then — un agent laissé arrêté sans moyen de le relancer est un piège
        expect(findAll(painted, "branch-popup-pause").map(plainText)).toEqual(["Resume claude"]);
    });

    it("Given an action already in flight, when the confirmation is painted again, then it cannot be launched twice", () => {
        // Given
        const overview = overviewOf([branch("feat/popup")], [agent("claude")]);
        const stage: PopupStage = { kind: "confirm", branch: branch("feat/popup"), offer: offer() };
        const model: PopupModel = { ...modelOf(overview, stage), running: true };

        // When
        const painted = composeBranchPopup(model, inert);

        // Then
        expect(find(painted, "is-danger")?.attrs["disabled"]).toBe("");
    });
});

describe("what the popup says when git answers", () => {
    it("Given a rebase git refused, when the outcome is shown, then it still names both sides", () => {
        // Given
        const outcome: ActionOutcome = {
            label: "Rebase main onto feat/popup",
            success: false,
            output: "error: cannot rebase: You have unstaged changes.",
        };

        // When
        const painted = composeBranchPopup(
            modelOf(overviewOf([], []), { kind: "outcome", outcome }),
            inert,
        );

        // Then — « y compris dans les messages d'erreur » (spec §7.1)
        const read = plainText(painted);
        expect(read).toContain("Rebase main onto feat/popup");
        expect(read).toContain("unstaged changes");
    });
});
