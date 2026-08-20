import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { ComposeOutcome } from "@/shared/ipc";
import type { PtyBridge, PtyFrame, ShellTab, Tab, TabId, TabInfo, TerminalSize } from "./ports";

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
    compose: (tabId, text) => invoke<ComposeOutcome>("pty_compose", { tabId, text }),
    resize: (tabId, size) => invoke("pty_resize", { tabId, cols: size.cols, rows: size.rows }),
    ack: (tabId) => invoke("pty_ack", { tabId }),
    close: (tabId) => invoke("pty_close", { tabId }),
    // `tabs` et non `pty_tabs` : la liste porte **les deux genres** d'onglet depuis #30, et
    // c'est le composition root Rust qui les réunit — la feature `pty` ne connaît pas les
    // surfaces de merge, et `merge` ne connaît pas les PTY.
    tabs: () => invoke<Tab[]>("tabs"),
    hasForegroundProcess: (tabId) => invoke<boolean>("pty_has_foreground_process", { tabId }),
    onTabsChanged: (handler) =>
        listen<TabInfo[]>(TAB_CHANGED_EVENT, (event) => {
            // L'event vient de la boucle de sonde du registre de PTY : **tout ce qui en
            // sort est un shell**, par construction. L'étiquette est posée ici plutôt que
            // devinée plus loin — sans elle, la seule façon de reconnaître le genre d'un
            // onglet serait de tester la présence d'un champ.
            handler(event.payload.map((tab): ShellTab => ({ ...tab, kind: "shell" })));
        }),
};
