import { describe, expect, it } from "bun:test";

import type { Tab } from "@/shared/ipc";
import { TabBuilder } from "@/shared/ipc/builders";
import { composeSidebarHeader, type SidebarHeaderModel } from "./header";
import { buildSidebar } from "./tree";

/** Sept agents, dont `waiting` d'entre eux — le décor du `1 waiting / 7 agents` de la spec. */
function sevenAgents(waiting: number): readonly Tab[] {
    return Array.from({ length: 7 }, (_, index) =>
        TabBuilder.create()
            .named(`T${index}`)
            .running("claude", index < waiting ? "waiting" : "working")
            .inFlatWorktree(`/dev/project-${index % 3}`)
            .build(),
    );
}

function header(tabs: readonly Tab[], columnCollapsed: boolean): SidebarHeaderModel {
    const tree = buildSidebar(tabs, {
        activeTabId: null,
        collapsed: new Set(),
        pinned: [],
    });
    return composeSidebarHeader(tree, columnCollapsed);
}

const words = (model: SidebarHeaderModel): string =>
    model.shape === "full" ? model.chips.map((chip) => chip.text).join(" ") : "";

describe("le compteur agrégé de l'en-tête", () => {
    it("Given seven agents of which one is waiting, when the header is composed, then it reads one waiting out of seven", () => {
        // Given / When
        const model = header(sevenAgents(1), false);

        // Then
        expect(words(model)).toBe("1 waiting / 7 agents");
    });

    it("Given seven agents and none waiting, when the header is composed, then it drops the waiting counter instead of showing a zero", () => {
        // Given — un `0 waiting` permanent apprendrait à l'œil à ignorer cette place, ce qui
        // est exactement ce qu'on ne peut pas se permettre pour le seul état qui alerte
        const model = header(sevenAgents(0), false);

        // Then
        expect(words(model)).toBe("7 agents");
    });

    it("Given a waiting agent, when the column is collapsed, then the header still says how many wait", () => {
        // Given — spec §4.1 : le compteur reste visible sidebar repliée. À 46 px la phrase
        // ne tient pas, mais l'information qui compte, si.
        const model = header(sevenAgents(3), true);

        // Then
        expect(model.shape).toBe("compact");
        if (model.shape !== "compact") return;
        expect(model.badge).toEqual({ state: "waiting", count: 3, urgent: true });
    });

    it("Given a collapsed column, when the header is composed, then the long sentence survives as its accessible summary", () => {
        // Given — replier abrège l'affichage, pas l'information : un lecteur d'écran et une
        // infobulle lisent la même phrase dans les deux formes
        const compact = header(sevenAgents(1), true);
        const full = header(sevenAgents(1), false);

        // Then
        expect(compact.summary).toBe("1 waiting / 7 agents");
        expect(full.summary).toBe(compact.summary);
    });

    it("Given no agent waiting, when the column is collapsed, then the badge falls back to the plain agent count and stays untinted", () => {
        // Given — `waiting` est le seul état teinté de l'interface ; un badge toujours
        // accentué ferait perdre ce test à la colonne entière
        const model = header(sevenAgents(0), true);

        // Then
        expect(model.shape).toBe("compact");
        if (model.shape !== "compact") return;
        expect(model.badge).toEqual({ state: null, count: 7, urgent: false });
    });

    it("Given no tab at all, when the header is composed, then it says so rather than counting zero waiting", () => {
        // Given / When
        const model = header([], true);

        // Then
        expect(model.summary).toBe("no agents");
        if (model.shape !== "compact") return;
        expect(model.badge.urgent).toBe(false);
    });
});
