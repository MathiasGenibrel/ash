import type { AgentState, TabId, TabInfo } from "@/shared/ipc";

/**
 * Les règles d'affichage de la sidebar, sans DOM ni IPC.
 *
 * C'est le cœur de la hiérarchie d'
 * [ADR-0012](../../../docs/adr/0012-worktree-unite-de-travail.md) : une liste plate
 * d'onglets entre, un arbre dépôt → worktree → onglets sort. **Rien n'est résolu ici** —
 * le backend a déjà dit, pour chaque onglet, quel worktree le porte et quel dépôt le
 * groupe ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ce module range,
 * nomme et ordonne ; il ne devine pas.
 */

/** Au-delà, un nom est coupé : à 240 px, la colonne ne montre pas plus. */
export const MAX_LABEL = 26;

export interface SidebarTabNode {
    readonly tabId: TabId;
    /** Le nom affiché, déjà tronqué. */
    readonly label: string;
    /** Le nom entier, pour l'infobulle. */
    readonly title: string;
    readonly state: AgentState;
    readonly active: boolean;
}

export interface WorktreeNode {
    /** La racine du worktree : c'est elle qui l'identifie, pas son nom. */
    readonly key: string;
    readonly label: string;
    readonly title: string;
    /**
     * Le `·sidebar` du design, aligné à droite.
     *
     * `null` dans la forme à plat : un worktree seul sous son dépôt n'a personne dont se
     * distinguer, et le suffixe n'y serait qu'un ornement.
     */
    readonly suffix: string | null;
    /** Replié : la ligne reste, ses onglets disparaissent. Propriété du **worktree**. */
    readonly collapsed: boolean;
    readonly tabs: readonly SidebarTabNode[];
    /** L'état le plus urgent de ses onglets — ce que la ligne montre quand elle est repliée. */
    readonly state: AgentState;
}

/**
 * Les deux formes d'ADR-0012, et rien entre les deux.
 *
 * `flat` : un dépôt sans worktree lié, ou un dossier hors dépôt. Deux niveaux visibles.
 * `repo` : un dépôt qui héberge des worktrees liés. Trois niveaux.
 *
 * Un dépôt sans worktree lié ne gagne **jamais** le niveau intermédiaire : c'est ce que le
 * `repo: null` du backend dit, et le seul rôle de ce module est de le rendre.
 */
export type SidebarGroup =
    | {
          readonly kind: "flat";
          readonly key: string;
          readonly worktree: WorktreeNode;
          readonly state: AgentState;
      }
    | {
          readonly kind: "repo";
          readonly key: string;
          readonly label: string;
          readonly title: string;
          readonly worktrees: readonly WorktreeNode[];
          readonly state: AgentState;
      };

export interface SidebarTree {
    readonly groups: readonly SidebarGroup[];
    readonly tabCount: number;
    readonly waitingCount: number;
}

export interface SidebarOptions {
    readonly activeTabId: TabId | null;
    /** Les worktrees repliés, par racine. */
    readonly collapsed: ReadonlySet<string>;
}

export const emptyTree: SidebarTree = { groups: [], tabCount: 0, waitingCount: 0 };

/**
 * Range les onglets par worktree, et les worktrees par dépôt.
 *
 * L'ordre est celui de **première apparition** des onglets, à tous les niveaux. C'est
 * l'ordre que le backend détient — celui que `⌘1..9` numérote — et le seul qui ne
 * réorganise pas la colonne sous les yeux de l'utilisateur quand un agent démarre. Un tri
 * alphabétique ferait sauter les lignes à chaque ouverture d'onglet.
 */
export function buildSidebar(
    tabs: readonly TabInfo[],
    options: SidebarOptions,
): SidebarTree {
    const groups = new Map<string, MutableGroup>();

    for (const tab of tabs) {
        const place = placeOf(tab);
        const group = groupFor(groups, place);
        const worktree = worktreeFor(group, place);
        worktree.tabs.push({
            tabId: tab.tabId,
            label: truncate(tab.process),
            title: tab.process,
            state: tab.state,
            active: tab.tabId === options.activeTabId,
        });
    }

    return {
        groups: [...groups.values()].map((group) => freeze(group, options.collapsed)),
        tabCount: tabs.length,
        waitingCount: tabs.filter((tab) => tab.state === "waiting").length,
    };
}

/**
 * L'état qu'une ligne de dépôt ou de worktree montre pour ses enfants.
 *
 * L'ordre d'urgence n'est pas cosmétique : `waiting` est le seul état qui **demande**
 * quelque chose à l'utilisateur, donc il l'emporte sur tout, y compris sur une erreur —
 * une erreur attendra, une question bloque un agent. `idle` ne remonte jamais tant qu'il
 * reste autre chose à dire.
 */
export function bubbleState(states: readonly AgentState[]): AgentState {
    const urgency: readonly AgentState[] = ["waiting", "error", "working", "done", "idle"];
    return urgency.find((state) => states.includes(state)) ?? "idle";
}

/**
 * Le suffixe qui distingue deux worktrees d'un même dépôt : `omelette-web` → `·web`.
 *
 * Le design ne prend que le **dernier segment** du nom de dossier — c'est ce qui rend
 * `·sidebar` et `·toc` lisibles côte à côte là où `omelette-sidebar` et `omelette-toc` se
 * ressemblent trop pour être distingués du coin de l'œil.
 */
