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
    private readonly pane: HTMLElement;

    /**
     * Crée sa propre surface dans `parent`, et la retire en se libérant.
     *
     * C'est la vue qui possède son élément, et pas l'appelant : plusieurs onglets
     * partagent le même parent, et il faut bien que chacun sache retirer le sien.
     */
    constructor(parent: HTMLElement) {
        this.pane = document.createElement("div");
        this.pane.className = "terminal-pane";
        parent.append(this.pane);

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

        this.term.open(this.pane);
        this.term.loadAddon(this.fit);
        this.refit();
        this.loadWebgl();

        // Le terminal suit la fenêtre. `ResizeObserver` plutôt que l'event `resize` :
        // la sidebar et le panneau bas changeront la largeur sans que la fenêtre bouge.
        this.observer = new ResizeObserver(() => {
            this.refit();
        });
        this.observer.observe(this.pane);
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

    clear(): void {
        this.term.clear();
    }

    setVisible(visible: boolean): void {
        this.pane.classList.toggle("is-active", visible);
    }

    focus(): void {
        this.term.focus();
    }

    dispose(): void {
        this.observer.disconnect();
        this.term.dispose();
        this.pane.remove();
    }

    /**
     * Recalcule la grille — sauf quand la surface n'a pas de taille.
     *
     * C'est le piège des onglets masqués : le `FitAddon` divise la place disponible par
     * la taille d'un caractère et borne le résultat à son minimum, donc une surface de
     * hauteur nulle lui fait proposer une grille de deux colonnes. Le PTY reçoit alors un
     * `SIGWINCH` à 2×1, la TUI qui y tourne se redessine à cette taille, et le retour sur
     * l'onglet montre un affichage détruit.
     *
     * Les onglets inactifs sont pour cette raison masqués par `visibility`, et non par
     * `display` : ils gardent leur taille. Ce garde-fou couvre le reste — fenêtre
     * réduite, panneau replié à zéro, onglet retiré du DOM.
     */
    private refit(): void {
        const { width, height } = this.pane.getBoundingClientRect();
        if (width < 1 || height < 1) return;
        this.fit.fit();
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
