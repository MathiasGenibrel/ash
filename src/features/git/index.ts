/**
 * API publique de la feature git côté fenêtre : **la popup de branches** (spec §7.1), **le
 * graphe de commits** (spec §7.2), **le tableau des worktrees** (spec §7.3), **la vue des
 * conflits** (spec §7.4) et **la fiche de branche** (spec §7.5).
 *
 * Ce fichier est le point de rencontre des cinq vues, et le seul que `app/` importe — jamais
 * `controller`, `popup`, `bridge`, `graph`, `worktree-table`, `conflicts`, `card` ni
 * `markdown`.
 *
 * L'onglet de merge lui-même n'est **pas** ici : il vit dans `features/merge`, et la vue des
 * conflits n'en est que la porte d'entrée — elle demande son ouverture, elle ne la dessine
 * pas.
 *
 * **Cette feature ne détient aucun état d'agent ni aucun état git**
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : elle demande, elle
 * rend, et elle repose ses questions au backend à chaque ouverture. Les couloirs du graphe,
 * sa colonne `by` et le repli des branches inactives sont décidés en Rust. Ce qu'elle tient
 * en propre est ce que le backend n'a pas — le filtre en cours, la ligne sélectionnée, la
 * ligne dont le détail est ouvert, la fenêtre déjà demandée.
 *
 * **Une seule de ses vues lance une action git** : la popup de branches, sur un geste
 * explicite. Le graphe, le tableau, la fiche et la vue des conflits **lisent** — le seul
 * geste de cette dernière ouvre un onglet, il n'écrit pas une ligne dans le dépôt.
 */

export type { CommitGraph, CommitRow, FoldedBranch, GraphLink } from "./contract";
export { mountCommitGraph, type CommitGraphPanel, type CommitGraphPorts } from "./graph";
export { WINDOW_STEP } from "./graph-view";

export {
    mountWorktreeTable,
    type WorktreeTable,
    type WorktreeTablePorts,
} from "./worktree-table";

import "./git.css";

import { mountBranchPopup, type BranchPopup, type BranchPopupPorts } from "./controller";
import { tauriBranches, tauriPause } from "./bridge";

export type { BranchPopup, BranchPopupPorts } from "./controller";
export type { AgentPause, BranchesBridge } from "./ports";

/** Ce que le composition root passe : où l'on est, où s'ancrer, et à qui rendre les doigts. */
export interface BranchPopupSetup {
    readonly worktreeRoot: () => string | null;
    readonly anchor: () => HTMLElement | null;
    readonly restoreFocus: () => void;
    readonly onRepositoryChanged: () => void;
}

/**
 * Monte la popup dans `host`, branchée sur les vraies commandes Tauri.
 *
 * Rien n'est ouvert ici : la popup n'apparaît que sur un geste — le raccourci, ou un clic
 * sur la branche du pied de fenêtre.
 */
export function mountBranches(host: HTMLElement, setup: BranchPopupSetup): BranchPopup {
    const ports: BranchPopupPorts = {
        branches: tauriBranches,
        agents: tauriPause,
        worktreeRoot: setup.worktreeRoot,
        anchor: setup.anchor,
        restoreFocus: setup.restoreFocus,
        onRepositoryChanged: setup.onRepositoryChanged,
    };
    return mountBranchPopup(host, ports);
}

export { composeBranchPopup, type PopupActions, type PopupModel, type PopupStage } from "./popup";
export { warnAbout, pauseOffers, type PauseOffer } from "./warning";
export {
    keepSelection,
    moveSelection,
    selectedBranch,
    visibleRows,
    type BranchRow,
} from "./branch-list";

/**
 * La vue `conflicts` du panneau bas — **minimale**, posée par #30 pour avoir une porte
 * d'entrée vers l'onglet de merge. Voir l'en-tête de `conflicts.ts` : #29 la remplacera.
 */
export { conflictsView, paintConflicts, type ConflictsActions } from "./conflicts";
export { stoppedOperation } from "./bridge";

export {
    mountBranchCard,
    view as branchCardView,
    type BranchCardPorts,
    type BranchCardView,
} from "./card";
export {
    markdown,
    progressOf,
    readCard,
    type CardContent,
    type Meta,
    type TaskProgress,
} from "./markdown";
