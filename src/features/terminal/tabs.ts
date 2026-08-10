import type { TabId, TabInfo } from "./ports";

/**
 * Les règles d'onglets, sans DOM ni IPC.
 *
 * **L'ordre n'est pas décidé ici.** Il appartient au backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : ce module le reçoit
 * par [`adopt`] et n'en fabrique jamais. Ce qu'il décide, c'est la **sélection** — quel
 * onglet est visible, et lequel le devient quand celui qu'on regardait disparaît. La
 * sélection est une question d'affichage : elle n'a pas d'existence côté backend, et
 * aucun shell n'en dépend.
 */
export interface TabsState {
    /** Dans l'ordre du backend. C'est celui que `Cmd+1..9` numérote (spec §4.4). */
    readonly tabs: readonly TabInfo[];
    readonly activeTabId: TabId | null;
}

export const noTabs: TabsState = { tabs: [], activeTabId: null };

/**
 * Reprend l'ordre du backend, et rattrape la sélection s'il l'a emportée.
 *
 * Quand l'onglet actif n'est plus là — fermé à la main, ou shell sorti tout seul — le
 * suivant prend sa place ; à défaut, le précédent. C'est la règle des navigateurs et
 * d'iTerm2, et elle a une raison : fermer plusieurs onglets d'affilée laisse le curseur
 * là où il est, sans le renvoyer au début de la barre. Le repli vers la gauche évite le
 * seul cas où « le suivant » n'existe pas, celui du dernier onglet.
 */
export function adopt(state: TabsState, tabs: readonly TabInfo[]): TabsState {
    const stillThere = (tabId: TabId): boolean => tabs.some((tab) => tab.tabId === tabId);

    if (state.activeTabId !== null && stillThere(state.activeTabId)) {
        return { tabs, activeTabId: state.activeTabId };
    }

    // Le voisinage se lit dans l'**ancien** ordre : le nouveau ne contient plus l'onglet
    // disparu, donc il ne dit plus où il était.
    const was = state.tabs.findIndex((tab) => tab.tabId === state.activeTabId);
    if (was !== -1) {
        const toTheRight = state.tabs.slice(was + 1).find((tab) => stillThere(tab.tabId));
        if (toTheRight !== undefined) return { tabs, activeTabId: toTheRight.tabId };

        const toTheLeft = state.tabs
            .slice(0, was)
            .reverse()
            .find((tab) => stillThere(tab.tabId));
        if (toTheLeft !== undefined) return { tabs, activeTabId: toTheLeft.tabId };
    }

    return { tabs, activeTabId: tabs[0]?.tabId ?? null };
}

/**
 * Reprend les onglets que la boucle de sonde du backend annonce.
 *
 * Rien n'est calculé ni deviné ici : le répertoire, l'état et la **localisation** viennent
 * du backend, qui seul les détient
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). L'ordre et la
 * sélection, eux, ne bougent pas — un `cd` n'est pas une ouverture d'onglet, même quand il
 * fait changer l'onglet de dépôt.
 *
 * Rend l'état **inchangé**, à l'identique, quand rien ne s'applique : un changement qui
 * ne concerne que des onglets déjà fermés ne doit pas provoquer de rendu.
 */
export function withUpdates(state: TabsState, changed: readonly TabInfo[]): TabsState {
    const announced = new Map(changed.map((tab) => [tab.tabId, tab]));

    let touched = false;
    const tabs = state.tabs.map((tab) => {
        const update = announced.get(tab.tabId);
        if (update === undefined || sameTab(update, tab)) return tab;
        touched = true;
        return update;
    });

    return touched ? { tabs, activeTabId: state.activeTabId } : state;
}

/** Deux descriptions d'un même onglet qui ne changeraient rien à l'affichage. */
function sameTab(one: TabInfo, other: TabInfo): boolean {
    return (
        one.cwd === other.cwd &&
        one.process === other.process &&
        one.state === other.state &&
        one.location?.worktreeRoot === other.location?.worktreeRoot &&
        one.location?.repo?.id === other.location?.repo?.id
    );
}

/** Sélectionne un onglet nommé. Un identifiant inconnu ne change rien. */
export function select(state: TabsState, tabId: TabId): TabsState {
    if (!state.tabs.some((tab) => tab.tabId === tabId)) return state;
    return { tabs: state.tabs, activeTabId: tabId };
}

/**
 * Sélectionne le n-ième onglet, à partir de 1 — `Cmd+1` … `Cmd+9`.
 *
 * Une position vide ne change rien : `Cmd+9` avec trois onglets ne fait rien. La spec
 * §4.4 dit « le n-ième onglet », pas « le dernier », et sauter au dernier ferait de
 * `Cmd+9` un raccourci dont l'effet dépend du nombre d'onglets ouverts.
 */
export function selectAt(state: TabsState, position: number): TabsState {
    const tab = state.tabs[position - 1];
    if (tab === undefined) return state;
    return { tabs: state.tabs, activeTabId: tab.tabId };
}

export function activeTab(state: TabsState): TabInfo | null {
    return state.tabs.find((tab) => tab.tabId === state.activeTabId) ?? null;
}
