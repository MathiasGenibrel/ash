import { Channel, invoke } from "@tauri-apps/api/core";

import type { PtyBridge, PtyFrame, TabId, TerminalSize } from "./ports";

/**
 * L'implémentation réelle du pont : les cinq commandes déclarées par
 * `src-tauri/src/features/pty/commands.rs`, et rien d'autre.
 *
 * Le frontend ne connaît que ces noms et le type `PtyFrame` — jamais la structure
 * interne du backend.
 */
export const tauriPty: PtyBridge = {
    async open(size: TerminalSize, onFrame: (frame: PtyFrame) => void): Promise<TabId> {
        const channel = new Channel<PtyFrame>();
        channel.onmessage = onFrame;
        return invoke<TabId>("pty_open", { channel, cols: size.cols, rows: size.rows });
    },

    write: (tabId, data) => invoke("pty_write", { tabId, data }),
    resize: (tabId, size) => invoke("pty_resize", { tabId, cols: size.cols, rows: size.rows }),
    ack: (tabId) => invoke("pty_ack", { tabId }),
    close: (tabId) => invoke("pty_close", { tabId }),
};
