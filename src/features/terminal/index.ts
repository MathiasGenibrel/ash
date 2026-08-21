/**
 * API publique de la feature terminal.
 *
 * Le reste du frontend n'importe que ce fichier : ni `xterm-view`, ni `pty-bridge`, ni
 * `workbench` ne sont des points d'entrée.
 */

import "./terminal.css";

import type { AccountUsage, WorktreeMetadata } from "@/shared/ipc";
import type {
    FontFamilySignal,
    FontSizeSignal,
    Tab,
    TabId,
    ThemeSignal,
    ToolSurfaceFactory,
} from "./ports";
import { askToClose } from "./confirm-dialog";
import { tauriGit } from "./git-bridge";
import { tauriLinks } from "./link-bridge";
import { WorktreeMetadataStore } from "./metadata-store";
import { tauriPty } from "./pty-bridge";
import { StatusLine, composeStatusLine } from "./status-line";
import { tauriStatusBar } from "./status-bar-bridge";
import { composeQuotas } from "./usage";
import { tauriUsage } from "./usage-bridge";
import type { StatusBarSegments } from "./status-bar";
import { activeTab, noTabs, type Step, type TabsState } from "./tabs";
import { XtermView } from "./xterm-view";
import { TerminalWorkbench, type Origin } from "./workbench";

export type {
    FontFamilySignal,
    FontSizeSignal,
    MergeTab,
    PtyFrame,
    ShellTab,
    Tab,
    TabId,
    TabInfo,
    TerminalSize,
    ThemeSignal,
    ToolSurface,
    ToolSurfaceFactory,
} from "./ports";
export type { Origin } from "./workbench";
export type { Step } from "./tabs";
/**
 * Passer un rebase arrêté à l'agent de l'onglet (spec §7.4,
 * [ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
 *
 * Publié parce que le geste part d'ailleurs — la vue `conflicts` du panneau bas (#24) —
 * alors que ce qu'il touche, l'onglet et son PTY, appartient à cette feature-ci. Le
 * panneau appellera ceci ; il n'appellera jamais `pty-bridge` directement.
 */
export {
    agentTabIn,
    handOverConflictsToAgent,
    writePromptInTab,
    type ComposeNotice,
    type HandOver,
    type HandOverDeps,
} from "./compose-prompt";
/**
 * Les tokens que le terminal lit dans la table de `app/styles.css`.
 *
 * Publiés parce qu'ils sont le contrat entre l'application, qui détient les palettes, et
 * la feature, qui les consomme — xterm.js peint ses cellules lui-même et ne peut pas
 * résoudre un `var(--ash-…)`. Voir `theme.ts`.
 */
export { TERMINAL_THEME_TOKENS } from "./theme";

/**
 * Le pont réel vers les commandes de PTY.
 *
 * Publié pour l'unique usage du composition root : l'onglet de merge (#30) passe « le
 * reste » à un agent par `writePromptInTab`, qui a besoin du même pont que l'atelier. Un
 * second pont fabriqué ailleurs serait un second chemin vers `pty_compose`, donc une
 * seconde occasion d'oublier la sélection préalable qu'ADR-0015 impose.
 */
export { tauriPty as tauriPtyBridge } from "./pty-bridge";

/** Ce que la feature annonce de ses onglets à qui les affiche autrement — la sidebar. */
export type TabsListener = (tabs: readonly Tab[], activeTabId: TabId | null) => void;

/**
 * L'onglet actif et l'état git du worktree qui le porte — de quoi dire **où l'on est**.
 *
 * Les deux faits sont déjà réunis ici, et une seule fois : la ligne de statut les lit à
 * chaque rendu, et `metadata` sort du cache qu'un unique abonnement à la surveillance
 * d'ADR-0011 alimente. La bande de titre de la fenêtre a besoin des mêmes ; les lui faire
 * relire par un second abonnement donnerait deux vérités qui se croisent
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * `metadata` à `null` veut dire « hors dépôt, ou pas encore lu » — la feature ne distingue
 * pas les deux, et rien de ce qui l'affiche n'a à le faire.
 */
export interface ActiveTab {
    /** L'onglet actif — **des deux genres** : un terminal, ou une surface d'outil. */
    readonly tab: Tab;
    readonly metadata: WorktreeMetadata | null;
}

