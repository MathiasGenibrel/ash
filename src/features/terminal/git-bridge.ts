import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { StoppedOperation, WorktreeMetadata, WorktreeMetadataChanged } from "@/shared/ipc";
import type { GitBridge } from "./ports";

/**
 * Nom de l'event que la surveillance git émet. Contrat avec `METADATA_CHANGED_EVENT` dans
 * `src-tauri/src/features/git/commands.rs` : une chaîne que rien ne vérifie à la
 * compilation, comme celle des onglets et celle du menu.
 */
const METADATA_CHANGED_EVENT = "ash://git-metadata";

/**
 * L'implémentation réelle du pont vers `features::git` : une commande, un event, et rien
 * d'autre.
 *
 * Le pendant de `pty-bridge.ts`, et posé pour la même raison : la feature qui consomme
 * l'état git est celle qui écrit le pont vers lui.
 */
export const tauriGit: GitBridge = {
    metadata: (worktreeRoot) => invoke<WorktreeMetadata | null>("git_metadata", { worktreeRoot }),
    onMetadataChanged: (handler) =>
        listen<WorktreeMetadataChanged>(METADATA_CHANGED_EVENT, (event) => {
            handler(event.payload);
        }),
    stoppedOperation: (worktreeRoot) =>
        invoke<StoppedOperation | null>("git_stopped_operation", { worktreeRoot }),
    conflictPrompt: (worktreeRoot) =>
        invoke<string | null>("git_conflict_prompt", { worktreeRoot }),
};
