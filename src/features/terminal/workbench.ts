import type { PtyBridge, TabChange, TabId, TabInfo, TerminalViewFactory } from "./ports";
import { TerminalSession } from "./session";
import { activeTab, adopt, noTabs, select, selectAt, withCwd, type TabsState } from "./tabs";

/**
 * Ce dont l'atelier a besoin, et rien de plus.
 *
 * `confirmClose` est un port, pas un `window.confirm` : la règle « `Cmd+W` demande
 * confirmation si un processus tourne » (spec §4.4) est ce que cette classe a de plus
 * important à protéger, et elle ne serait pas vérifiable derrière une boîte de dialogue.
 */
export interface WorkbenchPorts {
    bridge: PtyBridge;
    createView: TerminalViewFactory;
    confirmClose: (tab: TabInfo) => Promise<boolean>;
    /** Appelé après chaque changement : c'est la barre d'onglets qui écoute. */
    onRender: (state: TabsState) => void;
}

/** D'où part un nouvel onglet. */
export type Origin = "current-worktree" | "home";

/**
 * Les onglets shell et leur unique terminal visible.
 *
 * L'atelier ne tient **pas** la liste des onglets : il la relit au backend après chaque
 * ouverture et chaque fermeture ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 * Ce qu'il tient, c'est ce que le backend n'a pas — les surfaces de rendu, et la
 * sélection.
 *
 * Toutes les actions passent par une file : un `Cmd+N` maintenu enfoncé, ou une
 * fermeture pendant une ouverture, entrelaceraient sinon deux relectures d'ordre et la
 * sélection sauterait.
 */
export class TerminalWorkbench {
    private state: TabsState = noTabs;
    private readonly panes = new Map<TabId, TerminalSession>();
    private queue: Promise<void> = Promise.resolve();

    constructor(private readonly ports: WorkbenchPorts) {
        // L'abonnement est pris ici, et pas par l'appelant : un atelier qu'on aurait pu
        // oublier de brancher est exactement ce qui a laissé les titres d'onglet figés.
        // Il n'y a rien à désabonner — l'atelier vit aussi longtemps que la fenêtre.
        void this.ports.bridge
            .onTabsChanged((changes) => {
                this.applyChanges(changes);
            })
            .catch(() => {
                // Pas d'abonnement possible : les titres ne suivront pas les `cd`. C'est
                // une dégradation visible, pas une raison d'empêcher la fenêtre d'ouvrir.
            });
    }

    /** Ouvre un onglet et le sélectionne. `Cmd+N` / `Cmd+Shift+N`, et le bouton `+`. */
    openTab(origin: Origin): Promise<void> {
        return this.serialize(async () => {
            // `Cmd+N` part du répertoire **courant** de l'onglet actif, pas de celui
            // qu'il avait à sa dernière ouverture d'onglet : le `cwd` bouge à chaque `cd`
            // et vit dans le backend, donc on le lui redemande maintenant
            // ([ADR-0005](../../../docs/adr/0005-sonde-cwd-libproc.md)).
            let from: TabInfo | null = null;
            if (origin === "current-worktree") {
                await this.reload();
                from = activeTab(this.state);
            }

            // Le shell peut sortir avant même que `start` ait rendu la main — un `cwd`
            // qui n'existe plus, un `~/.zshrc` qui appelle `exit`. La session est donc
            // atteinte par un intermédiaire : la capturer directement laisserait `onExit`
            // lire une variable pas encore affectée.
            const pane: { session?: TerminalSession } = {};
            pane.session = await TerminalSession.start(
                this.ports.createView(),
                this.ports.bridge,
                {
                    cwd: from?.cwd ?? null,
                    onExit: () => {
                        const gone = pane.session;
                        if (gone !== undefined) void this.forget(gone.tabId);
                    },
                },
            );

            const session = pane.session;
            if (!session.isClosed) this.panes.set(session.tabId, session);

            await this.reload();
            this.state = select(this.state, session.tabId);
            this.render();
        });
    }