/** `null` quand il n'y a aucun onglet — au démarrage, ou après la fermeture du dernier. */
export type ActiveTabListener = (active: ActiveTab | null) => void;

/** Les actions d'onglet, telles que le menu applicatif et la sidebar les déclenchent. */
export interface Terminals {
    openTab(origin: Origin): Promise<void>;
    closeActiveTab(): Promise<void>;
    selectTab(tabId: TabId): Promise<void>;
    selectTabAt(position: number): Promise<void>;
    /**
     * `Ctrl+Tab` / `Ctrl+Shift+Tab` : l'onglet voisin dans l'ordre du backend, en bouclant.
     *
     * Un seul point d'entrée pour les deux sens : ce sont la même règle lue dans deux
     * directions, et deux méthodes en auraient fait deux règles à garder d'accord.
     */
    cycleTab(step: Step): Promise<void>;
    clearActiveScrollback(): Promise<void>;
    /**
     * Relit la liste d'onglets, et sélectionne celui qu'on vient d'ouvrir ailleurs.
     *
     * L'onglet de merge (#30) naît d'un geste dans le panneau des conflits, pas d'un `⌘T` :
     * le backend le connaît avant la webview. C'est le seul chemin par lequel un onglet
     * ouvert hors de cette feature devient visible.
     */
    adoptTab(tabId: TabId): Promise<void>;
    /**
     * S'abonne à l'état des onglets.
     *
     * La feature ne connaît pas la sidebar : c'est le composition root qui relie les deux.
     * Et il n'y a **qu'un** abonnement à la boucle de sonde — deux features qui écouteraient
     * le même event afficheraient deux vérités qui se croisent
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    onTabs(listener: TabsListener): void;
    /**
     * S'abonne à l'onglet **actif** et à l'état git de son worktree.
     *
     * Pour la bande de titre de la fenêtre (spec §4.2), que le composition root relie : la
     * feature ne la connaît pas plus qu'elle ne connaît la sidebar.
     *
     * L'avis part **au changement**, et à l'abonnement : à un changement d'onglet, à un `cd`,
     * et quand la surveillance git répond. Pas au rythme du compteur de la ligne de statut,
     * qui bat chaque seconde et ne dit rien de nouveau sur l'endroit où l'on est — un abonné
     * n'a donc pas à se défendre de ce qu'on lui envoie.
     *
     * Un canal séparé d'`onTabs`, et non un élargissement de celui-ci : ce sont deux
     * questions différentes — *quels onglets, et lequel est actif* pour la sidebar, *où l'on
     * est* pour la bande de titre. Les fondre obligerait la sidebar à recevoir un état git
     * dont elle ne fait rien, ou la bande à le relire par un second abonnement à la
     * surveillance d'ADR-0011, et c'est ce second abonnement qui ferait deux vérités.
     */
    onActiveTab(listener: ActiveTabListener): void;
    /**
     * `⌘B` a replié ou déplié la sidebar.
     *
     * Repliée, elle ne nomme plus les agents, et la ligne de statut reprend celui qui attend
     * avec son raccourci. C'est tout ce qu'il en reste : le contexte — dépôt et branche — est
     * dans la bande de titre, qui surplombe les deux colonnes et ne dépend donc pas de `⌘B`.
     * Du temps de la barre d'onglets, le repli faisait aussi grossir le libellé de chaque
     * onglet ; la barre est partie avec cette moitié-là (spec §4.2, amendée le 2026-08-17).
     */
    setSidebarCollapsed(collapsed: boolean): void;
    /**
     * L'utilisateur a demandé la popup de branches depuis le pied de fenêtre (spec §7.1).
     *
     * La ligne de statut est l'**ancre** de cette popup, mais elle ne la connaît pas : elle
     * annonce le geste, et le composition root décide quoi en faire — comme pour la sidebar
     * et pour la bande de titre. La feature terminal n'importe rien de `features/git`.
     */
    onBranchesRequested(listener: () => void): void;
    /**
     * L'élément qui porte la branche, sur lequel la popup doit se poser.
     *
     * `null` quand la ligne ne montre pas de branche — hors dépôt, ou avant le premier
     * rendu. C'est celui qui ouvre la popup qui décide alors où la poser.
     */
    branchAnchor(): HTMLElement | null;
}

