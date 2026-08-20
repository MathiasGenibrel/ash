import { paint } from "@/shared/ui";
import type { WorktreeRemoval, WorktreeRow } from "@/shared/ipc";

import "./worktree-table.css";
import { worktreeTable, type WorktreeTableActions } from "./table-view";

/**
 * Le tableau des worktrees, posé dans le corps du panneau bas (spec §7.3, #24).
 *
 * Tout ce qui décide est ailleurs : les lignes sont composées par le backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), leur mise en mots est une
 * valeur (`table-view.ts`). Il ne reste ici qu'un élément, un rafraîchissement, et la fiche
 * de suppression ouverte — qui est un fait d'affichage, pas un état du produit.
 *
 * **Rien n'y prend le focus, et rien n'y sélectionne tout seul** : le panneau ne vole pas le
 * clavier au terminal ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)), et
 * l'onglet d'un agent ne devient actif que sur un clic
 * ([ADR-0010](../../../docs/adr/0010-sidebar-informe-terminal-agit.md)).
 */
export interface WorktreeTablePorts {
    /** Les lignes, telles que le backend les compose. */
    list(): Promise<readonly WorktreeRow[]>;
    /** Ce qu'une suppression emporterait — relu au moment du geste, et qui ne supprime rien. */
    removal(worktreeRoot: string): Promise<WorktreeRemoval | null>;
    /** Aller à l'onglet d'un agent. */
    selectTab(tabId: string): void;
    /** Ouvrir un onglet dans un worktree que plus personne n'habite. */
    openTabIn(worktreeRoot: string): void;
    /**
     * La fiche de branche de ce worktree.
     *
     * **Le point de jonction avec #31** : le tableau nomme le worktree et sa branche, la fiche
     * les rendra. Tant qu'elle n'existe pas, le composition root se contente de faire montrer
     * la vue `branch` par le panneau — le renvoi est déjà là, et il n'y aura rien à changer
     * ici quand la fiche arrivera.
     */
    showCard(worktreeRoot: string, branch: string | null): void;
    /** L'heure, injectée : les durées affichées sont un fait d'affichage, et un test décide. */
    now(): number;
}

export interface WorktreeTable {
    readonly element: HTMLElement;
    /** Relit le tableau. Appelée quand la vue devient visible, et pas avant. */
    refresh(): void;
}

export function mountWorktreeTable(ports: WorktreeTablePorts): WorktreeTable {
    const element = document.createElement("div");
    element.className = "git-worktrees-host";

    let rows: readonly WorktreeRow[] = [];
    /**
     * La fiche de suppression ouverte, s'il y en a une.
     *
     * Elle vit ici et nulle part ailleurs : ce n'est pas un état du produit — rien n'est
     * décidé, rien n'est retenu, rien ne survit à la fermeture du panneau —, c'est la
     * question que l'utilisateur vient de poser. Ce qu'elle **dit**, en revanche, vient
     * entièrement du backend.
     */
    let showing: WorktreeRemoval | null = null;

    const actions: WorktreeTableActions = {
        selectTab: (tabId) => {
            ports.selectTab(tabId);
        },
        openTabIn: (worktreeRoot) => {
            ports.openTabIn(worktreeRoot);
        },
        showCard: (line) => {
            ports.showCard(
                line.worktreeRoot,
                line.metadata?.head.kind === "branch" ? line.metadata.head.name : null,
            );
        },
        askRemoval: (worktreeRoot) => {
            // Un plan qui n'arrive pas ne laisse **aucune moitié d'écran** : rien n'a été
            // décidé, rien n'a été supprimé, et la ligne reste exactement comme elle était.
            ports
                .removal(worktreeRoot)
                .then((plan) => {
                    showing = plan;
                    draw();
                })
                .catch(() => undefined);
        },
        dismissRemoval: () => {
            showing = null;
            draw();
        },
    };

    const draw = (): void => {
        element.replaceChildren(paint(worktreeTable(rows, ports.now(), showing, actions).build()));
    };

    draw();

    return {
        element,
        refresh() {
            ports
                .list()
                .then((fresh) => {
                    rows = fresh;
                    // La fiche **de suppression** ouverte ne survit pas à une relecture : ce
                    // qu'elle énonçait décrivait le worktree tel qu'il était il y a un
                    // instant, et une phrase
                    // périmée juste avant un geste destructeur est pire que pas de phrase.
                    showing = null;
                    draw();
                })
                .catch(() => undefined);
        },
    };
}
