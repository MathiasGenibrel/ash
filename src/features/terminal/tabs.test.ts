import { describe, expect, it } from "bun:test";

import { TabBuilder } from "@/shared/ipc/builders";
import type { TabInfo } from "./ports";
import { activeTab, adopt, noTabs, select, selectAt, withUpdates, type TabsState } from "./tabs";

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

        // Then — la fenêtre reste ouverte, sans terminal : `⌘N` en rouvre un
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
