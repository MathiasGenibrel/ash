import { Channel, invoke } from "@tauri-apps/api/core";

import type { PtyBridge, PtyFrame, TabId, TabInfo, TerminalSize } from "./ports";

/**
 * L'implémentation réelle du pont : les sept commandes déclarées par
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
};
