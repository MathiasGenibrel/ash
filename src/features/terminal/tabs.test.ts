import { describe, expect, it } from "bun:test";

import { TabBuilder } from "@/shared/ipc/builders";
import type { TabInfo } from "./ports";
import {
    activeTab,
    adopt,
    cycle,
    noTabs,
    select,
    selectAt,
    withUpdates,
    type TabsState,
} from "./tabs";

/**
 * Test Data Builder : un état d'onglets décrit par l'ordre que le backend rendrait, et
 * par celui qu'on regarde. Les identifiants sont des lettres pour que les assertions se
 * lisent d'un coup d'œil.
 */
class TabsBuilder {
    private ids: string[] = [];
    private active: string | null = null;

    static create(): TabsBuilder {
        return new TabsBuilder();
    }

    inOrder(...ids: string[]): this {
        this.ids = ids;
        return this;
    }

    looking(at: string): this {
        this.active = at;
        return this;
    }

    build(): TabsState {
        return adopt(
            { ...noTabs, activeTabId: this.active },
            this.ids.map((tabId) => tab(tabId)),
        );
    }
}

/** Un onglet tel que le backend le décrit — cwd, programme, état, localisation. */
const tab = (tabId: string, cwd = `/dev/${tabId}`): TabInfo =>
    TabBuilder.create().named(tabId).inFlatWorktree(cwd).build();
const order = (state: TabsState): string[] => state.tabs.map((each) => each.tabId);

describe("l'ordre des onglets", () => {
    it("Given tabs the backend reordered, when the frontend adopts them, then it shows the backend order and not its own", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A", "B", "C").looking("B").build();

        // When — le backend a retiré « A » et ajouté « D » à la fin
        const adopted = adopt(state, [tab("B"), tab("C"), tab("D")]);

        // Then
        expect(order(adopted)).toEqual(["B", "C", "D"]);
        expect(adopted.activeTabId).toBe("B");
    });
});

describe("la sélection après une fermeture", () => {
    it("Given the tab being watched is closed, when the remaining tabs are adopted, then the one to its right takes over", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A", "B", "C").looking("B").build();

        // When
        const adopted = adopt(state, [tab("A"), tab("C")]);

        // Then
        expect(adopted.activeTabId).toBe("C");
    });

    it("Given the last tab of the bar is closed, when the remaining tabs are adopted, then the one to its left takes over", () => {
        // Given — il n'y a pas de « suivant » : le repli vers la gauche est la seule
        // façon de ne pas renvoyer l'utilisateur au début de la barre.
        const state = TabsBuilder.create().inOrder("A", "B", "C").looking("C").build();

        // When
        const adopted = adopt(state, [tab("A"), tab("B")]);

        // Then
        expect(adopted.activeTabId).toBe("B");
    });

    it("Given a tab nobody was watching is closed, when the remaining tabs are adopted, then the selection does not move", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A", "B", "C").looking("A").build();

        // When
        const adopted = adopt(state, [tab("A"), tab("C")]);

        // Then
        expect(adopted.activeTabId).toBe("A");
    });

    it("Given the only tab is closed, when the empty list is adopted, then nothing is selected any more", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A").looking("A").build();

        // When
        const adopted = adopt(state, []);

        // Then — la fenêtre reste ouverte, sans terminal : `⌘T` en rouvre un
        expect(adopted.activeTabId).toBeNull();
        expect(activeTab(adopted)).toBeNull();
    });
});

describe("Cmd+1..9", () => {
    it("Given a tab was closed in the middle, when Cmd+3 is pressed, then it selects the third tab of the bar and not the third ever opened", () => {
        // Given — l'ordre affiché est « A, C, D » après la fermeture de « B »
        const state = TabsBuilder.create().inOrder("A", "C", "D").looking("A").build();

        // When
        const selected = selectAt(state, 3);

        // Then
        expect(selected.activeTabId).toBe("D");
    });

    it("Given only three tabs are open, when Cmd+9 is pressed, then nothing changes", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A", "B", "C").looking("B").build();

        // When
        const selected = selectAt(state, 9);

        // Then — la spec dit « le n-ième onglet » ; sauter au dernier ferait de ⌘9 un
        // raccourci dont l'effet dépend du nombre d'onglets ouverts.
        expect(selected.activeTabId).toBe("B");
    });

    it("Given a tab that no longer exists, when it is selected by name, then the selection stays where it was", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A", "B").looking("A").build();

        // When
        const selected = select(state, "Z");

        // Then
        expect(selected.activeTabId).toBe("A");
    });
});

