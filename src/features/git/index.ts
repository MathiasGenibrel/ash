/**
 * API publique de la feature git côté webview — pour l'instant, **la popup de branches**
 * et rien d'autre (spec §7.1).
 *
 * Le dossier est prévu pour porter aussi le graphe, l'onglet de merge et la fiche de
 * branche : ils appartiennent à d'autres tranches, et rien n'est posé d'avance ici. Ce que
 * le reste du frontend importe est ce fichier, jamais `controller`, `popup` ni `bridge`.
 *
 * **Cette feature ne détient aucun état d'agent ni aucun état git**
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : elle demande, elle
 * rend, et elle repose ses questions au backend à chaque ouverture. Ce qu'elle tient en
 * propre est ce que le backend n'a pas — le filtre en cours et la ligne sélectionnée.
 */

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
