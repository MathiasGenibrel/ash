import { describe, expect, it } from "bun:test";

import type { TabInfo } from "./ports";
import { activeTab, adopt, noTabs, select, selectAt, type TabsState } from "./tabs";

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
        return adopt({ ...noTabs, activeTabId: this.active }, this.ids.map(tab));
    }
}

const tab = (tabId: string): TabInfo => ({ tabId, cwd: `/dev/${tabId}` });
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