describe("Ctrl+Tab", () => {
    it("Given the tab in the middle of the bar, when the next one is asked for, then the selection moves one to the right", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A", "B", "C").looking("B").build();

        // When
        const cycled = cycle(state, 1);

        // Then
        expect(cycled.activeTabId).toBe("C");
    });

    it("Given the last tab of the bar, when the next one is asked for, then it wraps around to the first", () => {
        // Given — c'est là tout l'intérêt du raccourci : il ne s'arrête pas au bout
        const state = TabsBuilder.create().inOrder("A", "B", "C").looking("C").build();

        // When
        const cycled = cycle(state, 1);

        // Then
        expect(cycled.activeTabId).toBe("A");
    });

    it("Given the first tab of the bar, when the previous one is asked for, then it wraps around to the last", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A", "B", "C").looking("A").build();

        // When
        const cycled = cycle(state, -1);

        // Then
        expect(cycled.activeTabId).toBe("C");
    });

    it("Given a single open tab, when cycling in either direction, then the selection stays on it", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A").looking("A").build();

        // When
        const forwards = cycle(state, 1);
        const backwards = cycle(state, -1);

        // Then — le suivant du seul onglet, c'est lui-même : rien ne clignote
        expect(forwards.activeTabId).toBe("A");
        expect(backwards.activeTabId).toBe("A");
    });

    it("Given the tabs are cycled through in the backend order, when going forwards then backwards, then the selection comes back where it was", () => {
        // Given — l'ordre affiché est « A, C, D » après la fermeture de « B », et c'est
        // celui-là que le cycle suit ([ADR-0009])
        const state = TabsBuilder.create().inOrder("A", "C", "D").looking("C").build();

        // When
        const visited = [cycle(state, 1), cycle(cycle(state, 1), -1)];

        // Then
        expect(visited.map((each) => each.activeTabId)).toEqual(["D", "C"]);
    });

    it("Given no tab is open at all, when the next one is asked for, then nothing is selected", () => {
        // Given — la fenêtre reste ouverte quand le dernier shell est sorti
        // When
        const cycled = cycle(noTabs, 1);

        // Then
        expect(cycled.activeTabId).toBeNull();
    });
});

describe("les onglets que la sonde annonce", () => {
    it("Given a tab whose shell moved, when the probe announces it, then that tab shows the new directory and the others are untouched", () => {
        // Given
        const state = TabsBuilder.create().inOrder("A", "B").looking("A").build();

        // When
        const moved = withUpdates(state, [tab("A", "/tmp")]);

        // Then — un `cd` change un répertoire, pas l'ordre ni la sélection
        expect(moved.tabs.map((each) => each.cwd)).toEqual(["/tmp", "/dev/B"]);
        expect(order(moved)).toEqual(["A", "B"]);
        expect(moved.activeTabId).toBe("A");
    });

    it("Given a tab that changed repository, when the probe announces it, then the frontend adopts the location the backend resolved", () => {
        // Given — la sidebar ne résout rien de son côté : elle range ce que le backend
        // a situé ([ADR-0009])
        const state = TabsBuilder.create().inOrder("A").looking("A").build();
        const elsewhere = TabBuilder.create().named("A").inWorktree("/wt/ash-toc", "ash").build();

        // When
        const moved = withUpdates(state, [elsewhere]);

        // Then
        expect(moved.tabs[0]?.location?.repo?.name).toBe("ash");
    });

    it("Given a tab that has already closed, when a change still names it, then nothing is rendered again", () => {
        // Given — la passe de sonde et la fermeture d'un onglet se croisent
        const state = TabsBuilder.create().inOrder("A").looking("A").build();

        // When
        const applied = withUpdates(state, [tab("Z", "/tmp")]);

        // Then — l'état est rendu tel quel : rien à réafficher
        expect(applied).toBe(state);
    });
});
