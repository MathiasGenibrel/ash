/**
 * API publique de la feature terminal.
 *
 * Le reste du frontend n'importe que ce fichier : ni `xterm-view`, ni `pty-bridge`, ni
 * `session` ne sont des points d'entrée.
 */

import { tauriPty } from "./pty-bridge";
import { TerminalSession } from "./session";
import { XtermView } from "./xterm-view";

export type { PtyFrame, TabId, TerminalSize } from "./ports";
export { TerminalSession } from "./session";

/** Ouvre un onglet shell dans `host`. Un onglet, un PTY — pas plus (ADR-0003). */
export function openTerminal(host: HTMLElement): Promise<TerminalSession> {
    return TerminalSession.start(new XtermView(host), tauriPty);
}
