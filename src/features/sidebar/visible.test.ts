import { describe, expect, it } from "bun:test";

import type { AgentState, TabInfo } from "@/shared/ipc";
import { TabBuilder } from "@/shared/ipc/builders";
import { buildSidebar, type SidebarTree } from "./tree";
import { showsSubagents, visibleStates } from "./visible";

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
        pinned: [],
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

/**
 * Le même décor, plus **un niveau** : l'onglet `claude` de `api` porte deux sous-agents.
 *
 * Il double le nombre de combinaisons de replis — les seize d'avant, avec et sans enfants —
 * et c'est ce que la tranche des sous-agents ajoute à la garantie de la spec §4.1 : une
 * colonne repliée ne doit pas davantage cacher ce qui se passe *sous* une ligne d'agent.
 */
function tabsWithSubagents(waiting: "in-api" | "in-web" | "nowhere"): readonly TabInfo[] {
    return tabsWith(waiting).map((tab, index) =>
        index === 0
            ? TabBuilder.create()
                  .named(tab.tabId)
                  .running(tab.process, tab.state)
                  .inWorktree(API, REPO)
                  .withSubagent("explore", "working")
                  .withSubagent("code-reviewer", "done")
                  .build()
            : tab,
    );
}

describe("une ligne fille ne se perd pas davantage qu'une autre", () => {
    it("Given a subagent running under an agent, when any combination of rows and of the column is collapsed, then something above it still says that it works", () => {
        // Given — un enfant n'a pas de ligne à lui quand son parent est replié : c'est la
        // ligne du worktree, puis celle du dépôt, qui doivent le porter. Sans `tabStates`
        // dans la remontée, un worktree replié dont le seul travail se passe **sous** un
        // agent `done` afficherait `done`, et la colonne dirait que tout est fini.
        const tabs = [
            TabBuilder.create()
                .named("A")
                .running("claude", "done")
                .inWorktree(API, REPO)
                .withSubagent("explore", "working")
                .build(),
        ];

        // When
        const blind = everyFolding().filter(
            (folding) => !shownUnder(tabs, folding).includes("working"),
        );

        // Then
        expect(blind.map(describeFolding)).toEqual([]);
    });

    it("Given an expanded worktree whose agent runs two subagents, when the column is drawn, then each child adds its own line", () => {
        // Given — plusieurs sous-agents en parallèle, chacun avec son état
        const tabs = tabsWithSubagents("nowhere");

        // When
        const shown = shownUnder(tabs, { groups: [], worktrees: [], columnCollapsed: false });

        // Then — l'onglet `claude`, ses deux enfants, puis les deux autres onglets
        expect(shown).toEqual(["working", "working", "done", "idle", "working"]);
    });

    it("Given a collapsed column, when a tab carries subagents, then the rail adds no line for them", () => {
        // Given — à 46 px, une ligne fille n'aurait rien pour se distinguer de la ligne de
        // son parent. C'est le glyphe du dépôt qui porte ce qui se passe dessous, et il le
        // porte déjà (`bubbleState`) : ajouter un glyphe par enfant remplirait le rail sans
        // dire lequel appartient à qui.
        const tabs = tabsWithSubagents("nowhere");

        // When
        const shown = shownUnder(tabs, { groups: [], worktrees: [], columnCollapsed: true });

        // Then — un état de dépôt, puis un glyphe par **onglet**
        expect(shown).toEqual(["working", "working", "idle", "working"]);
    });

    it("Given a column that shows no subagent at all, when it is drawn, then nothing asks for a per-second beat", () => {
        // Given — les durées des lignes filles avancent par un battement, et un battement
        // redessine la colonne entière. Sans sous-agent à l'écran — le cas de presque toutes
        // les colonnes, et de **toutes** celles d'un outil qui n'en expose pas — il n'a rien
        // à animer, et la sidebar doit rester dessinée sur événement.
        const withChildren = foldedTree(tabsWithSubagents("nowhere"), {
            groups: [],
            worktrees: [],
            columnCollapsed: false,
        });
        const without = foldedTree(tabsWith("nowhere"), {
            groups: [],
            worktrees: [],
            columnCollapsed: false,
        });

        // When / Then
        expect(showsSubagents(without, false)).toBe(false);
        expect(showsSubagents(withChildren, false)).toBe(true);
        expect(showsSubagents(withChildren, true)).toBe(false);
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

    it("Given a collapsed column, when the rows below it are folded in any combination, then the rail shows the same agents regardless", () => {
        // Given — à 46 px le rail aplatit le groupe (`planRailEntry`) : replier une ligne
        // en dessous ne doit rien lui retirer, sans quoi `⌘B` réduirait deux fois la même
        // colonne et masquerait exactement ce qu'on est venu y chercher. Sans ce test, le
        // chemin du rail n'est pincé par rien : le remplacer par celui de la colonne
        // dépliée laissait la suite verte.
        const tabs = tabsWith("in-api");
        const railFoldings = everyFolding().filter((folding) => folding.columnCollapsed);

        // When
        const renderings = new Set(
            railFoldings.map((folding) => shownUnder(tabs, folding).join()),
        );

        // Then — l'état du dépôt, puis un glyphe par agent, et une seule lecture pour les
        // huit replis
        expect(railFoldings).toHaveLength(8);
        expect([...renderings]).toEqual(["waiting,waiting,idle,working"]);
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
