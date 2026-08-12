import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { applyKeyAction, resolveKeyAction } from "./key-actions";
import { resolveKeyBinding } from "./key-bindings";
import type { TerminalSize, TerminalView, ThemeSignal, Unsubscribe } from "./ports";
import { readTerminalTheme } from "./theme";

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
    private readonly unfollowTheme: Unsubscribe;
    /**
     * Les abonnés à la saisie, appelés par xterm.js **et** par la table de raccourcis.
     *
     * La liste est tenue ici plutôt que déléguée à `term.onData` parce qu'il y a désormais
     * deux sources de saisie pour un même onglet, et qu'une seule est celle de xterm. Elle
     * est donc aussi vidée dans `dispose` : c'est la seule ressource de cette vue que
     * `term.dispose()` ne libère pas à notre place.
     */
    private readonly inputs: ((data: string) => void)[] = [];

    /**
     * Crée sa propre surface dans `parent`, et la retire en se libérant.
     *
     * C'est la vue qui possède son élément, et pas l'appelant : plusieurs onglets
     * partagent le même parent, et il faut bien que chacun sache retirer le sien.
     */
    constructor(parent: HTMLElement, theme: ThemeSignal) {
        this.pane = document.createElement("div");
        this.pane.className = "terminal-pane";
        parent.append(this.pane);

        this.term = new Terminal({
            fontFamily: '"JetBrains Mono", ui-monospace, monospace',
            fontSize: 13,
            // La palette du document, résolue maintenant : xterm.js ne comprend pas les
            // `var(--ash-…)`. Un onglet ouvert après une bascule naît donc déjà à la
            // bonne palette, sans attendre le prochain changement de thème.
            theme: readTerminalTheme(),
            scrollback: 10_000,
            // `scrollOnUserInput` n'est pas réglé ici, et ce n'est pas un oubli : il vaut
            // `true` par défaut, donc une frappe qu'xterm.js traite lui-même ramène déjà
            // l'affichage en bas après une remontée dans le scrollback. L'écrire ne
            // changerait rien. Ce qu'il ne couvre pas — les octets qu'Ash écrit pour un
            // raccourci, qui ne passent pas par son chemin clavier — est rattrapé dans
            // `emitInput`.
            allowProposedApi: true,
            // ⌥ **compose**, il n'est pas Meta. À `true`, xterm.js transformait toute
            // frappe avec ⌥ en `ESC`+touche avant que macOS n'ait composé quoi que ce
            // soit : sur un clavier AZERTY, `|` (⌥⇧L) était intapable, comme `~`, `\`,
            // `{`, `}`, `[`, `]` et `€`. À `false`, ces frappes passent par le chemin
            // « third level shift » de xterm.js, qui ne les annule pas et laisse la
            // webview livrer le caractère composé.
            //
            // Ce que ⌥ était censé apporter en échange — la navigation par mot — n'était
            // relié nulle part : c'est maintenant la table de `key-bindings.ts`, posée
            // ci-dessous, qui l'assure explicitement.
            //
            // À relire en montant xterm.js de version : ce chemin est **interne** à
            // xterm.js (`_keyDown` consulte `_isThirdLevelShift`, vérifié sur 6.0.0) et
            // aucun test ne le couvre — `bun test` n'a ni WKWebView ni clavier, et les
            // tests de `key-bindings.test.ts` protègent la table, pas le comportement de
            // xterm.js. La vérification est manuelle et tient en une frappe : sur un
            // clavier AZERTY, ⌥⇧L doit écrire `|` dans un onglet.
            macOptionIsMeta: false,
        });

        // Le gestionnaire est branché avant `open` : xterm.js le consulte pour chaque
        // `keydown` et `keyup`, et `false` veut dire « xterm ne traite pas cet
        // événement ». Il ne rend `false` que pour les accords qu'il a **effectivement**
        // envoyés ; tout le reste — un caractère composé, un `Ctrl-A` tapé directement,
        // un accélérateur que macOS n'aurait pas consommé — repart intact.
        this.term.onData((data) => {
            this.emitInput(data);
        });

        // Saisie d'abord, action ensuite. L'ordre est celui du coût d'une erreur : la
        // saisie est le chemin nominal du terminal, et les deux tables sont disjointes —
        // un test de `key-actions.test.ts` le vérifie plutôt que de le supposer.
        this.term.attachCustomKeyEventHandler((event) => {
            const bytes = resolveKeyBinding(event);
            if (bytes !== null) {
                // WKWebView garde des défauts à lui pour ces accords — `Cmd+←` y est
                // encore « page précédente ». Rendre `false` arrête xterm.js, pas le
                // navigateur.
                event.preventDefault();
                this.emitInput(bytes);
                return false;
            }

            const action = resolveKeyAction(event);
            if (action !== null) {
                // Rien ne part vers le PTY : `applyKeyAction` ne touche qu'à la fenêtre
                // d'affichage du terminal, qui appartient à cet onglet et à lui seul
                // (ADR-0003). `⌘↑` est « début de document » dans WKWebView, d'où le
                // `preventDefault` — sans lui, la page entière se déplacerait.
                //
                // `Terminal` est passé comme `ScrollSurface` : il satisfait ses deux
                // méthodes **structurellement**, sans adaptateur, et c'est ce qui met la
                // convention de signe sous test sans DOM ni WebGL. Le prix est un couplage
                // qu'aucun type ne réaffirme, à relire en montant xterm.js de version, au
                // même titre que `macOptionIsMeta` ci-dessus : `scrollPages(pageCount)` et
                // `scrollLines(amount)` sont publiques et documentées « negative scrolls
                // up » sur 6.0.0, comme `scrollToBottom()` qu'`emitInput` appelle. Un
                // renommage ferait échouer la compilation ; une **inversion du signe**,
                // non — c'est le seul cas à vérifier à la main, et il tient en une frappe :
                // dans un onglet qui a du scrollback, `⌘↑` doit remonter.
                event.preventDefault();
                applyKeyAction(action, this.term);
                return false;
            }

            return true;
        });

        // Repeindre à chaud, et non recréer : `options.theme` remplace les couleurs et
        // redessine, sans toucher au tampon — le contenu et le scrollback de l'onglet
        // survivent à la bascule. Un onglet qui reconstruirait son terminal perdrait les
        // deux, et son PTY continuerait d'écrire dans une surface morte.
        //
        // L'abonnement est repris à chaque vue plutôt que porté par l'atelier : c'est ici
        // qu'est la ressource, donc ici qu'est sa libération (voir `dispose`).
        this.unfollowTheme = theme.subscribe(() => {
            this.term.options.theme = readTerminalTheme();
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
        this.inputs.push(handler);
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
        this.unfollowTheme();
        this.observer.disconnect();
        this.term.dispose();
        this.pane.remove();
        // Le gestionnaire de touches et `term.onData` passent tous deux par `emitInput` :
        // laisser la liste garnie après la fermeture retiendrait la session, son `tabId` et
        // son pont pour un onglet dont le PTY n'existe plus.
        this.inputs.length = 0;
    }

    /**
     * Pousse une saisie vers les abonnés, qu'elle vienne de xterm.js ou d'un raccourci.
     *
     * Et **ramène l'affichage en bas**, parce que taper est ce qui l'y ramène dans un
     * terminal. xterm.js le fait déjà pour ce qu'il traite lui-même (`scrollOnUserInput`
     * vaut `true`), mais les raccourcis d'édition de ligne de `key-bindings.ts` — `⌥←`,
     * `⌘⌫`… — n'empruntent pas son chemin clavier : ils rendent `false` et écrivent ici.
     * Sans ce rappel, l'utilisateur qui remonte au clavier avec `⌘↑` puis corrige sa
     * commande avec `⌥←` ne voit pas ce qu'il édite. Le défaut existait depuis #75 ; il
     * n'était pas atteignable avant que #78 ne donne le moyen de remonter.
     *
     * C'est ici et nulle part ailleurs : c'est le seul chemin par lequel des octets
     * partent vers le PTY, donc toute source de saisie future — la recherche de #79, une
     * composition d'ADR-0015 — hérite du même retour sans y penser. Le geste de
     * défilement, lui, ne passe pas par ici : `applyKeyAction` n'envoie rien, et
     * l'affichage reste où l'utilisateur l'a mis.
     */
    private emitInput(data: string): void {
        this.term.scrollToBottom();
        for (const handler of this.inputs) handler(data);
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
