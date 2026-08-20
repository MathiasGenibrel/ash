import { describe, expect, it } from "bun:test";

import { find, findAll, plainText } from "@/shared/ui";
import type { WorktreeRemoval, WorktreeRow } from "@/shared/ipc";

import { aged, worktreeTable, type WorktreeTableActions } from "./table-view";

/**
 * Ce que ce fichier protège : les deux colonnes que `git worktree list` ne donne pas, la
 * phrase que la spec §7.3 appelle l'état le plus utile du tableau, et le fait qu'aucun
 * bouton de cet écran ne supprime quoi que ce soit.
 *
 * Rien n'y monte de DOM : le tableau est une valeur, et un test le lit.
 */

/** Test Data Builder : une ligne du tableau, valide et déterministe. */
class RowBuilder {
    private row: WorktreeRow = {
        worktreeRoot: "/wt/ash-sidebar",
        worktreeName: "ash-sidebar",
        repo: { id: "/dev/ash/.git", name: "ash" },
        metadata: {
            head: { kind: "branch", name: "feat/table" },
            operation: null,
            status: {
                tree: { added: 0, modified: 0, deleted: 0, conflicted: 0 },
                upstream: null,
                conflicts: [],
            },
        },
        agentsNow: [],
        awaitingReview: false,
        lastWorkedBy: null,
        stale: false,
        main: false,
    };

    static create(): RowBuilder {
        return new RowBuilder();
    }

    at(worktreeRoot: string, worktreeName: string): this {
        this.row = { ...this.row, worktreeRoot, worktreeName };
        return this;
    }

    withAgent(command: string, state: WorktreeRow["agentsNow"][number]["state"]): this {
        this.row = {
            ...this.row,
            agentsNow: [
                ...this.row.agentsNow,
                { tabId: `01J0${command}`, command, state, since: NOW - 60_000 },
            ],
        };
        return this;
    }

    awaitingReview(): this {
        this.row = { ...this.row, awaitingReview: true };
        return this;
    }

    lastWorkedBy(agent: string, source: "tab" | "commit", ago: number): this {
        this.row = { ...this.row, lastWorkedBy: { agent, at: NOW - ago, source } };
        return this;
    }

    stale(): this {
        this.row = { ...this.row, stale: true };
        return this;
    }

    main(): this {
        this.row = { ...this.row, main: true };
        return this;
    }

    build(): WorktreeRow {
        return this.row;
    }
}

const NOW = 1_755_000_000_000;
const DAY = 24 * 60 * 60 * 1000;

function actions(): WorktreeTableActions & { asked: string[]; selected: string[] } {
    const asked: string[] = [];
    const selected: string[] = [];
    return {
        asked,
        selected,
        selectTab: (tabId) => selected.push(tabId),
        openTabIn: () => undefined,
        showCard: () => undefined,
        askRemoval: (root) => asked.push(root),
        dismissRemoval: () => undefined,
    };
}

/** La cellule d'une colonne, dans la première ligne du tableau. */
function cellOf(rows: readonly WorktreeRow[], column: string, showing: WorktreeRemoval | null = null): string {
    const table = worktreeTable(rows, NOW, showing, actions()).build();
    const cells = findAll(table, "git-worktrees-cell").filter(
        (cell) => cell.attrs["data-column"] === column,
    );
    // La première est celle de l'en-tête : elle porte le nom de la colonne, pas sa valeur.
    const value = cells[1];
    expect(value).toBeDefined();
    return plainText(value ?? { kind: "text", text: "" });
}

