/**
 * API publique de la feature terminal.
 *
 * Le reste du frontend n'importe que ce fichier : ni `xterm-view`, ni `pty-bridge`, ni
 * `workbench` ne sont des points d'entrée.
 */

import "./terminal.css";

import type { TabId, TabInfo } from "./ports";
import { askToClose } from "./confirm-dialog";
import { tauriPty } from "./pty-bridge";
import { TabBar } from "./tab-bar";
import { XtermView } from "./xterm-view";
import { TerminalWorkbench, type Origin } from "./workbench";

export type { PtyFrame, TabId, TabInfo, TerminalSize } from "./ports";
export type { Origin } from "./workbench";

/** Ce que la feature annonce de ses onglets à qui les affiche autrement — la sidebar. */
export type TabsListener = (tabs: readonly TabInfo[], activeTabId: TabId | null) => void;

/** Les actions d'onglet, telles que le menu applicatif et la barre les déclenchent. */
export interface Terminals {
    openTab(origin: Origin): Promise<void>;
    closeActiveTab(): Promise<void>;
    selectTab(tabId: TabId): Promise<void>;
    selectTabAt(position: number): Promise<void>;
    clearActiveScrollback(): Promise<void>;
    /**
     * S'abonne à l'état des onglets.
     *
     * La feature ne connaît pas la sidebar : c'est le composition root qui relie les deux.
     * Et il n'y a **qu'un** abonnement à la boucle de sonde — deux features qui écouteraient
     * le même event afficheraient deux vérités qui se croisent
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    onTabs(listener: TabsListener): void;
    /**
     * Les titres d'onglet portent-ils leur localisation — dépôt, ou worktree à défaut ?
     *
     * `⌘B` replie la sidebar : le contexte qu'elle portait doit passer dans la barre.
     */
    showLocationInTitles(show: boolean): void;
}

/**
 * Monte la barre d'onglets et la pile de terminaux dans `host`.
 *
 * Rien n'est ouvert ici : c'est au composition root de décider que l'application démarre
 * sur un onglet.
 *
 * Un onglet porte au plus un PTY, et un seul terminal est visible à la fois
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)).
 */
export function mountTerminals(host: HTMLElement): Terminals {
    host.classList.add("terminal-workbench");

    // La pile est un conteneur positionné : chaque onglet s'y superpose en occupant
    // toute la surface, et seul l'actif est visible. Voir `xterm-view.ts` — les onglets
    // masqués gardent leur taille, sans quoi leur grille serait détruite au retour.
    const stack = document.createElement("div");
    stack.className = "terminal-stack";

    const listeners: TabsListener[] = [];

    const workbench = new TerminalWorkbench({
        bridge: tauriPty,
        createView: () => new XtermView(stack),
        confirmClose: (tab) => askToClose(host, tab.cwd),
        onRender: (state) => {
            bar.render(state);
            for (const listener of listeners) listener(state.tabs, state.activeTabId);
        },
    });

    const bar = new TabBar({
        select: (tabId) => void workbench.select(tabId),
        close: (tabId) => void workbench.closeTab(tabId),
        openInCurrentWorktree: () => void workbench.openTab("current-worktree"),
        openAtHome: () => void workbench.openTab("home"),
        clearActive: () => void workbench.clearActive(),
    });

    bar.render(workbench.tabs);
    host.append(bar.element, stack);

    return {
        openTab: (origin) => workbench.openTab(origin),
        closeActiveTab: () => workbench.closeActive(),
        selectTab: (tabId) => workbench.select(tabId),
        selectTabAt: (position) => workbench.selectAt(position),
        clearActiveScrollback: () => workbench.clearActive(),
        onTabs: (listener) => {
            listeners.push(listener);
            // L'abonné arrive après le premier rendu : lui donner l'état courant tout de
            // suite lui évite d'attendre le prochain `cd` pour afficher quoi que ce soit.
            listener(workbench.tabs.tabs, workbench.tabs.activeTabId);
        },
        showLocationInTitles: (show) => {
            if (bar.showLocationInTitles(show)) bar.render(workbench.tabs);
        },
    };
}
