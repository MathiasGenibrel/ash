import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import type { TerminalSize, TerminalView } from "./ports";

/**
 * xterm.js, adapté au port `TerminalView`.
 *
 * Deux choix viennent du spike (`docs/spike-xterm.md`) et ne sont pas des préférences :
 * l'addon **WebGL**, qui donne environ 50 % de débit en plus sous WKWebView, et son
 * repli explicite sur perte de contexte — sans écoute, une perte se lirait comme un
 * écran figé.
 */
export class XtermView implements TerminalView {
    private readonly term: Terminal;
    private readonly fit = new FitAddon();
    private readonly observer: ResizeObserver;

    constructor(host: HTMLElement) {
        this.term = new Terminal({
            fontFamily: '"JetBrains Mono", ui-monospace, monospace',
            fontSize: 13,
            // Les couleurs complètes et le thème clair/sombre appartiennent à la tâche
            // « ligne de statut et thème ». Ici, de quoi lire du texte.
            theme: { background: "#16181d", foreground: "#d4d7dd" },
            scrollback: 10_000,
            allowProposedApi: true,
            macOptionIsMeta: true,
        });

        this.term.open(host);
        this.term.loadAddon(this.fit);
        this.fit.fit();
        this.loadWebgl();

        // Le terminal suit la fenêtre. `ResizeObserver` plutôt que l'event `resize` :
        // la sidebar et le panneau bas changeront la largeur sans que la fenêtre bouge.
        this.observer = new ResizeObserver(() => {
            this.fit.fit();
        });
        this.observer.observe(host);

        this.term.focus();
    }

    get size(): TerminalSize {
        return { cols: this.term.cols, rows: this.term.rows };
    }

    write(data: string, done: () => void): void {
        this.term.write(data, done);
    }

    onInput(handler: (data: string) => void): void {
        this.term.onData(handler);
    }

    onResize(handler: (size: TerminalSize) => void): void {
        this.term.onResize(({ cols, rows }) => {
            handler({ cols, rows });
        });
    }

    dispose(): void {
        this.observer.disconnect();
        this.term.dispose();
    }

    private loadWebgl(): void {
        try {
            const webgl = new WebglAddon();
            // WKWebView peut perdre son contexte WebGL sous pression mémoire. xterm.js
            // retombe alors sur le rendu DOM, à condition qu'on ait libéré l'addon.
            webgl.onContextLoss(() => {
                webgl.dispose();
            });
            this.term.loadAddon(webgl);
        } catch {
            // Pas de WebGL : le rendu DOM tient (24 Mo/s mesurés), c'est une dégradation,
            // pas une panne.
        }
    }
}
