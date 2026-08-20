/**
 * La moitié qui touche le DOM : l'ancrage, le clavier, le focus, et le retrait.
 *
 * Elle ne décide rien — le filtre, la sélection, les quatre étapes et les phrases vivent
 * dans [`branch-list`](./branch-list.ts), [`warning`](./warning.ts) et
 * [`popup`](./popup.ts), qui sont purs et testés. Ce fichier-ci se vérifie à la main : il
 * n'y a pas de `document` sous `bun test`.
 *
 * **Ancrée sur la branche du pied de fenêtre** (spec §7.1) : la popup est positionnée par
 * rapport à l'élément que la ligne de statut lui donne, et se replie si elle n'y tient pas.
 *
 * **Rien ne vole le focus, et rien n'agit sans un geste**
 * ([ADR-0010](../../../docs/adr/0010-la-sidebar-informe-le-terminal-agit.md),
 * [ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)) : la popup ne
 * s'ouvre que sur demande, elle rend les doigts au terminal en se fermant, et aucune de ses
 * commandes ne part d'un minuteur.
 */

import { FOCUS_KEY, paint } from "@/shared/ui";
import type { ActionOffer, Branch, BranchAction } from "@/shared/ipc";

import {
    keepSelection,
    moveSelection,
    selectedBranch,
    visibleRows,
    type BranchRow,
} from "./branch-list";
import {
    composeBranchPopup,
    CANCEL_FOCUS_KEY,
    FILTER_FOCUS_KEY,
    type PopupActions,
    type PopupModel,
    type PopupStage,
} from "./popup";
import type { AgentPause, BranchesBridge } from "./ports";
import type { PauseOffer } from "./warning";

/** De quoi ouvrir la popup, et savoir de quel worktree elle parle. */
export interface BranchPopupPorts {
    readonly branches: BranchesBridge;
    readonly agents: AgentPause;
    /**
     * La racine du worktree courant, relue **à chaque ouverture**.
     *
     * Une fonction et non une valeur : l'onglet actif change, et une popup qui garderait la
     * racine de son premier montage parlerait d'un autre dépôt que celui affiché.
     */
    readonly worktreeRoot: () => string | null;
    /** L'élément du pied de fenêtre sur lequel s'ancrer — la branche de la ligne de statut. */
    readonly anchor: () => HTMLElement | null;
    /** Rendre les doigts au terminal en se refermant. */
    readonly restoreFocus: () => void;
    /**
     * Le dépôt a bougé sous nos pieds : la ligne de statut et la sidebar ont à le relire.
     *
     * La popup ne les met pas à jour elle-même — elle ne détient rien. La surveillance de
     * `.git` d'ADR-0011 fera le reste ; ce rappel n'est là que pour ce qu'elle ne voit pas
     * immédiatement.
     */
    readonly onRepositoryChanged: () => void;
}

/** Ce que le composition root garde en main. */
export interface BranchPopup {
    /** `⌘⌃B`, ou un clic sur la branche du pied de fenêtre. Bascule. */
    toggle(): void;
    close(): void;
    readonly isOpen: boolean;
}

