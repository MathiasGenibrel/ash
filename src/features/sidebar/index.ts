/**
 * API publique de la feature sidebar.
 *
 * Le reste du frontend n'importe que ce fichier : ni `tree`, ni `view`, ni `states`, ni
 * `labels` ne sont des points d'entrée.
 *
 * La sidebar **rend** la hiérarchie d'
 * [ADR-0012](../../../docs/adr/0012-worktree-unite-de-travail.md) ; elle ne la détient pas.
 * Elle n'appelle aucune commande Tauri et ne lit aucun fichier : le composition root lui
 * passe les onglets que le backend décrit, déjà situés
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */

import "./sidebar.css";

import type { TabId, TabInfo, SidebarRows } from "@/shared/ipc";
import {
    appliedWidth,
    DEFAULT_SIDEBAR_WIDTH,
    RAIL_WIDTH,
    type SidebarColumnState,
} from "./resize";
import { createSidebarResizer } from "./resizer";
import { buildSidebar } from "./tree";
import { SidebarView } from "./view";
import { showsSubagents } from "./visible";

export type { SidebarGroup, SidebarTree, WorktreeNode } from "./tree";
export { DEFAULT_SIDEBAR_WIDTH, type SidebarColumnState } from "./resize";

/** Ce que la sidebar sait demander, et qu'elle ne sait pas faire elle-même. */
export interface SidebarPorts {
    /** Cliquer un agent, c'est aller à son onglet. */
    selectTab(tabId: TabId): void;
    /** Le `+` du pied. */
    newTab(): void;
    /**
     * Le clic sur une ligne de worktree **sans onglet** : en ouvrir un dans ce worktree
     * (spec §5.2).
     *
     * La sidebar ne sait pas ouvrir un PTY — elle nomme le worktree, et passe la main, comme
     * pour le marqueur d'instrumentation (ADR-0010). C'est le composition root qui relie.
     */
    openTabIn(worktreeRoot: string): void;
    /**
     * Épingler ou désépingler un worktree.
     *
     * Le geste **part**, il ne se pose pas ici : ce qui survit à la fermeture vit en Rust
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et la colonne
     * redessinera quand le backend l'aura annoncé.
     */
    setPinned(worktreeRoot: string, pinned: boolean): void;
    /** Replier ou déplier une ligne — un worktree, ou un groupe de dépôt. Même chemin. */
    setCollapsed(key: string, collapsed: boolean): void;
    /**
     * Le marqueur « non instrumenté » d'une ligne d'agent : ouvrir les réglages sur cet outil.
     *
     * La sidebar **informe** ; c'est l'écran qui agit
     * ([ADR-0010](../../../docs/adr/0010-sidebar-informe-terminal-agit.md)). Elle ne sait ni
     * écrire un fichier, ni ce qu'instrumenter veut dire — elle nomme l'outil, et passe la
     * main.
     */
    instrument(command: string, adapter: string): void;
    /**
     * La largeur réglée en relâchant le bord — ou en poussant une flèche sur le séparateur.
     *
     * Elle **part**, elle ne se pose pas ici : la largeur survit à la fermeture, donc elle
     * vit en Rust avec le thème et la taille de police
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et la colonne prend
     * celle que le backend annonce. Ce qui suit le pointeur pendant le glissement est un fait
     * d'affichage, et le reste.
     */
    setColumnWidth(width: number): void;
    /** Refermer la colonne — un glissement relâché sous le plancher. */
    setColumnCollapsed(collapsed: boolean): void;
    /** `⌘B`, et la touche du séparateur : le même geste, et le même détenteur. */
    toggleColumn(): void;
    /**
     * La largeur de la fenêtre, d'où se déduisent les bornes de 10 % et 80 %.
     *
     * Injectée comme l'horloge, et pour la même raison : c'est une lecture du monde, et la
     * colonne n'a pas à savoir qu'elle vit dans une `window`.
     */
    viewportWidth?: () => number;
    /**
     * L'heure qu'il est, pour les durées des lignes de sous-agents.
     *
     * Injectée plutôt que lue, comme partout ailleurs où le temps entre dans le produit :
     * `Date.now` par défaut, et le composition root n'a rien à en dire.
     */
    now?: () => number;
}