    /** `Cmd+1` … `Cmd+9`, à partir de 1. Hors de la barre, ne fait rien. */
    selectAt(position: number): Promise<void> {
        return this.serialize(() => {
            this.state = selectAt(this.state, position);
            this.render();
            return Promise.resolve();
        });
    }

    /** Le clic sur un onglet de la barre. */
    select(tabId: TabId): Promise<void> {
        return this.serialize(() => {
            this.state = select(this.state, tabId);
            this.render();
            return Promise.resolve();
        });
    }

    /** `Cmd+W` sur l'onglet actif. */
    closeActive(): Promise<void> {
        const active = activeTab(this.state);
        return active === null ? Promise.resolve() : this.closeTab(active.tabId);
    }

    /**
     * Ferme un onglet — la croix de la barre, ou `Cmd+W`.
     *
     * Si quelque chose tourne dedans, **rien n'est détruit tant que l'utilisateur n'a
     * pas répondu**, et un refus laisse l'onglet exactement comme il était.
     */
    closeTab(tabId: TabId): Promise<void> {
        return this.serialize(async () => {
            const tab = this.state.tabs.find((candidate) => candidate.tabId === tabId);
            const session = this.panes.get(tabId);
            if (tab === undefined || session === undefined) return;

            if (await this.ports.bridge.hasForegroundProcess(tabId)) {
                if (!(await this.ports.confirmClose(tab))) return;
            }

            await session.close();
            this.panes.delete(tabId);
            await this.reload();
            this.render();
        });
    }

    /** `Cmd+K` : efface le scrollback de l'onglet courant, et de lui seul. */
    clearActive(): Promise<void> {
        return this.serialize(() => {
            const active = this.state.activeTabId;
            if (active !== null) this.panes.get(active)?.clear();
            return Promise.resolve();
        });
    }

    /** L'état affiché. Le backend reste la source de l'ordre ; ceci en est le reflet. */
    get tabs(): TabsState {
        return this.state;
    }

    /**
     * Attend que la file soit vide.
     *
     * Toutes les actions ne partent pas d'un appel : un shell qui sort de lui-même en
     * déclenche une, et personne n'a de promesse à attendre pour celle-là.
     */
    settled(): Promise<void> {
        return this.queue;
    }

    /**
     * La boucle de sonde du backend annonce des répertoires qui ont bougé.
     *
     * Volontairement **hors** de la file d'actions : une confirmation de fermeture peut
     * la retenir aussi longtemps que l'utilisateur hésite, et les titres d'onglet
     * n'auraient aucune raison de se figer pendant ce temps. Rien n'est perdu pour
     * autant — c'est le backend qui détient le `cwd`, et la relecture suivante rendra la
     * même valeur.
     */
    private applyChanges(changes: readonly TabChange[]): void {
        const updated = withCwd(this.state, changes);
        if (updated === this.state) return;
        this.state = updated;
        this.render();
    }

    /** Le shell d'un onglet est sorti tout seul : sa surface part avec lui. */
    private forget(tabId: TabId): Promise<void> {
        return this.serialize(async () => {
            if (!this.panes.delete(tabId)) return;
            await this.reload();
            this.render();
        });
    }

    private async reload(): Promise<void> {
        this.state = adopt(this.state, await this.ports.bridge.tabs());
    }

    private render(): void {
        // Un seul terminal visible ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)).
        // Les autres sont masqués, pas démontés : leur shell tourne, leur sortie arrive,
        // et leur acquittement continue — sans quoi ils se figeraient au bout de huit
        // morceaux (voir `session.ts` et `docs/spike-xterm.md`).
        for (const [tabId, session] of this.panes) {
            session.setVisible(tabId === this.state.activeTabId);
        }
        this.ports.onRender(this.state);
    }

    /**
     * Enchaîne les actions. Le rejet d'une action ne doit pas condamner la file : la
     * suivante repart d'une promesse tenue.
     */
    private serialize(task: () => Promise<void>): Promise<void> {
        const done = this.queue.then(task);
        this.queue = done.catch(() => undefined);
        return done;
    }
}
