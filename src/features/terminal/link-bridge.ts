import { invoke } from "@tauri-apps/api/core";

import type { LinkBridge } from "./links";

/**
 * L'implémentation réelle du pont vers `features::links` : deux commandes, et rien d'autre.
 *
 * Le pendant de `pty-bridge.ts` et de `git-bridge.ts`, et posé pour la même raison : la
 * feature qui consomme la capacité écrit le pont vers elle. Aucun event : ce que le backend
 * a à dire, il le dit en réponse à une question.
 */
export const tauriLinks: LinkBridge = {
    openable: (cwd, candidates) => invoke<string[]>("links_openable", { cwd, candidates }),
    open: (cwd, candidate) => invoke<void>("links_open", { cwd, candidate }),
};
