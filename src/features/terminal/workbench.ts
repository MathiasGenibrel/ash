import { isShell } from "./ports";
import type {
    PtyBridge,
    ShellTab,
    Tab,
    TabId,
    TerminalViewFactory,
    ToolSurface,
    ToolSurfaceFactory,
} from "./ports";
import { TerminalSession } from "./session";
import {
    activeTab,
    adopt,
    cycle,
    noTabs,
    select,
    selectAt,
    withUpdates,
    type Step,
    type TabsState,
} from "./tabs";

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
    /**
     * Fabrique la surface d'un onglet qui n'est pas un shell — l'onglet de merge (#30).
     *
     * Injectée depuis le composition root : l'atelier ne connaît pas `features/merge`, et
     * `features/merge` ne connaît pas les onglets. Absente, un onglet de merge annoncé par
     * le backend reste dans la liste, sans surface — ce qui est ce qu'on veut d'une webview
     * en retard d'un genre sur son backend.
     */
    createSurface?: ToolSurfaceFactory;
    confirmClose: (tab: ShellTab) => Promise<boolean>;
    /** Appelé après chaque changement : la ligne de statut et la sidebar écoutent. */
    onRender: (state: TabsState) => void;
}

/**
 * D'où part un nouvel onglet.
 *
 * Les deux premières formes sont les deux raccourcis de la spec §4.4 — `⌘T` reprend le
 * répertoire courant de l'onglet actif, `⌘⇧T` part de `~`. La troisième est le clic sur une
 * ligne de worktree **épinglée sans onglet** (spec §5.2) : il n'y a alors aucun onglet dont
 * reprendre un répertoire, et c'est la ligne elle-même qui dit lequel.
 */
export type Origin = "current-worktree" | "home" | { readonly directory: string };

/**
 * Les onglets shell et leur unique terminal visible.
 *
 * L'atelier ne tient **pas** la liste des onglets : il la relit au backend après chaque
 * ouverture et chaque fermeture ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 * Ce qu'il tient, c'est ce que le backend n'a pas — les surfaces de rendu, et la
 * sélection.
 *
 * Toutes les actions passent par une file : un `Cmd+T` maintenu enfoncé, ou une
 * fermeture pendant une ouverture, entrelaceraient sinon deux relectures d'ordre et la
 * sélection sauterait.
 */
export class TerminalWorkbench {
    private state: TabsState = noTabs;
    private readonly panes = new Map<TabId, TerminalSession>();
    /**
     * Les surfaces d'outil, dans la même pile que les terminaux et sous la même règle : une
     * seule visible à la fois ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)).
     *
     * Elles ne portent **aucun PTY** : rien ici ne leur écrit, ne les redimensionne, ni ne
     * les acquitte. C'est ce que la séparation des deux registres côté Rust rend
     * structurel — un identifiant d'onglet de merge n'entre jamais dans `panes`.
     */
    private readonly surfaces = new Map<TabId, ToolSurface>();
    private queue: Promise<void> = Promise.resolve();

    constructor(private readonly ports: WorkbenchPorts) {
        // L'abonnement est pris ici, et pas par l'appelant : un atelier qu'on aurait pu
        // oublier de brancher est exactement ce qui a laissé les titres d'onglet figés.
        // Il n'y a rien à désabonner — l'atelier vit aussi longtemps que la fenêtre.
        void this.ports.bridge
            .onTabsChanged((changed) => {
                this.applyChanges(changed);
            })
            .catch(() => {
                // Pas d'abonnement possible : les titres ne suivront pas les `cd`. C'est
                // une dégradation visible, pas une raison d'empêcher la fenêtre d'ouvrir.
            });
    }

    /** Ouvre un onglet et le sélectionne. `Cmd+T` / `Cmd+Shift+T`, et le `+` de la sidebar. */
    openTab(origin: Origin): Promise<void> {
        return this.serialize(async () => {
            // `Cmd+T` part du répertoire **courant** de l'onglet actif, pas de celui
            // qu'il avait à sa dernière ouverture d'onglet : le `cwd` bouge à chaque `cd`
            // et vit dans le backend, donc on le lui redemande maintenant
            // ([ADR-0005](../../../docs/adr/0005-sonde-cwd-libproc.md)).
            let from: Tab | null = null;
            if (origin === "current-worktree") {
                await this.reload();
                from = activeTab(this.state);
            }
            // Depuis un onglet de merge, « le worktree courant » est celui dont il résout le
            // conflit : il n'a pas de `cwd` — aucun processus n'y tourne —, mais il sait
            // parfaitement où il est.
            const inherited = from === null ? null : isShell(from) ? from.cwd : from.worktreeRoot;
            const cwd = typeof origin === "object" ? origin.directory : inherited;

            // Le shell peut sortir avant même que `start` ait rendu la main — un `cwd`
            // qui n'existe plus, un `~/.zshrc` qui appelle `exit`. La session est donc
            // atteinte par un intermédiaire : la capturer directement laisserait `onExit`
            // lire une variable pas encore affectée.
            const pane: { session?: TerminalSession } = {};
            pane.session = await TerminalSession.start(this.ports.createView(), this.ports.bridge, {
                cwd,
                onExit: () => {
                    const gone = pane.session;
                    if (gone !== undefined) void this.forget(gone.tabId);
                },
            });

            const session = pane.session;
            if (!session.isClosed) this.panes.set(session.tabId, session);

            await this.reload();
            this.state = select(this.state, session.tabId);
            this.render();
        });
    }

