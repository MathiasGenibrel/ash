import type { PtyBridge, PtyFrame, TabId, TerminalView } from "./ports";

/**
 * Relie un moteur de terminal à un PTY du backend.
 *
 * Toute la subtilité tient dans l'acquittement. Le backend n'envoie qu'un nombre limité
 * de morceaux sans réponse ; c'est ce qui empêche xterm.js de dépasser son tampon
 * d'écriture et de **jeter de la sortie** (voir `docs/spike-xterm.md`). Un acquittement
 * manquant fige l'onglet au bout de quelques morceaux ; un acquittement en trop rouvre
 * la vanne et ramène la perte de données.
 */
export interface SessionOptions {
    /** Répertoire de lancement du shell. `null` — le défaut — vaut `~`. */
    cwd?: string | null;
    /** Appelé quand le shell sort de lui-même : l'onglet n'a plus lieu d'être. */
    onExit?: () => void;
}

export class TerminalSession {
    private closed = false;

    private constructor(
        readonly tabId: TabId,
        private readonly view: TerminalView,
        private readonly bridge: PtyBridge,
        private readonly onExit: (() => void) | undefined,
    ) {}

    static async start(
        view: TerminalView,
        bridge: PtyBridge,
        options: SessionOptions = {},
    ): Promise<TerminalSession> {
        // Le shell écrit dès qu'il démarre, et `open` n'a pas encore rendu l'identifiant
        // d'onglet : les premiers morceaux arrivent avant que la session existe. Les
        // mettre de côté est la seule façon de ne pas perdre l'invite de commande.
        const early: PtyFrame[] = [];
        let receive = (frame: PtyFrame): void => {
            early.push(frame);
        };

        const tabId = await bridge.open(view.size, options.cwd ?? null, (frame) => {
            receive(frame);
        });

        const session = new TerminalSession(tabId, view, bridge, options.onExit);
        receive = (frame) => {
            session.onFrame(frame);
        };
        for (const frame of early) session.onFrame(frame);

        view.onInput((data) => {
            if (session.closed) return;
            void bridge.write(tabId, data);
        });

        view.onResize((size) => {
            if (session.closed) return;
            void bridge.resize(tabId, size);
        });

        return session;
    }

    /** Ferme l'onglet et termine son shell. Idempotent. */
    async close(): Promise<void> {
        if (this.closed) return;
        this.closed = true;
        this.view.dispose();
        await this.bridge.close(this.tabId);
    }

    /** Vrai quand le shell est sorti, ou que l'onglet a été fermé. */
    get isClosed(): boolean {
        return this.closed;
    }

    /** Efface le scrollback de l'onglet — `Cmd+K`. */
    clear(): void {
        if (this.closed) return;
        this.view.clear();
    }

    /** Montre ou masque l'onglet. Le shell continue de tourner dans les deux cas. */
    setVisible(visible: boolean): void {
        if (this.closed) return;
        this.view.setVisible(visible);
        if (visible) this.view.focus();
    }

    private onFrame(frame: PtyFrame): void {
        if (frame.kind === "exit") {
            this.closed = true;
            // Le shell est sorti sans qu'on le lui demande : le backend a déjà retiré
            // l'onglet de son registre, il reste à retirer sa surface.
            this.view.dispose();
            this.onExit?.();
            return;
        }

        if (this.closed) return;

        this.view.write(frame.data, () => {
            // Le rappel peut arriver après la fermeture : xterm.js rend en différé, et
            // l'utilisateur a pu fermer l'onglet entre-temps. Acquitter ici viserait un
            // onglet que le backend ne connaît plus.
            if (this.closed) return;
            void this.bridge.ack(this.tabId);
        });
    }
}
