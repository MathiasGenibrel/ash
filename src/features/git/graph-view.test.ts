import { describe, expect, it } from "bun:test";

import { find, findAll, plainText } from "@/shared/ui";

import type { CommitGraph, CommitRow } from "./contract";
import {
    commitDetail,
    commitGraphView,
    day,
    laneDrawing,
    linkShape,
    ROW_HEIGHT,
} from "./graph-view";

/**
 * Test Data Builder : une ligne de graphe, avec des défauts valides et déterministes.
 *
 * Le défaut est le cas le plus courant du produit — un commit qu'Ash n'a **pas** vu naître,
 * donc une colonne `by` qui montre le nom d'auteur git.
 */
function aCommit(overrides: Partial<CommitRow> = {}): CommitRow {
    return {
        sha: "8f3a1c2aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        short: "8f3a1c2",
        subject: "feat(sidebar): group tabs by worktree",
        by: "mathias",
        attributed: false,
        author: "mathias",
        authorDate: "2026-08-12T14:03:21+02:00",
        authoredAt: 1_786_000_000,
        refs: [],
        lane: 0,
        links: [{ from: 0, to: 0 }],
        tabId: null,
        prompt: null,
        promptNote: "no agent was observed writing this commit",
        ...overrides,
    };
}

function aGraph(rows: CommitRow[], overrides: Partial<CommitGraph> = {}): CommitGraph {
    return { rows, lanes: 1, folded: [], window: 200, hasMore: false, ...overrides };
}

const NOTHING = {
    select: (): void => undefined,
    widen: (): void => undefined,
};

describe("the by column", () => {
    it("Given a commit ash saw an agent write, when the graph is drawn, then its by cell says the name comes from ash", () => {
        // Given — c'est la raison d'être de l'écran (spec §7.2). Le mot seul ne suffit pas :
        // un dépôt dont l'auteur git s'appelle `claude` rendrait les deux indiscernables.
        const graph = aGraph([aCommit({ by: "claude", attributed: true })]);

        // When
        const view = commitGraphView({ graph, selected: null }, NOTHING).build();

        // Then
        const cell = find(view, "git-graph-by");
        expect(plainText(cell ?? { kind: "text", text: "" })).toBe("claude");
        expect(cell?.attrs["data-attributed"]).toBe("agent");
    });

    it("Given a commit ash never observed, when the graph is drawn, then its by cell falls back to the git author and says so", () => {
        // Given — ADR-0014 : la colonne ne montre un nom d'agent que quand Ash l'a réellement
        // observé. Un commit sans correspondance n'est pas orphelin.
        const graph = aGraph([aCommit()]);

        // When
        const view = commitGraphView({ graph, selected: null }, NOTHING).build();

        // Then
        const cell = find(view, "git-graph-by");
        expect(plainText(cell ?? { kind: "text", text: "" })).toBe("mathias");
        expect(cell?.attrs["data-attributed"]).toBe("git");
    });
});

describe("the detail panel", () => {
    it("Given a commit whose prompt was kept, when its detail is drawn, then the prompt is shown as it was typed", () => {
        // Given — le jour où le prompt aura une source, c'est lui qui doit s'afficher, et ses
        // retours à la ligne sont de l'information.
        const commit = aCommit({
            attributed: true,
            by: "claude",
            tabId: "01J0TAB",
            prompt: "ajoute les onglets\net leurs raccourcis",
            promptNote: "",
        });

        // When
        const detail = commitDetail(commit).build();

        // Then
        const shown = find(detail, "git-graph-detail-prompt");
        expect(shown?.tag).toBe("pre");
        expect(plainText(shown ?? { kind: "text", text: "" })).toContain("ajoute les onglets");
    });

    it("Given a commit with no prompt, when its detail is drawn, then it says so with the backend's sentence and invents nothing", () => {
        // Given — le champ `prompt` du journal n'a aujourd'hui aucune source, donc c'est le
        // cas de **tous** les commits. Fabriquer un texte de remplacement ferait croire à un
        // prompt qui n'a jamais existé, et la phrase qui distingue les deux absences est
        // composée en Rust — l'écran ne la réécrit pas.
        const commit = aCommit({
            attributed: true,
            by: "claude",
            promptNote: "ash saw claude write this commit, but kept no prompt for it",
        });

        // When
        const detail = commitDetail(commit).build();

        // Then
        expect(find(detail, "git-graph-detail-prompt")).toBeNull();
        expect(plainText(find(detail, "git-graph-detail-note") ?? { kind: "text", text: "" })).toBe(
            "ash saw claude write this commit, but kept no prompt for it",
        );
    });

    it("Given a selected commit, when the graph is drawn, then its detail is there and its row is marked", () => {
        // Given — une seule ligne peut être ouverte, et c'est la même information qui la
        // marque et qui ouvre le panneau : deux sources finiraient par diverger.
        const graph = aGraph([aCommit(), aCommit({ sha: "autre", short: "autre" })]);

        // When
        const view = commitGraphView({ graph, selected: "autre" }, NOTHING).build();

        // Then
        const selected = findAll(view, "is-selected");
        expect(selected).toHaveLength(1);
        expect(selected[0]?.attrs["data-sha"]).toBe("autre");
        expect(find(view, "git-graph-detail")).not.toBeNull();
    });
});

