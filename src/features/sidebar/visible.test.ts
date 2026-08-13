import { describe, expect, it } from "bun:test";

import type { AgentState, TabInfo } from "@/shared/ipc";
import { TabBuilder } from "@/shared/ipc/builders";
import { buildSidebar, type SidebarTree } from "./tree";
import { visibleStates } from "./visible";

/**
 * Un dépôt, deux worktrees, trois onglets — le plus petit décor qui ait les **trois**
 * replis de la colonne : le groupe de dépôt, chacun des deux worktrees, et `⌘B`.
 */
const REPO = "acme";
const GROUP = "repo:/dev/acme/.git";
const API = "/wt/acme-api";
const WEB = "/wt/acme-web";

function tabsWith(waiting: "in-api" | "in-web" | "nowhere"): readonly TabInfo[] {
    const state = (where: "in-api" | "in-web"): AgentState =>
        waiting === where ? "waiting" : "working";

    return [
        TabBuilder.create()
            .named("A")
            .running("claude", state("in-api"))
            .inWorktree(API, REPO)
            .build(),
        TabBuilder.create().named("B").running("bun", "idle").inWorktree(API, REPO).build(),
        TabBuilder.create()
            .named("C")
            .running("codex", state("in-web"))
            .inWorktree(WEB, REPO)
            .build(),
    ];
}

interface Folding {
    readonly groups: readonly string[];
    readonly worktrees: readonly string[];
    readonly columnCollapsed: boolean;
}

/** Les seize façons de replier cette colonne : le groupe × les deux worktrees × `⌘B`. */
function everyFolding(): readonly Folding[] {
    const foldings: Folding[] = [];
    for (const groups of [[], [GROUP]]) {
        for (const api of [[], [API]]) {
            for (const web of [[], [WEB]]) {
                for (const columnCollapsed of [false, true]) {
                    foldings.push({ groups, worktrees: [...api, ...web], columnCollapsed });
                }
            }
        }
    }
    return foldings;
}

function describeFolding(folding: Folding): string {
    return `groupe:[${folding.groups.join()}] worktrees:[${folding.worktrees.join()}] colonne:${String(folding.columnCollapsed)}`;
}

function foldedTree(tabs: readonly TabInfo[], folding: Folding): SidebarTree {
    return buildSidebar(tabs, {
        activeTabId: null,
        collapsedWorktrees: new Set(folding.worktrees),
        collapsedGroups: new Set(folding.groups),
    });
}

function shownUnder(tabs: readonly TabInfo[], folding: Folding): readonly AgentState[] {
    return visibleStates(foldedTree(tabs, folding), folding.columnCollapsed);
}

describe("une ligne repliée ne cache jamais un agent qui attend", () => {
    it("Given a waiting agent somewhere in a repository, when any combination of rows and of the column is collapsed, then the column still shows waiting", () => {
        // Given — la garantie de la spec §4.1 porte sur la colonne entière et pas sur une
        // ligne : elle ne tient que si *aucune* combinaison de replis ne l'efface. Deux
        // placements de l'agent, parce qu'un worktree replié et un worktree déplié
        // n'empruntent pas le même chemin.
        const foldings = everyFolding();

        // When — les replis qui perdent l'agent, plutôt qu'un échec au premier : on veut
        // lire *lesquels* échouent
        const blind = ["in-api", "in-web"].flatMap((where) => {
            const tabs = tabsWith(where as "in-api" | "in-web");
            return foldings
                .filter((folding) => !shownUnder(tabs, folding).includes("waiting"))
                .map((folding) => `${where} — ${describeFolding(folding)}`);
        });

        // Then
        expect(foldings).toHaveLength(16);
        expect(blind).toEqual([]);
    });

    it("Given no waiting agent at all, when every combination of rows is collapsed, then the column never invents one", () => {
        // Given — sans ce garde-fou, le test précédent passerait aussi si la colonne criait
        // `waiting` partout
        const tabs = tabsWith("nowhere");

        // When
        const noisy = everyFolding().filter((folding) =>
            shownUnder(tabs, folding).includes("waiting"),
        );
        const mute = everyFolding().filter((folding) => shownUnder(tabs, folding).length === 0);

        // Then — et aucune combinaison ne rend une colonne muette
        expect(noisy.map(describeFolding)).toEqual([]);
        expect(mute.map(describeFolding)).toEqual([]);
    });
});

describe("ce qu'une ligne repliée porte à la place de ses enfants", () => {
    const expanded: Folding = { groups: [], worktrees: [], columnCollapsed: false };

    it("Given a collapsed repository whose only waiting agent sits two levels down, when the column is drawn, then the repository row carries waiting itself", () => {
        // Given — un dépôt replié efface ses worktrees *et* leurs onglets : sa ligne est le
        // seul endroit qui reste pour le dire
        const tabs = tabsWith("in-web");

        // When
        const shown = shownUnder(tabs, { ...expanded, groups: [GROUP] });

        // Then — une seule ligne visible, et elle porte l'état le plus urgent du dépôt
        expect(shown).toEqual(["waiting"]);
    });

    it("Given a collapsed worktree where an agent waits while another is idle, when the column is drawn, then its row shows waiting and not idle", () => {
        // Given
        const tabs = tabsWith("in-api");

        // When
        const shown = shownUnder(tabs, { ...expanded, worktrees: [API] });

        // Then — l'onglet `bun` idle est derrière la ligne ; c'est `waiting` qui la
        // représente, et le worktree resté déplié montre toujours son propre onglet
        expect(shown).toEqual(["waiting", "working"]);
    });

    it("Given an expanded worktree, when the column is drawn, then its row adds no state of its own", () => {
        // Given — dépliée, la ligne n'a rien à remonter : ses onglets se disent eux-mêmes,
        // et un glyphe de plus ne ferait que répéter le plus urgent d'entre eux
        const tabs = tabsWith("in-api");

        // When
        const shown = shownUnder(tabs, expanded);

        // Then
        expect(shown).toEqual(["waiting", "idle", "working"]);
    });
});