export interface Sidebar {
    readonly element: HTMLElement;
    /**
     * Le séparateur du bord droit : la zone qu'on attrape, et la poignée qui l'annonce.
     *
     * Il est rendu **à part** de la colonne parce qu'il la déborde de 7 px : posé dedans,
     * l'`overflow: hidden` qui empêche les lignes de fuir couperait la moitié de la zone
     * attrapable. Le composition root le pose à côté de la colonne, dans la même grille.
     */
    readonly separator: HTMLElement;
    /**
     * Dessine la colonne à partir de ce que le backend détient : les onglets, l'onglet actif,
     * et l'état gardé d'une session à l'autre — les épingles et les lignes repliées.
     */
    render(
        tabs: readonly TabInfo[],
        activeTabId: TabId | null,
        kept: SidebarRows,
    ): void;
    /**
     * La largeur et le repli que le backend vient d'annoncer.
     *
     * Un canal séparé de [`render`], et non un cinquième paramètre : les onglets bougent à
     * chaque `cd`, la colonne seulement quand on la règle — et c'est **cette** annonce, et
     * elle seule, qui remplace la largeur montrée pendant un glissement.
     */
    setColumn(column: SidebarColumnState): void;
    /**
     * Le raccourci **en vigueur** de « nouvel onglet », tel que le backend le rend.
     *
     * La colonne l'affiche, elle ne le connaît pas : les liaisons sont réglables et détenues
     * en Rust (spec §4.4, issue #22), et une combinaison écrite dans le TypeScript
     * deviendrait fausse au premier rebinding. Vide veut dire « aucun raccourci », et se
     * montre en n'affichant rien.
     */
    showNewTabShortcut(keys: string): void;
}

