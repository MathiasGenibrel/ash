/**
 * API publique de l'onglet de merge (spec §7.4, issue #30).
 *
 * Le reste du frontend n'importe que ce fichier : ni `screen`, ni `bridge`, ni `editor` ne
 * sont des points d'entrée.
 *
 * **C'est le premier onglet sans PTY**
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md), reformulation du
 * 2026-08-10 : « un onglet est soit un terminal, soit une surface d'outil »). Il ne
 * détient rien : le compte de conflits, les hunks, les noms des côtés et le droit de
 * continuer viennent tous du backend, et sont **relus** après chaque geste
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */

import "./merge.css";

import type { MergeTab, MergeView, TabId } from "@/shared/ipc";
import { paint } from "@/shared/ui";

import { tauriMerge, type MergeBridge } from "./bridge";
import {
    currentFile,
    currentHunk,
    mergeScreen,
    NO_SELECTION,
    type MergeSelection,
} from "./screen";

export { tauriMerge, type MergeBridge } from "./bridge";
export {
    currentFile,
    currentHunk,
    mergeScreen,
    NO_SELECTION,
    type MergeActions,
    type MergeSelection,
} from "./screen";

/**
 * Ce que la surface ne sait pas faire elle-même.
 *
 * `agentTab` et `writePrompt` viennent du composition root : la feature ne connaît ni les
 * onglets de shell ni le pupitre de composition, et c'est la fenêtre qui relie les deux —
 * exactement comme pour la vue `conflicts` du panneau bas.
 */
export interface MergeSurfaceDeps {
    readonly bridge: MergeBridge;
    /**
     * L'onglet où poser le prompt : un shell **du même worktree** qui porte un agent
     * reconnu. `null` quand il n'y en a aucun — et la surface le dit plutôt que d'en ouvrir
     * un : ouvrir un onglet est un geste de l'utilisateur (ADR-0010).
     */
    agentTab(worktreeRoot: string): TabId | null;
    /**
     * Écrit un prompt dans un onglet — **sans l'envoyer**. Le chemin est celui de #29, et
     * il n'y en a pas de second ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
     */
    writePrompt(prompt: string | null, tabId: TabId): Promise<string | null>;
}

/** Ce que l'atelier attend d'une surface d'outil. Miroir de `ToolSurface` côté terminal. */
export interface MergeSurface {
    readonly element: HTMLElement;
    setVisible(visible: boolean): void;
    close(): Promise<void>;
}

/**
 * Monte la surface d'un onglet de merge.
 *
 * Elle se redessine entièrement après chaque geste, à partir d'une vue **relue** au
 * backend : il n'y a pas de diffing, et surtout pas de copie de l'état des conflits côté
 * webview. Ce qui vit ici est la seule chose que le backend n'a pas — la sélection, et ce
 * qui est tapé dans le panneau du milieu tant que ce n'est pas appliqué.
 */
export function createMergeSurface(tab: MergeTab, deps: MergeSurfaceDeps): MergeSurface {
    const element = document.createElement("div");
    element.className = "merge-surface";

    let view: MergeView = {
        tabId: tab.tabId,
        worktreeRoot: tab.worktreeRoot,
        title: tab.title,
        stopped: null,
    };
    let selection: MergeSelection = NO_SELECTION;
    let notice: string | null = null;

    function draw(): void {
        element.replaceChildren(
            paint(
                mergeScreen(view, selection, notice, {
                    selectFile: (path) => {
                        // Changer de fichier remet le panneau du milieu à zéro : ce qui y
                        // était tapé parlait d'un autre conflit.
                        selection = { path, hunk: 0, draft: "" };
                        draw();
                    },
                    selectHunk: (index) => {
                        const file = currentFile(view, selection);
                        const count = file?.hunks.length ?? 0;
                        if (count === 0) return;
                        // On boucle, comme `⌃⇥` : sans ça, les deux flèches ne feraient
                        // rien aux deux bouts, et il faudrait regarder où l'on est pour
                        // savoir si elles vont répondre.
                        const at = ((index % count) + count) % count;
                        selection = { path: file?.path ?? null, hunk: at, draft: "" };
                        draw();
                    },
                    edit: (draft) => {
                        // Pas de `draw()` : la zone de saisie est déjà à jour, et la
                        // repeindre reposerait le curseur au début à chaque frappe.
                        selection = { ...selection, draft };
                    },
                    take: (side) => {
                        const hunk = currentHunk(currentFile(view, selection), selection);
                        if (hunk === null) return;
                        // Recopier un côté est un **point de départ**, pas une décision :
                        // le texte reste éditable, et rien n'est écrit tant qu'on n'a pas
                        // appliqué.
                        selection = {
                            ...selection,
                            draft: side === "left" ? hunk.ours : hunk.theirs,
                        };
                        draw();
                    },
                    apply: () => {
                        void applyHunk();
                    },
                    proceed: () => {
                        void proceed();
                    },
                    handOverRest: () => {
                        void handOverRest();
                    },
                }).build(),
            ),
        );
    }

    async function reload(): Promise<void> {
        try {
            view = await deps.bridge.view(tab.tabId);
        } catch (why) {
            notice = String(why);
        }
        draw();
    }

    async function applyHunk(): Promise<void> {
        const file = currentFile(view, selection);
        const hunk = currentHunk(file, selection);
        if (file === null || hunk === null) return;
        try {
            view = await deps.bridge.resolve(tab.tabId, file.path, hunk.index, selection.draft);
            notice = null;
        } catch (why) {
            notice = String(why);
        }
        // Le hunk suivant du même fichier, ou le fichier suivant : le rang zéro est
        // toujours le bon, puisque celui qu'on vient de trancher a disparu du fichier.
        selection = { path: file.path, hunk: 0, draft: "" };
        draw();
    }

    async function proceed(): Promise<void> {
        try {
            const outcome = await deps.bridge.proceed(tab.tabId);
            notice = `${outcome.label} — ${outcome.output.length === 0 ? (outcome.success ? "done" : "refused") : outcome.output}`;
        } catch (why) {
            notice = String(why);
        }
        await reload();
    }

    async function handOverRest(): Promise<void> {
        const tabId = deps.agentTab(view.worktreeRoot);
        if (tabId === null) {
            notice = "no agent is running in this worktree — open one and try again";
            draw();
            return;
        }
        try {
            const prompt = await deps.bridge.restPrompt(tab.tabId);
            notice =
                (await deps.writePrompt(prompt, tabId)) ??
                "nothing left to hand over — ash wrote nothing";
        } catch (why) {
            notice = String(why);
        }
        draw();
    }

    draw();
    void reload();

    return {
        element,
        setVisible: (visible) => {
            element.style.display = visible ? "" : "none";
        },
        close: () => tauriClose(deps.bridge, tab.tabId),
    };
}

function tauriClose(bridge: MergeBridge, tabId: TabId): Promise<void> {
    return bridge.close(tabId);
}

/** Le pont réel, pour le composition root. */
export const mergeBridge = tauriMerge;
