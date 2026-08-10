import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { PtyBridge, PtyFrame, TabId, TabInfo, TerminalSize } from "./ports";

/**
 * Nom de l'event que la boucle de sonde émet. Contrat avec `TAB_CHANGED_EVENT` dans
 * `src-tauri/src/features/pty/commands.rs` : une chaîne que rien ne vérifie à la
 * compilation, comme celle du menu applicatif.
 */
const TAB_CHANGED_EVENT = "ash://tab-changed";

/**
 * L'implémentation réelle du pont : les sept commandes et l'event déclarés par
 * `src-tauri/src/features/pty/commands.rs`, et rien d'autre.
 *
 * Le frontend ne connaît que ces noms et le type `PtyFrame` — jamais la structure
 * interne du backend.
 */
export const tauriPty: PtyBridge = {
    async open(
        size: TerminalSize,
        cwd: string | null,
        onFrame: (frame: PtyFrame) => void,
    ): Promise<TabId> {
        const channel = new Channel<PtyFrame>();
        channel.onmessage = onFrame;
        return invoke<TabId>("pty_open", { channel, cols: size.cols, rows: size.rows, cwd });
    },

    write: (tabId, data) => invoke("pty_write", { tabId, data }),
    resize: (tabId, size) => invoke("pty_resize", { tabId, cols: size.cols, rows: size.rows }),
    ack: (tabId) => invoke("pty_ack", { tabId }),
    close: (tabId) => invoke("pty_close", { tabId }),
    tabs: () => invoke<TabInfo[]>("pty_tabs"),
    hasForegroundProcess: (tabId) => invoke<boolean>("pty_has_foreground_process", { tabId }),
    onTabsChanged: (handler) =>
        listen<TabInfo[]>(TAB_CHANGED_EVENT, (event) => {
            handler(event.payload);
        }),
};