/**
 * Monte la pile de terminaux et sa ligne de statut dans `host`.
 *
 * Rien n'est ouvert ici : c'est au composition root de décider que l'application démarre
 * sur un onglet. C'est lui, aussi, qui passe `theme` et `fontSize` : la feature ne détecte
 * ni les bascules de palette ni les changements d'apparence, elle en est prévenue.
 *
 * Un onglet porte au plus un PTY, et un seul terminal est visible à la fois
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)). L'apparence, elle, ne se
 * règle pas par onglet : la taille **et la famille** valent pour toute l'application, et
 * c'est une décision de `features::theme` côté Rust — pas un effet de bord du câblage.
 */
export function mountTerminals(
    host: HTMLElement,
    theme: ThemeSignal,
    fontSize: FontSizeSignal,
    fontFamily: FontFamilySignal,
    below?: HTMLElement,
    createSurface?: ToolSurfaceFactory,
): Terminals {
    host.classList.add("terminal-workbench");

    // La pile est un conteneur positionné : chaque onglet s'y superpose en occupant
    // toute la surface, et seul l'actif est visible. Voir `xterm-view.ts` — les onglets
    // masqués gardent leur taille, sans quoi leur grille serait détruite au retour.
    const stack = document.createElement("div");
    stack.className = "terminal-stack";

    const listeners: TabsListener[] = [];
    const activeListeners: ActiveTabListener[] = [];
    // Le dernier contexte annoncé, pour ne pas le redire à chaque battement — voir
    // `announceActive`.
    let announced: ActiveTab | null = null;

    // La ligne de statut parle de l'onglet **actif** et du worktree qui le porte
    // (ADR-0012). Elle ne détient rien : le `cwd` vient de la sonde, l'état git de la
    // surveillance, l'état d'agent du backend.
    const branchListeners: (() => void)[] = [];
    const status = new StatusLine(
        () => {
            for (const listener of branchListeners) listener();
        },
        // Le clic sur une ligne du menu contextuel part en **bascule** vers le backend, et
        // rien n'est appliqué ici : ce qui revient par l'event est ce que la barre montrera
        // (ADR-0009). Un échec se tait — l'affichage n'a alors simplement pas bougé.
        (segment) => {
            void tauriStatusBar.toggle(segment).catch(() => undefined);
        },
    );
    let shown: TabsState = noTabs;
    let sidebarCollapsed = false;
    /**
     * Les deux quotas du compte, tels que le backend les a annoncés — **pas** un état que la
     * webview tiendrait.
     *
     * Ils sont ici et non dans le modèle de la ligne parce qu'ils ne parlent d'aucun onglet :
     * le seul chemin qui les change est l'event, et un changement d'onglet ne peut donc rien
     * leur faire. `null` des deux côtés est le départ, et c'est aussi ce que le backend rend
     * quand il ne sait rien (ADR-0016, condition 3) : dans les deux cas, on n'affiche rien.
     */
    let account: AccountUsage = { session: null, weekly: null };

    /**
     * L'onglet actif et l'état git de son worktree, lus au même instant.
     *
     * Un seul endroit les rapproche, et c'est ce qui garantit que la ligne de statut et la
     * bande de titre ne peuvent pas raconter deux endroits différents.
     */
    function currentActive(): ActiveTab | null {
        const tab = activeTab(shown);
        if (tab === null) return null;
        return { tab, metadata: metadata.of(tab.location?.worktreeRoot ?? null) };
    }

    // Déclaration de fonction, et non `const` : le cache l'appelle depuis un rappel posé
    // dans son constructeur, donc avant la fin de ce bloc.
    function drawStatus(): void {
        const now = Date.now();
        const seen = currentActive();
        const known = seen?.metadata ?? null;
        // Les deux rythmes de la ligne, composés côte à côte sur le **même** `now` : à
        // gauche ce qui parle de l'onglet, à droite ce qui parle du compte. Deux composeurs
        // purs, deux entrées de la ligne — et aucun chemin par lequel un changement d'onglet
        // pourrait atteindre les quotas.
        status.render(composeStatusLine(shown, known, sidebarCollapsed, now));
        // Le décompte des deux quotas — `resets in 2h14` — avance comme la durée d'état :
        // au battement, à partir d'une date absolue, sans rien redemander à personne.
        status.showQuotas(composeQuotas(account, now));
        announceActive(seen);
    }

    /**
     * Prévient les abonnés — mais **seulement quand ça a changé**.
     *
     * `drawStatus` bat une fois par seconde pour faire avancer la durée de la ligne de statut ;
     * l'onglet actif, lui, ne change pas à ce rythme. Sans ce filtre, `onActiveTab` promettrait
     * un changement et livrerait un tic d'horloge, et chaque abonné devrait se défendre de
     * l'écriture par seconde — une règle à retenir de plus à l'interface, pour rien.
     *
     * Comparaison par **référence** : l'onglet vient de l'état que le backend annonce, qui
     * garde ses objets tant que rien ne bouge (`tabs.ts`), et `metadata` sort d'un cache qui
     * ne remplace le sien qu'à la réponse d'une surveillance.
     */
    function announceActive(seen: ActiveTab | null): void {
        const unchanged =
            seen === announced ||
            (seen !== null &&
                announced !== null &&
                seen.tab === announced.tab &&
                seen.metadata === announced.metadata);
        if (unchanged) return;

        announced = seen;
        for (const listener of activeListeners) listener(seen);
    }

    const metadata = new WorktreeMetadataStore(tauriGit, drawStatus);

    // Le compteur de la ligne de statut (`working · 15m22s`) est un fait d'affichage : le
    // backend date l'entrée dans un état **une fois**, et c'est ce battement-ci qui fait
    // avancer les secondes. Le faire côté backend rendrait la fiche de chaque onglet actif
    // différente à chaque seconde, donc réveillerait la sidebar entière pour animer un
    // chiffre.
    //
    // Un redessin d'une ligne par seconde, sans rien redemander à personne : `metadata.of`
    // lit un cache, et ne déclenche une lecture qu'au premier worktree jamais demandé. Rien
    // à désabonner : comme l'atelier, la ligne de statut vit aussi longtemps que la fenêtre.
    //
    // Posé **après** le cache, et non avant : `drawStatus` le lit, et un battement qui
    // partirait pendant l'assemblage tomberait sur une liaison pas encore initialisée.
    setInterval(drawStatus, 1000);

    /**
     * Les deux quotas du compte (spec §4.2, ADR-0016).
     *
     * Le couple du thème : on **lit une fois** en s'affichant — sans quoi la ligne resterait
     * muette jusqu'au prochain cycle du fil de fond, qui peut être long —, puis c'est l'event
     * qui tient à jour. La webview ne redemande jamais : elle n'a aucun moyen de faire partir
     * un appel réseau, et c'est une condition d'ADR-0016, pas une économie.
     *
     * Les deux échecs se taisent, et c'est la même règle que les valeurs elles-mêmes : un
     * quota qu'on n'a pas ne s'affiche pas, et rien ne dit pourquoi.
     */
    const takeAccountUsage = (usage: AccountUsage): void => {
        account = usage;
        drawStatus();
    };
    void tauriUsage.snapshot().then(takeAccountUsage, () => undefined);
    void tauriUsage.onAccountUsage(takeAccountUsage).catch(() => undefined);

    /**
     * Ce que la ligne de statut montre (spec §4.2, vue 5c).
     *
     * Le couple du thème, une troisième fois : on lit une fois en s'affichant, puis c'est
     * l'event qui tient à jour. La lecture peut échouer — la ligne garde alors les défauts
     * qu'elle s'est posés, weekly masqué et le reste visible, et rien ne le signale : un
     * fichier de préférence n'est jamais une raison d'écrire une erreur dans une barre
     * d'état.
     */
    const takeStatusBarSegments = (segments: StatusBarSegments): void => {
        status.showSegments(segments);
        drawStatus();
    };
    void tauriStatusBar.segments().then(takeStatusBarSegments, () => undefined);
    void tauriStatusBar.onSegments(takeStatusBarSegments).catch(() => undefined);

    const workbench = new TerminalWorkbench({
        bridge: tauriPty,
        // Chaque terminal suit le thème, la taille et la police pour son compte, et s'en
        // désabonne en se libérant : l'atelier n'a à connaître ni la palette ni l'apparence
        // pour savoir qu'un onglet est ouvert.
        // Les liens du terminal (spec §4.2) : chaque vue a les siens, et tous demandent au
        // même backend. Le `cwd` est lu **à chaque survol**, sur l'onglet affiché : c'est le
        // seul qui puisse recevoir la souris, et sa valeur change à chaque `cd` — la sonde
        // d'ADR-0005 la suit, et c'est elle qui donne à Ash ce qu'un terminal ordinaire n'a
        // pas, de quoi résoudre un chemin **relatif** sans se tromper.
        createView: () =>
            new XtermView(stack, theme, fontSize, fontFamily, {
                bridge: tauriLinks,
                cwd: () => {
                    // Un onglet de merge n'a pas de `cwd` (ADR-0003 : il ne porte pas de
                    // PTY), et il n'a pas de terminal non plus — d'où le `null`, qui laisse
                    // les chemins relatifs inertes plutôt que de les résoudre ailleurs.
                    const tab = activeTab(shown);
                    return tab?.kind === "shell" ? tab.cwd : null;
                },
            }),
        // La surface d'un onglet qui n'est pas un shell se pose dans la **même** pile : un
        // seul onglet visible à la fois, terminal ou non (ADR-0003). L'atelier ne sait pas
        // ce qu'elle montre ; le composition root, lui, sait la fabriquer.
        ...(createSurface === undefined ? {} : { createSurface }),
        confirmClose: (tab) => askToClose(host, tab.cwd),
        onRender: (state) => {
            shown = state;
            drawStatus();
            for (const listener of listeners) listener(state.tabs, state.activeTabId);
        },
    });

    drawStatus();
    // Trois rangées quand `below` est là, deux sinon : les terminaux, ce qui leur prend de la
    // hauteur, puis la ligne de statut. La feature ne sait pas ce qu'est `below` — c'est le
    // panneau bas (spec §4.3), que le composition root lui passe —, elle sait seulement qu'il
    // se pose **entre** les deux et que le terminal se réduit d'autant. C'est ce qui referme
    // la boucle : la pile rétrécit, son `ResizeObserver` refait la grille, et le PTY reçoit
    // son `SIGWINCH` par le seul chemin de redimensionnement qui existe (`xterm-view.ts`).
    //
    // Un slot, et pas un import : la feature terminal n'a pas à connaître une surface qui
    // n'est pas un terminal, et le panneau n'a pas à savoir dans quelle mise en page il est
    // posé. **Rien de ce qui descend ici ne porte de PTY**
    // ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)) : cette feature est la
    // seule à en ouvrir, et elle ne les met que dans `stack`.
    host.append(...(below === undefined ? [stack, status.element] : [stack, below, status.element]));

    return {
        openTab: (origin) => workbench.openTab(origin),
        closeActiveTab: () => workbench.closeActive(),
        selectTab: (tabId) => workbench.select(tabId),
        selectTabAt: (position) => workbench.selectAt(position),
        cycleTab: (step) => workbench.cycle(step),
        clearActiveScrollback: () => workbench.clearActive(),
        adoptTab: async (tabId) => {
            await workbench.refresh();
            await workbench.select(tabId);
        },
        onTabs: (listener) => {
            listeners.push(listener);
            // L'abonné arrive après le premier rendu : lui donner l'état courant tout de
            // suite lui évite d'attendre le prochain `cd` pour afficher quoi que ce soit.
            listener(workbench.tabs.tabs, workbench.tabs.activeTabId);
        },
        onActiveTab: (listener) => {
            activeListeners.push(listener);
            // Même raison que pour `onTabs`, et de la même façon — au seul nouvel abonné :
            // il arrive après le premier rendu, et une bande de titre qui attendrait le
            // premier `cd` pour s'écrire serait vide au démarrage.
            listener(currentActive());
        },
        setSidebarCollapsed: (collapsed) => {
            sidebarCollapsed = collapsed;
            drawStatus();
        },
        onBranchesRequested: (listener) => {
            branchListeners.push(listener);
        },
        branchAnchor: () => status.anchor,
    };
}