export function shortSuffix(worktreeName: string): string {
    const segments = worktreeName.split("-").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? worktreeName;
}

/** Coupe un nom trop long, en gardant le début — c'est lui qui identifie. */
export function truncate(text: string, max: number = MAX_LABEL): string {
    return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

/** Deux lettres pour le rail replié : `omelette-web` → `ow`, `ash` → `as`. */
export function abbreviate(name: string): string {
    const segments = name.split(/[-_. ]/).filter((segment) => segment.length > 0);
    const initials = segments
        .slice(0, 2)
        .map((segment) => segment[0] ?? "")
        .join("");
    return (initials.length === 2 ? initials : name.slice(0, 2)).toLowerCase();
}

/** Ce que le backend a dit d'un onglet, réduit à ce dont le rangement a besoin. */
interface Place {
    readonly groupKey: string;
    readonly repo: { readonly id: string; readonly name: string } | null;
    readonly worktreeKey: string;
    readonly worktreeName: string;
}

/**
 * Un onglet que le backend n'a pas su situer reste **affiché** : c'est son propre
 * worktree, à plat, nommé d'après son répertoire. Le masquer serait la seule façon de
 * perdre un onglet vivant.
 */
function placeOf(tab: TabInfo): Place {
    const location = tab.location;
    if (location === null) {
        return {
            groupKey: `flat:${tab.cwd}`,
            repo: null,
            worktreeKey: tab.cwd,
            worktreeName: basename(tab.cwd),
        };
    }

    const repo = location.repo;
    return {
        groupKey: repo === null ? `flat:${location.worktreeRoot}` : `repo:${repo.id}`,
        repo,
        worktreeKey: location.worktreeRoot,
        worktreeName: location.worktreeName,
    };
}

interface MutableGroup {
    readonly key: string;
    readonly repo: { readonly id: string; readonly name: string } | null;
    readonly worktrees: Map<string, MutableWorktree>;
}

interface MutableWorktree {
    readonly key: string;
    readonly name: string;
    readonly tabs: SidebarTabNode[];
}

function groupFor(groups: Map<string, MutableGroup>, place: Place): MutableGroup {
    const known = groups.get(place.groupKey);
    if (known !== undefined) return known;

    const group: MutableGroup = {
        key: place.groupKey,
        repo: place.repo,
        worktrees: new Map(),
    };
    groups.set(place.groupKey, group);
    return group;
}

function worktreeFor(group: MutableGroup, place: Place): MutableWorktree {
    const known = group.worktrees.get(place.worktreeKey);
    if (known !== undefined) return known;

    const worktree: MutableWorktree = {
        key: place.worktreeKey,
        name: place.worktreeName,
        tabs: [],
    };
    group.worktrees.set(place.worktreeKey, worktree);
    return worktree;
}

function freeze(group: MutableGroup, collapsed: ReadonlySet<string>): SidebarGroup {
    const worktrees = [...group.worktrees.values()];
    const suffixes = suffixesOf(
        worktrees.map((worktree) => worktree.name),
        group.repo !== null,
    );

    const nodes = worktrees.map((worktree, index) =>
        node(worktree, suffixes[index] ?? null, collapsed),
    );
    const state = bubbleState(nodes.map((worktree) => worktree.state));

    if (group.repo === null) {
        // La forme à plat n'a **qu'un** worktree par construction : sa clé de groupe est
        // sa racine. Le repli est donc toujours possible, mais jamais un niveau de plus.
        const only = nodes[0];
        if (only !== undefined) {
            return { kind: "flat", key: group.key, worktree: only, state };
        }
    }

    return {
        kind: "repo",
        key: group.key,
        label: truncate(group.repo?.name ?? ""),
        title: group.repo?.name ?? "",
        worktrees: nodes,
        state,
    };
}

function node(
    worktree: MutableWorktree,
    suffix: string | null,
    collapsed: ReadonlySet<string>,
): WorktreeNode {
    return {
        key: worktree.key,
        label: truncate(worktree.name),
        title: worktree.name,
        suffix,
        collapsed: collapsed.has(worktree.key),
        tabs: worktree.tabs,
        state: bubbleState(worktree.tabs.map((tab) => tab.state)),
    };
}

/**
 * Les suffixes d'un groupe, ou aucun dans la forme à plat.
 *
 * Deux worktrees dont le dernier segment est le même (`api-sidebar`, `web-sidebar`)
 * rendraient deux fois `·sidebar` : le suffixe cesserait alors de distinguer quoi que ce
 * soit, ce qui est précisément son seul rôle. Dans ce cas, tout le groupe reprend le nom
 * de dossier entier.
 */
function suffixesOf(names: readonly string[], grouped: boolean): (string | null)[] {
    if (!grouped) return names.map(() => null);

    const shortened = names.map(shortSuffix);
    const distinct = new Set(shortened).size === shortened.length;
    return (distinct ? shortened : names).map((suffix) => `·${suffix}`);
}

/** Dernier segment d'un chemin — `/` reste `/`. */
function basename(path: string): string {
    const segments = path.split("/").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? "/";
}
