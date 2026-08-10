/**
 * API publique de la feature terminal.
 *
 * Le reste du frontend n'importe que ce fichier : ni `xterm-view`, ni `pty-bridge`, ni
 * `workbench` ne sont des points d'entrée.
 */

import { askToClose } from "./confirm-dialog";
import { tauriPty } from "./pty-bridge";
import { TabBar } from "./tab-bar";
import { XtermView } from "./xterm-view";
import { TerminalWorkbench, type Origin } from "./workbench";

export type { PtyFrame, TabId, TabInfo, TerminalSize } from "./ports";
export type { Origin } from "./workbench";

/** Les actions d'onglet, telles que le menu applicatif et la barre les déclenchent. */
export interface Terminals {
    openTab(origin: Origin): Promise<void>;
    closeActiveTab(): Promise<void>;
    selectTabAt(position: number): Promise<void>;
    clearActiveScrollback(): Promise<void>;
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

    const workbench = new TerminalWorkbench({
        bridge: tauriPty,
        createView: () => new XtermView(stack),
        confirmClose: (tab) => askToClose(host, tab.startDir),
        onRender: (state) => {
            bar.render(state);
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
        selectTabAt: (position) => workbench.selectAt(position),
        clearActiveScrollback: () => workbench.clearActive(),
    };
}