describe("the folded branches", () => {
    it("Given branches folded by the 30-day rule, when the graph is drawn, then it names them instead of hiding them silently", () => {
        // Given — replier sans le dire ferait croire à une histoire perdue. C'est pour ça que
        // le backend rend le **nom** des branches et pas seulement leur compte.
        const graph = aGraph([aCommit()], {
            folded: [
                { name: "wip/2024", lastActivity: 1_700_000_000 },
                { name: "spike", lastActivity: 1_690_000_000 },
            ],
        });

        // When
        const view = commitGraphView({ graph, selected: null }, NOTHING).build();

        // Then
        const notice = plainText(find(view, "git-graph-folded") ?? { kind: "text", text: "" });
        expect(notice).toContain("wip/2024");
        expect(notice).toContain("spike");
        expect(notice).toContain("30 days");
    });

    it("Given a graph with nothing folded, when it is drawn, then no notice appears at all", () => {
        // Given — le cas de l'immense majorité des dépôts. Une bannière permanente qui dirait
        // « 0 branche repliée » serait du bruit sur un écran déjà dense.
        const graph = aGraph([aCommit()]);

        // When
        const view = commitGraphView({ graph, selected: null }, NOTHING).build();

        // Then
        expect(find(view, "git-graph-folded")).toBeNull();
    });
});

describe("the window", () => {
    it("Given more history than the window holds, when show more is pressed, then it asks for a larger window from the top", () => {
        // Given — les couloirs d'une ligne dépendent de tout ce qui la précède, donc le graphe
        // grandit par une **fenêtre** et jamais par une page qui commencerait au milieu.
        const asked: number[] = [];
        const graph = aGraph([aCommit()], { hasMore: true, window: 200 });
        const view = commitGraphView(
            { graph, selected: null },
            {
                select: () => undefined,
                widen: (next) => {
                    asked.push(next);
                },
            },
        ).build();

        // When
        find(view, "git-graph-more")?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(asked).toEqual([400]);
    });

    it("Given a graph that reached the end of the history, when it is drawn, then nothing promises more", () => {
        // Given — un bouton qui ne rendrait rien de plus est pire qu'une absence de bouton.
        const graph = aGraph([aCommit()]);

        // When
        const view = commitGraphView({ graph, selected: null }, NOTHING).build();

        // Then
        expect(find(view, "git-graph-more")).toBeNull();
    });
});

describe("the lane drawing", () => {
    it("Given a link that stays in its lane, when it is shaped, then it is a straight line", () => {
        // Given — c'est le cas de l'immense majorité des traits, et une courbe de Bézier qui
        // rendrait un trait droit ferait un tracé plus lourd pour rien.
        const shape = linkShape({ from: 1, to: 1 }, ROW_HEIGHT / 2);

        // Then
        expect(shape).toContain("V");
        expect(shape).not.toContain("C");
    });

    it("Given a link that changes lane, when it is shaped, then it curves with vertical tangents", () => {
        // Given — une diagonale nue ferait un angle au point de départ de chaque fusion. Les
        // deux points de contrôle sont à la même abscisse que leurs extrémités : c'est ce qui
        // rend le raccord lisse au point du commit.
        const shape = linkShape({ from: 0, to: 2 }, ROW_HEIGHT / 2);

        // Then
        expect(shape).toContain("C");
    });

    it("Given a commit ash saw an agent write, when its lane is drawn, then its dot is marked too", () => {
        // Given — la colonne `by` est en bout de ligne ; le point, lui, est là où l'œil suit
        // le graphe. Dire l'attribution aux deux endroits est ce qui la rend lisible en
        // parcourant le dessin.
        const drawing = laneDrawing(aCommit({ attributed: true, by: "claude" }), 2).build();

        // Then
        expect(find(drawing, "is-attributed")).not.toBeNull();
        expect(drawing.attrs["width"]).toBe("28");
    });
});

describe("the commit date", () => {
    it("Given the date git wrote, when the row shows it, then only its day is kept", () => {
        // Given — la date d'auteur est gardée **telle que git l'écrit** parce que c'est la
        // moitié de la clé d'attribution d'ADR-0014. Ce que la ligne en montre est un choix
        // d'affichage, et il ne touche pas à la chaîne qui traverse.
        expect(day("2026-08-12T14:03:21+02:00")).toBe("2026-08-12");
    });

    it("Given a date git could not write, when the row shows it, then it shows nothing rather than a fragment", () => {
        // Given — la sortie vient d'un dépôt que personne n'a validé. Découper les dix
        // premiers caractères de n'importe quoi afficherait n'importe quoi.
        expect(day("")).toBe("");
    });
});

describe("an empty graph", () => {
    it("Given a directory outside any repository, when the panel is drawn, then it says there is nothing rather than looking broken", () => {
        // Given — un onglet dans `/tmp` est un cas nominal, pas une panne. C'est le même
        // rendu qu'un dépôt sans commit, et pour la même raison : il n'y a rien à montrer.
        const view = commitGraphView({ graph: null, selected: null }, NOTHING).build();

        // Then
        expect(view.classes).toContain("is-empty");
        expect(plainText(view)).toContain("not inside a git repository");
    });
});