    /** `Cmd+1` … `Cmd+9`, à partir de 1. Sur une position vide, ne fait rien. */
    selectAt(position: number): Promise<void> {
        return this.serialize(() => {
            this.state = selectAt(this.state, position);
            this.render();
            return Promise.resolve();
        });
    }

    /** `Ctrl+Tab` et `Ctrl+Shift+Tab` : l'onglet voisin, en bouclant. */
    cycle(step: Step): Promise<void> {
        return this.serialize(() => {
            this.state = cycle(this.state, step);
            this.render();
            return Promise.resolve();
        });
    }

    /** Le clic sur une ligne d'onglet de la sidebar. */
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
     * Ferme un onglet — `Cmd+W`, ou le menu Terminal.
     *
     * Si quelque chose tourne dedans, **rien n'est détruit tant que l'utilisateur n'a
     * pas répondu**, et un refus laisse l'onglet exactement comme il était.
     */
    closeTab(tabId: TabId): Promise<void> {
        return this.serialize(async () => {
            const tab = this.state.tabs.find((candidate) => candidate.tabId === tabId);
            if (tab === undefined) return;

            // Une surface d'outil se ferme **sans question** : il n'y a rien dedans qui
            // puisse être perdu. Pour l'onglet de merge, c'est un critère du ticket —
            // l'état vit dans l'index git, pas dans Ash (spec §7.4).
            if (!isShell(tab)) {
                const surface = this.surfaces.get(tabId);
                if (surface === undefined) return;
                await surface.close();
                this.surfaces.delete(tabId);
                surface.element.remove();
                await this.reload();
                this.render();
                return;
            }

            const session = this.panes.get(tabId);
            if (session === undefined) return;

            if (await this.ports.bridge.hasForegroundProcess(tabId)) {
                if (!(await this.ports.confirmClose(tab))) return;
            }

            await session.close();
            this.panes.delete(tabId);
            await this.reload();
            this.render();
        });
    }

    /**
     * Relit la liste d'onglets au backend, et redessine.
     *
     * Le chemin par lequel un onglet **ouvert ailleurs** apparaît : l'onglet de merge (#30)
     * naît d'un geste dans le panneau des conflits, pas d'un `⌘T`. L'atelier ne tient pas la
     * liste — il la relit ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) —,
     * donc « quelque chose a changé côté backend » n'a qu'une réponse : redemander.
     */
    refresh(): Promise<void> {
        return this.serialize(async () => {
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
    private applyChanges(changed: readonly ShellTab[]): void {
        const updated = withUpdates(this.state, changed);
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
        this.adoptSurfaces();
        // Un seul terminal visible ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)).
        // Les autres sont masqués, pas démontés : leur shell tourne, leur sortie arrive,
        // et leur acquittement continue — sans quoi ils se figeraient au bout de huit
        // morceaux (voir `session.ts` et `docs/spike-xterm.md`).
        for (const [tabId, session] of this.panes) {
            session.setVisible(tabId === this.state.activeTabId);
        }
        for (const [tabId, surface] of this.surfaces) {
            surface.setVisible(tabId === this.state.activeTabId);
        }
        this.ports.onRender(this.state);
    }

    /**
     * Donne une surface aux onglets qui n'en ont pas encore, et retire celles des onglets
     * partis.
     *
     * C'est ici que le second genre d'onglet devient visible, et nulle part ailleurs : la
     * liste vient du backend, et l'atelier n'ouvre jamais une surface de sa propre
     * initiative ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    private adoptSurfaces(): void {
        const factory = this.ports.createSurface;
        for (const tab of this.state.tabs) {
            if (isShell(tab) || this.surfaces.has(tab.tabId)) continue;
            const surface = factory?.(tab) ?? null;
            if (surface !== null) this.surfaces.set(tab.tabId, surface);
        }

        for (const [tabId, surface] of this.surfaces) {
            if (this.state.tabs.some((tab) => tab.tabId === tabId)) continue;
            // L'onglet a disparu de la liste du backend — fermé ailleurs. La surface part
            // avec lui, sans qu'on redemande au backend d'oublier ce qu'il a déjà oublié.
            this.surfaces.delete(tabId);
            surface.element.remove();
        }
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