export function mountSidebar(ports: SidebarPorts): Sidebar {
    // Trois replis, et ils ne se confondent pas : la **colonne** (`⌘B`), chaque **dépôt**,
    // et chaque **worktree** pris séparément (ADR-0012, spec §4.1).
    //
    // **Aucun des trois ne vit ici.** Les deux replis de ligne survivent au redémarrage
    // (spec §5.2), et celui de la colonne aussi depuis qu'elle est redimensionnable (#129) :
    // `⌘B` et la poignée agissent sur le même état, donc il n'y en a qu'un, et il est en Rust
    // avec le thème ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). La
    // colonne rend ce qu'on lui annonce.
    let column: SidebarColumnState = { width: DEFAULT_SIDEBAR_WIDTH, collapsed: false };

    // La largeur qui suit le pointeur pendant un glissement, et rien d'autre. Elle n'est pas
    // un second détenteur : elle est effacée par la première annonce du backend, et elle est
    // au repli ce que le compteur de durée de la ligne de statut est à `stateSince` — un fait
    // d'affichage. Elle survit au relâchement le temps de l'aller-retour, sans quoi la colonne
    // reviendrait d'une image à sa largeur précédente avant de repartir à la bonne.
    let dragged: number | null = null;

    let tabs: readonly TabInfo[] = [];
    let activeTabId: TabId | null = null;
    let kept: SidebarRows = { pinned: [], collapsed: [] };
    const now = ports.now ?? ((): number => Date.now());
    const viewportWidth = ports.viewportWidth ?? ((): number => window.innerWidth);

    // Le battement qui fait avancer les durées des lignes filles, **et seulement quand il y
    // en a une à l'écran**. La colonne entière se redessine à chaque rendu : la faire battre
    // en permanence coûterait un rendu par seconde pour animer un compteur qui n'existe pas
    // la plupart du temps. Sans sous-agent, la sidebar redevient exactement ce qu'elle était
    // — dessinée sur événement, et jamais autrement.
    let ticker: ReturnType<typeof setInterval> | null = null;

    const view = new SidebarView({
        selectTab: (tabId) => {
            ports.selectTab(tabId);
        },
        // Les deux replis de ligne partent au backend et reviennent par son annonce : rien
        // n'est posé ici, sans quoi la colonne et le fichier pourraient se contredire.
        toggleRowCollapsed: (key) => {
            ports.setCollapsed(key, !collapsed().has(key));
        },
        newTab: () => {
            ports.newTab();
        },
        openTabIn: (worktreeRoot) => {
            ports.openTabIn(worktreeRoot);
        },
        setPinned: (worktreeRoot, pinned) => {
            ports.setPinned(worktreeRoot, pinned);
        },
        instrument: (command, adapter) => {
            ports.instrument(command, adapter);
        },
    });

    // Le séparateur ne détient rien non plus : il lit la colonne annoncée, applique la règle
    // de `resize.ts`, et passe la main. Seul le relâchement traverse la frontière.
    const resizer = createSidebarResizer({
        column: () => column,
        viewportWidth,
        preview: (width) => {
            dragged = width;
            layOut();
        },
        commitWidth: (width) => {
            dragged = width;
            layOut();
            ports.setColumnWidth(width);
        },
        collapse: () => {
            dragged = null;
            ports.setColumnCollapsed(true);
        },
        toggle: () => {
            ports.toggleColumn();
        },
    });

    /**
     * Les lignes repliées, telles que le backend les a annoncées.
     *
     * Un seul ensemble pour les deux niveaux : les clés d'un groupe sont préfixées (`repo:`,
     * `flat:`) et celles d'un worktree sont des chemins absolus, donc elles ne peuvent pas se
     * confondre — et `state.json` n'a qu'une liste à garder.
     */
    function collapsed(): ReadonlySet<string> {
        return new Set(kept.collapsed);
    }

    function draw(): void {
        const tree = buildSidebar(tabs, {
            activeTabId,
            collapsed: collapsed(),
            pinned: kept.pinned,
        });
        view.render(tree, column.collapsed, now());
        beat(showsSubagents(tree, column.collapsed));
        layOut();
    }

    /**
     * Pose la largeur de la colonne, en une seule propriété.
     *
     * Elle est portée par la **racine du document** parce que deux éléments de deux sous-arbres
     * la lisent — la colonne, et le séparateur qui se place sur son bord. C'est le même chemin
     * que la palette de `app/theme.ts`, et pour la même raison : une valeur de fenêtre, lue par
     * du CSS.
     *
     * `appliedWidth` est rappelée à **chaque** pose, et pas seulement quand la largeur change :
     * c'est ce qui fait que réduire la fenêtre ramène la colonne dans ses bornes sans jamais
     * réécrire la largeur qu'on a réglée.
     */
    function layOut(): void {
        const width = dragged ?? column.width;
        const shown =
            dragged === null && column.collapsed ? RAIL_WIDTH : appliedWidth(width, viewportWidth());
        document.documentElement.style.setProperty("--ash-sidebar-width", `${shown}px`);
        resizer.update();
    }

    /**
     * Ouvre ou ferme le battement des durées, sans jamais en laisser deux.
     *
     * Le battement rappelle [`draw`] lui-même, et non un rendu à lui : c'est ce qui lui permet
     * de **s'arrêter tout seul** quand la dernière ligne fille a fini d'expirer. Un second
     * chemin de rendu, qui ne repasserait pas par [`showsSubagents`], laisserait battre la
     * colonne pour toujours le jour où le backend n'a plus rien à annoncer — et deux chemins
     * de rendu finiraient de toute façon par ne plus dessiner la même chose.
     */
    function beat(wanted: boolean): void {
        if (wanted === (ticker !== null)) return;
        if (ticker !== null) {
            clearInterval(ticker);
            ticker = null;
            return;
        }
        ticker = setInterval(draw, 1000);
    }

    // La fenêtre qui rétrécit ne redessine pas la colonne — elle la **replace** dans ses
    // bornes. `resize` de la fenêtre suffit : la colonne ne change pas de largeur pour une
    // autre raison que la sienne, et un `ResizeObserver` ici observerait un élément dont c'est
    // justement nous qui posons la largeur.
    window.addEventListener("resize", layOut);

    draw();

    return {
        element: view.element,
        separator: resizer.element,
        render(nextTabs, nextActive, nextKept) {
            tabs = nextTabs;
            activeTabId = nextActive;
            kept = nextKept;
            draw();
        },
        setColumn(next) {
            column = next;
            // L'annonce du backend **remplace** ce que le glissement montrait : c'est elle qui
            // fait autorité, et c'est le seul endroit où la largeur montrée redevient la
            // largeur gardée.
            dragged = null;
            draw();
        },
        showNewTabShortcut(keys) {
            view.showNewTabShortcut(keys);
        },
    };
}
