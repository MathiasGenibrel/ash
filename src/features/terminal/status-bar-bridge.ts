import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { StatusBarBridge } from "./ports";
import { parseStatusBarSegments } from "./status-bar";

/**
 * Nom de l'event qui porte ce que la ligne de statut montre. Contrat avec
 * `STATUS_BAR_SEGMENTS_EVENT` dans `src-tauri/src/features/theme/commands.rs` : une chaîne
 * que rien ne vérifie à la compilation, comme celle des quotas et celle des onglets.
 */
const STATUS_BAR_SEGMENTS_EVENT = "ash://status-bar-segments";

/**
 * Le pont vers les sept interrupteurs de la vue 5c : **une lecture, une bascule, un event**.
 *
 * Posé dans la feature terminal parce que c'est elle qui les consomme — la ligne de statut
 * et son menu contextuel —, comme `usage-bridge.ts` l'est pour les quotas. Le couple est
 * celui du thème : on lit une fois en s'affichant, puis c'est l'event qui tient à jour, et
 * la webview ne redemande jamais.
 */
export const tauriStatusBar: StatusBarBridge = {
    segments: async () => parseStatusBarSegments(await invoke<unknown>("status_bar_segments")),
    toggle: (segment) => invoke<void>("toggle_status_bar_segment", { segment }),
    onSegments: (handler) =>
        listen<unknown>(STATUS_BAR_SEGMENTS_EVENT, (event) => {
            handler(parseStatusBarSegments(event.payload));
        }),
};