export function mountBranchPopup(host: HTMLElement, ports: BranchPopupPorts): BranchPopup {
    let overlay: HTMLElement | null = null;
    let model: PopupModel = {
        overview: null,
        query: "",
        rows: [],
        selected: -1,
        stage: { kind: "list" },
        running: false,
    };

    function open(): void {
        const root = ports.worktreeRoot();
        overlay = document.createElement("div");
        overlay.className = "branch-popup-overlay";
        overlay.setAttribute("role", "dialog");
        overlay.setAttribute("aria-label", "branches");
        host.append(overlay);
        document.addEventListener("keydown", onKey, true);

        model = {
            overview: null,
            query: "",
            rows: [],
            selected: -1,
            stage: { kind: "list" },
            running: false,
        };
        render();

        if (root === null) return;
        void ports.branches
            .branches(root)
            .then((overview) => {
                // La popup a pu se refermer pendant l'appel : on ne ressuscite rien.
                if (overlay === null) return;
                const rows = visibleRows(overview, model.query);
                model = { ...model, overview, rows, selected: keepSelection(rows, null) };
                render();
            })
            .catch(() => {
                // `git` n'a pas répondu : le rendu le dit déjà avec `overview` à `null`.
            });
    }

    function close(): void {
        if (overlay === null) return;
        document.removeEventListener("keydown", onKey, true);
        overlay.remove();
        overlay = null;
        ports.restoreFocus();
    }

    /** Revenir d'un cran : le sous-menu rend la liste, la liste referme. */
    function back(): void {
        if (model.stage.kind === "list") {
            close();
            return;
        }
        model = { ...model, stage: { kind: "list" }, running: false };
        render();
    }

    function filter(query: string): void {
        const kept = selectedBranch(model.rows, model.selected);
        const rows = visibleRows(model.overview, query);
        model = { ...model, query, rows, selected: keepSelection(rows, kept) };
        render();
    }

    function move(step: number): void {
        model = { ...model, selected: moveSelection(model.rows, model.selected, step) };
        render();
    }

    /**
     * Ouvre le sous-menu d'une branche, et y choisit une action tout de suite si on lui en
     * nomme une.
     *
     * Un seul chemin pour `⏎`, `⌘⏎`, le clic sur une ligne et le clic sur `⋯` : les quatre
     * gestes relisent les offres au même instant, donc la souris et le clavier ne peuvent
     * pas travailler sur deux états du dépôt différents.
     */
    function openActions(branch: Branch, straightTo: BranchAction | null = null): void {
        const root = ports.worktreeRoot();
        if (root === null) return;

        void ports.branches
            .offers(root, branch.name)
            .then((offers) => {
                if (overlay === null) return;
                const wanted =
                    straightTo === null
                        ? null
                        : (offers.find((offer) => offer.action === straightTo) ?? null);
                if (wanted !== null) {
                    pick(wanted, branch);
                    return;
                }
                model = { ...model, stage: { kind: "actions", branch, offers } };
                render();
            })
            .catch(() => {
                // Rien à proposer : la liste reste, et rien n'a été tenté.
            });
    }

    /**
     * Une action a été choisie : on la refuse, on la confirme, ou on la lance.
     *
     * La confirmation n'apparaît **que** quand l'action touche l'arbre et qu'un agent y
     * travaille (spec §7.1). C'est ce qui la garde lisible : une question posée à chaque
     * geste se clique sans être lue.
     */
    function pick(offer: ActionOffer, branch: Branch): void {
        if (offer.refused !== null) {
            model = {
                ...model,
                stage: {
                    kind: "outcome",
                    outcome: { label: offer.label, success: false, output: offer.refused },
                },
            };
            render();
            return;
        }

        const disturbs = offer.touchesTree && (model.overview?.agentsAtRisk.length ?? 0) > 0;
        if (disturbs) {
            model = { ...model, stage: { kind: "confirm", branch, offer } };
            render();
            return;
        }
        run(offer, branch);
    }

    function run(offer: ActionOffer, branch: Branch): void {
        const root = ports.worktreeRoot();
        if (root === null || model.running) return;

        model = { ...model, running: true };
        render();

        void ports.branches
            .run(root, offer.action, branch.name)
            .then((outcome) => {
                if (overlay === null) return;
                const answered = outcome ?? {
                    label: offer.label,
                    success: false,
                    output: "git could not be started",
                };
                // Une action qui a marché n'a rien à raconter : la branche du pied de fenêtre
                // et la sidebar disent déjà où l'on est. Un écran « done » qu'il faut fermer
                // ajoute un geste à chaque changement de branche, et c'est celui qu'on finit
                // par cliquer sans lire — donc celui qui masquerait un échec le jour où il
                // s'en présente un. L'échec, lui, garde son écran : il nomme ses deux côtés
                // et rapporte ce que git a dit (spec §7.1).
                if (answered.success) {
                    model = { ...model, running: false };
                    close();
                    ports.onRepositoryChanged();
                    return;
                }
                model = { ...model, running: false, stage: { kind: "outcome", outcome: answered } };
                render();
            })
            .catch(() => {
                if (overlay === null) return;
                model = { ...model, running: false };
                render();
            });
    }

    /** `SIGSTOP` ou `SIGCONT`, puis on relit la liste : l'avertissement doit se mettre à jour. */
    function pause(offer: PauseOffer): void {
        const signal = offer.resumes
            ? ports.agents.resume(offer.agent.tabId)
            : ports.agents.pause(offer.agent.tabId);

        void signal
            .then(() => refreshAgents())
            .catch(() => {
                // Le groupe a disparu, ou le système a refusé : l'avertissement reste tel
                // quel, ce qui est plus honnête que de le faire disparaître.
            });
    }

    /** Relit l'aperçu pour que l'avertissement dise l'état réel des agents. */
    function refreshAgents(): Promise<void> {
        const root = ports.worktreeRoot();
        if (root === null || overlay === null) return Promise.resolve();

        return ports.branches
            .branches(root)
            .then((overview) => {
                if (overlay === null || overview === null) return;
                model = { ...model, overview, rows: visibleRows(overview, model.query) };
                render();
            })
            .catch(() => undefined);
    }

    const actions: PopupActions = {
        filter,
        move,
        choose: (branch) => {
            openActions(branch, "checkout");
        },
        openActions: (branch) => {
            openActions(branch);
        },
        pick: (offer) => {
            const branch = branchOfStage(model.stage);
            if (branch !== null) pick(offer, branch);
        },
        proceed: () => {
            if (model.stage.kind === "confirm") run(model.stage.offer, model.stage.branch);
        },
        pause,
        back,
        close,
    };

    /**
     * Le clavier de la popup, en **capture** : le terminal a le focus, et xterm.js consomme
     * les touches. Sans ça, `⎋` partirait dans le shell au lieu de refermer.
     */
    function onKey(event: KeyboardEvent): void {
        if (overlay === null) return;

        if (event.key === "Escape") {
            event.preventDefault();
            back();
            return;
        }
        if (model.stage.kind !== "list") return;

        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            move(event.key === "ArrowDown" ? 1 : -1);
            return;
        }
        if (event.key === "Enter") {
            const branch = selectedBranch(model.rows, model.selected);
            if (branch === null) return;
            event.preventDefault();
            // `⌘⏎` ouvre les actions ; `⏎` seul fait le geste principal (spec §7.1).
            openActions(branch, event.metaKey ? null : "checkout");
        }
    }

    function render(): void {
        if (overlay === null) return;

        // Le champ est détruit et reconstruit à chaque rendu — donc pendant qu'on tape. On
        // relève la clé focalisée et la position du curseur avant, et on les repose après :
        // le même mécanisme que la fenêtre de réglages et que la boîte de recherche.
        const focused = document.activeElement;
        const key =
            focused instanceof HTMLElement ? focused.getAttribute(FOCUS_KEY) : null;
        const caret = focused instanceof HTMLInputElement ? focused.selectionStart : null;

        overlay.replaceChildren(paint(composeBranchPopup(model, actions).build()));
        anchorTo(overlay);

        const wanted =
            key ?? (model.stage.kind === "list" ? FILTER_FOCUS_KEY : CANCEL_FOCUS_KEY);
        const target = overlay.querySelector<HTMLElement>(`[${FOCUS_KEY}="${wanted}"]`);
        target?.focus();
        if (target instanceof HTMLInputElement && caret !== null) {
            target.setSelectionRange(caret, caret);
        }
    }

    /**
     * Pose la popup au-dessus de son ancre, et la ramène dans la fenêtre si elle déborde.
     *
     * Au-dessus, parce que l'ancre est au **pied** de la fenêtre : ouvrir vers le bas la
     * ferait sortir de l'écran. Sans ancre — la ligne de statut n'a pas de branche à
     * montrer —, elle se pose en bas à gauche plutôt que de ne pas s'ouvrir.
     */
    function anchorTo(element: HTMLElement): void {
        const anchor = ports.anchor();
        const bounds = anchor?.getBoundingClientRect();
        const left = bounds === undefined ? 8 : Math.max(8, bounds.left);
        element.style.left = `${String(Math.round(left))}px`;
        element.style.bottom = `${String(
            Math.round(bounds === undefined ? 8 : window.innerHeight - bounds.top + 4),
        )}px`;
    }

    return {
        toggle: () => {
            if (overlay === null) open();
            else close();
        },
        close,
        get isOpen() {
            return overlay !== null;
        },
    };
}

/** La branche dont le sous-menu ou la confirmation parle. */
function branchOfStage(stage: PopupStage): Branch | null {
    return stage.kind === "actions" || stage.kind === "confirm" ? stage.branch : null;
}

export type { BranchRow };
