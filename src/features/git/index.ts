/**
 * API publique de la feature git côté fenêtre : **la popup de branches** (spec §7.1), **le tableau
 * des worktrees** (spec §7.3) et **la fiche de branche** (spec §7.5).
 *
 * Le dossier portera aussi le graphe (#27) et l'onglet de merge (#30), écrits en parallèle. Ce fichier est leur point de rencontre, et le seul que `app/`
 * importe — jamais `controller`, `popup`, `bridge`, `worktree-table`, `card` ni `markdown`.
 *
 * **Cette feature ne détient aucun état d'agent ni aucun état git**
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : elle demande, elle
 * rend, et elle repose ses questions au backend à chaque ouverture. Ce qu'elle tient en
 * propre est ce que le backend n'a pas — le filtre en cours et la ligne sélectionnée.
 */

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

export { mountBranchCard, view as branchCardView, type BranchCardPorts, type BranchCardView } from "./card";
export { markdown, progressOf, readCard, type CardContent, type Meta, type TaskProgress } from "./markdown";
