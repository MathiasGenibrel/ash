import { invoke } from "@tauri-apps/api/core";

import type { MergeOutcome, MergeView, TabId } from "@/shared/ipc";

/**
 * Ce que l'onglet de merge demande au backend — les six commandes de
 * `src-tauri/src/features/merge/commands.rs`, et rien d'autre.
 *
 * Un port et non des `invoke` dispersés : l'écran est une fonction pure, et ce qui l'entoure
 * doit pouvoir être doublé dans un test sans Tauri.
 */
export interface MergeBridge {
    /** Ouvre — ou retrouve — l'onglet de merge d'un worktree. */
    open(worktreeRoot: string): Promise<TabId>;
    close(tabId: TabId): Promise<void>;
    view(tabId: TabId): Promise<MergeView>;
    /** Tranche un hunk. Rend la vue **relue**, jamais une vue calculée ici. */
    resolve(tabId: TabId, path: string, hunk: number, resolution: string): Promise<MergeView>;
    proceed(tabId: TabId): Promise<MergeOutcome>;
    /**
     * Le prompt pour les conflits **restants**, ou `null` s'il n'en reste aucun.
     *
     * Il n'est écrit nulle part par cet appel : c'est `pty_compose` qui le pose dans un
     * terminal, et l'utilisateur seul qui l'envoie
     * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
     */
    restPrompt(tabId: TabId): Promise<string | null>;
}

export const tauriMerge: MergeBridge = {
    open: (worktreeRoot) => invoke<TabId>("merge_open", { worktreeRoot }),
    close: (tabId) => invoke("merge_close", { tabId }),
    view: (tabId) => invoke<MergeView>("merge_view", { tabId }),
    resolve: (tabId, path, hunk, resolution) =>
        invoke<MergeView>("merge_resolve", { tabId, path, hunk, resolution }),
    proceed: (tabId) => invoke<MergeOutcome>("merge_continue", { tabId }),
    restPrompt: (tabId) => invoke<string | null>("merge_rest_prompt", { tabId }),
};