describe("le tableau des worktrees", () => {
    it("Given a worktree where an agent is working, when the table is drawn, then the agents now column names it", () => {
        // Given — la colonne que `git worktree list` ne donne pas (spec §7.3).
        const rows = [RowBuilder.create().withAgent("claude", "working").build()];

        // When
        const said = cellOf(rows, "agents now");

        // Then
        expect(said).toContain("claude");
        expect(said).toContain("working");
    });

    it("Given a worktree nobody is in, when the table is drawn, then the agents now column says nothing rather than nobody", () => {
        // Given
        const rows = [RowBuilder.create().build()];

        // When
        const said = cellOf(rows, "agents now");

        // Then
        expect(said).toBe("—");
    });

    it("Given an agent that finished and nobody looked, when the table is drawn, then the row says done · waiting for your review", () => {
        // Given — l'état que la spec §7.3 nomme le plus utile du tableau. Il est décidé par le
        // backend : la vue ne redéfinit pas « personne n'a regardé ».
        const rows = [RowBuilder.create().withAgent("claude", "done").awaitingReview().build()];

        // When
        const said = cellOf(rows, "agents now");

        // Then
        expect(said).toContain("done · waiting for your review");
    });

    it("Given a done agent whose row has been reviewed, when the table is drawn, then it is a plain done", () => {
        // Given — la phrase ne s'accroche pas à l'état `done` : elle dit qu'on n'a pas regardé.
        const rows = [RowBuilder.create().withAgent("claude", "done").build()];

        // When
        const said = cellOf(rows, "agents now");

        // Then
        expect(said).not.toContain("waiting for your review");
        expect(said).toContain("done");
    });

    it("Given a worktree ash never saw an agent in, when the table is drawn, then last worked by says it does not know", () => {
        // Given — vide veut dire « ash ne sait pas », jamais « personne » (ADR-0014). Un blanc
        // se lirait comme une panne, et une affirmation serait fausse.
        const rows = [RowBuilder.create().build()];
        const table = worktreeTable(rows, NOW, null, actions()).build();

        // When
        const unknown = find(table, "git-worktrees-unknown");

        // Then
        expect(unknown?.attrs["title"]).toContain("it does not mean nobody did");
    });

    it("Given a commit ash saw an agent write here two days ago, when the table is drawn, then last worked by names it with its age", () => {
        // Given
        const rows = [RowBuilder.create().lastWorkedBy("codex", "commit", 2 * DAY).build()];

        // When
        const said = cellOf(rows, "last worked by");

        // Then
        expect(said).toBe("codex · 2d ago");
    });

    it("Given a stale worktree, when the table is drawn, then the word carries what it observed and never proposes a deletion", () => {
        // Given — spec §5.4 : ash signale, il ne supprime jamais.
        const rows = [RowBuilder.create().stale().build()];
        const table = worktreeTable(rows, NOW, null, actions()).build();

        // When
        const marked = find(table, "git-worktrees-stale");

        // Then
        expect(plainText(marked ?? { kind: "text", text: "" })).toBe("stale");
        expect(marked?.attrs["title"]).toContain("uncommitted work");
    });

    it("Given the main worktree of a repository, when the table is drawn, then the remove button stays visible, off, and says why", () => {
        // Given — la maquette est formelle : un bouton éteint garde sa raison, il ne disparaît
        // pas. Le masquer ferait croire que la suppression n'existe pas.
        const rows = [RowBuilder.create().main().build()];
        const table = worktreeTable(rows, NOW, null, actions()).build();

        // When
        const remove = find(table, "git-worktrees-remove");

        // Then
        expect(remove?.attrs["disabled"]).toBe("");
        expect(remove?.attrs["title"]).toContain("main worktree");
    });

    it("Given a removal asked on a worktree, when its plan comes back, then the row shows what it would carry and no button that removes", () => {
        // Given — spec §5.4 : la suppression énonce ce qu'elle emporte **avant**. Ash s'arrête
        // là : la commande est du texte à montrer (ADR-0015).
        const rows = [RowBuilder.create().build()];
        const plan: WorktreeRemoval = {
            worktreeRoot: "/wt/ash-sidebar",
            worktreeName: "ash-sidebar",
            carries: ["3 uncommitted files — nothing here is in the repository yet"],
            refused: null,
            command: "git worktree remove /wt/ash-sidebar",
        };

        // When
        const table = worktreeTable(rows, NOW, plan, actions()).build();

        // Then
        const notice = find(table, "git-worktrees-removal");
        expect(plainText(notice ?? { kind: "text", text: "" })).toContain("3 uncommitted files");
        expect(plainText(notice ?? { kind: "text", text: "" })).toContain(
            "git worktree remove /wt/ash-sidebar",
        );
        const buttons = findAll(notice ?? { kind: "text", text: "" }, "ui-button");
        expect(buttons.map((held) => plainText(held))).toEqual(["close"]);
    });

    it("Given a plan asked on one worktree, when another row is drawn, then it does not show that plan", () => {
        // Given — la fiche appartient à la ligne dont on a demandé la suppression, et à elle
        // seule : deux worktrees d'un même dépôt se ressemblent assez pour qu'une fiche posée
        // sur la mauvaise ligne se lise comme la bonne.
        const rows = [RowBuilder.create().at("/dev/ash", "ash").build()];
        const plan: WorktreeRemoval = {
            worktreeRoot: "/wt/ash-sidebar",
            worktreeName: "ash-sidebar",
            carries: [],
            refused: null,
            command: "git worktree remove /wt/ash-sidebar",
        };

        // When
        const table = worktreeTable(rows, NOW, plan, actions()).build();

        // Then
        expect(find(table, "git-worktrees-removal")).toBeNull();
    });

    it("Given an agent line, when it is clicked, then the tab is selected and nothing else moves", () => {
        // Given — ADR-0010 : rien ne sélectionne sans un geste de l'utilisateur.
        const spy = actions();
        const rows = [RowBuilder.create().withAgent("claude", "waiting").build()];
        const table = worktreeTable(rows, NOW, null, spy).build();

        // When
        find(table, "git-worktrees-agent")?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(spy.selected).toEqual(["01J0claude"]);
    });
});

describe("l'âge d'une observation", () => {
    it("Given an observation of four days ago, when it is written, then it is said in days rather than in hours", () => {
        // Given — `formatElapsed` écrirait `96h00m` : elle mesure la durée d'un état d'agent,
        // pas l'âge d'une trace.
        const at = NOW - 4 * DAY;

        // When
        const said = aged(at, NOW);

        // Then
        expect(said).toBe("4d ago");
    });

    it("Given an observation dated in the future, when it is written, then it does not count backwards", () => {
        // Given — une horloge recalée entre le backend et le rendu. `-3s ago` serait pire que
        // rien.
        const at = NOW + 5_000;

        // When
        const said = aged(at, NOW);

        // Then
        expect(said).toBe("just now");
    });
});
