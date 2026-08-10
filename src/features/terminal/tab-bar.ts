import type { TabId, TabInfo } from "./ports";
import type { TabsState } from "./tabs";

/**
 * Ce que la barre sait déclencher. Les mêmes actions que le menu applicatif — la spec
 * §4.4 demande que tout soit atteignable à la souris.
 */
export interface TabBarActions {
    select(tabId: TabId): void;
    close(tabId: TabId): void;
    openInCurrentWorktree(): void;
    openAtHome(): void;
    clearActive(): void;
}

/**
 * La barre d'onglets : ordre du backend, onglet actif nettement marqué, bouton `+`
 * (spec §4.2).
 *
 * Elle ne décide rien — elle affiche un `TabsState` et rend des intentions. Le DOM est
 * reconstruit à chaque rendu : quelques nœuds par onglet, contre le risque bien réel
 * d'une barre qui diverge de l'ordre que le backend détient.
 */
export class TabBar {
    readonly element: HTMLElement;

    constructor(private readonly actions: TabBarActions) {
        this.element = document.createElement("div");
        this.element.className = "terminal-bar";
        this.element.setAttribute("role", "tablist");
    }

    render(state: TabsState): void {
        this.element.replaceChildren(
            ...state.tabs.map((tab, index) =>
                this.tabButton(tab, index + 1, tab.tabId === state.activeTabId),
            ),
            this.controls(state),
        );
    }

    private tabButton(tab: TabInfo, position: number, active: boolean): HTMLElement {
        const row = document.createElement("div");
        row.className = active ? "terminal-tab is-active" : "terminal-tab";

        const label = document.createElement("button");
        label.type = "button";
        label.className = "terminal-tab-label";
        label.setAttribute("role", "tab");
        label.setAttribute("aria-selected", String(active));
        // Le titre montre le répertoire de lancement : c'est tout ce qu'on sait de
        // l'onglet tant que la sonde `cwd` (ADR-0005) n'existe pas.
        label.title = tab.startDir;
        label.textContent =
            position <= 9 ? `⌘${position} ${basename(tab.startDir)}` : basename(tab.startDir);
        label.addEventListener("click", () => {
            this.actions.select(tab.tabId);
        });

        const close = document.createElement("button");
        close.type = "button";
        close.className = "terminal-tab-close";
        close.title = "Fermer l'onglet (⌘W)";
        close.setAttribute("aria-label", `Fermer ${basename(tab.startDir)}`);
        close.textContent = "×";
        close.addEventListener("click", (event) => {
            // Sans ça, le clic sélectionnerait l'onglet avant de le fermer, et la
            // confirmation s'afficherait sur un onglet qui vient de prendre le focus.
            event.stopPropagation();
            this.actions.close(tab.tabId);
        });

        row.append(label, close);
        return row;
    }

    private controls(state: TabsState): HTMLElement {
        const group = document.createElement("div");
        group.className = "terminal-bar-controls";

        group.append(
            button("+", "Nouvel onglet dans le worktree courant (⌘N)", () => {
                this.actions.openInCurrentWorktree();
            }),
            button("+~", "Nouvel onglet à ~ (⇧⌘N)", () => {
                this.actions.openAtHome();
            }),
        );

        // Effacer le scrollback n'a pas de sens sans onglet ; le bouton disparaît plutôt
        // que de rester là, désactivé, à ne rien vouloir dire.
        if (state.activeTabId !== null) {
            group.append(
                button("⌫", "Effacer le scrollback (⌘K)", () => {
                    this.actions.clearActive();
                }),
            );
        }

        return group;
    }
}

function button(text: string, title: string, onClick: () => void): HTMLButtonElement {
    const element = document.createElement("button");
    element.type = "button";
    element.className = "terminal-bar-button";
    element.title = title;
    element.setAttribute("aria-label", title);
    element.textContent = text;
    element.addEventListener("click", onClick);
    return element;
}

/** Dernier segment d'un chemin — `~` reste `~`, `/` reste `/`. */
function basename(path: string): string {
    const segments = path.split("/").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? "/";
}
