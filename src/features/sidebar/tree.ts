import { isShell } from "@/shared/ipc";
import type { AgentState, PinnedWorktree, Tab, TabId, TabLocation } from "@/shared/ipc";
import { instrumentationMark, type InstrumentationMark } from "./instrumentation";
import { basename, shortSuffix, truncate } from "./labels";
import { bubbleState } from "./states";
import { subagentNodes, type SubagentNode } from "./subagents";

/**
 * La hiérarchie de la sidebar, sans DOM ni IPC.
 *
 * C'est le cœur d'[ADR-0012](../../../docs/adr/0012-worktree-unite-de-travail.md) : une
 * liste plate d'onglets entre, un arbre dépôt → worktree → onglets sort. **Rien n'est
 * résolu ici** — le backend a déjà dit, pour chaque onglet, quel worktree le porte et quel
 * dépôt le groupe ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ce
 * module range et ordonne ; il ne devine pas.
 *
 * Deux règles vivent à côté plutôt qu'ici, parce qu'elles ne connaissent pas l'arbre et
 * que d'autres lignes que ses nœuds s'en serviront : faire tenir un nom dans la colonne
 * ([`./labels`]) et choisir l'état qu'une ligne repliée montre ([`./states`]).
 */

export interface SidebarTabNode {
    readonly tabId: TabId;
    /** Le nom affiché, déjà tronqué. */
    readonly label: string;
    /** Le nom entier, pour l'infobulle. */
    readonly title: string;
    /**
     * L'état d'agent de la ligne, ou `null` pour une **surface d'outil** — l'onglet de merge
     * (#30), qui n'a pas de processus.
     *
     * `null` n'est pas un sixième état, et ce n'est pas non plus `idle` : `idle` veut dire
     * « un shell est là, à son invite », et l'afficher sous un onglet où rien ne tourne
     * ferait remonter un état inventé jusqu'à la ligne de dépôt
     * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md) : un état a une source, ou il
     * n'existe pas).
     */
    readonly state: AgentState | null;
    readonly active: boolean;
    /**
     * Le marqueur « non instrumenté », ou `null` quand la ligne n'a rien à signaler.
     *
     * Composé ici plutôt que dans la vue, comme les états repliés : la règle qui décide de ce
     * qu'une ligne signale se vérifie sans DOM (voir [`./instrumentation`]). Il porte aussi
     * **l'outil que son geste nomme** (ADR-0006), pour que la vue n'ait rien à recoller.
     */
    readonly mark: InstrumentationMark | null;
    /**
     * Les sous-agents qui tournent sous cet onglet (spec §6.5), dans leur ordre d'apparition.
     *
     * Vide dans le cas courant. Ils n'ont **pas** de repli à eux : une ligne d'onglet montre
     * ses enfants ou n'en a pas, et un troisième niveau de repli dans une colonne de 240 px
     * cacherait plus qu'il ne rangerait.
     */
    readonly subagents: readonly SubagentNode[];
}

/**
 * Tous les états qu'une ligne d'onglet représente : le sien, et ceux de ses enfants.
 *
 * C'est ce qui fait qu'une ligne repliée porte l'état le plus urgent **sous-agents compris**
 * (spec §4.1) : la remontée d'un worktree ou d'un dépôt part d'ici, et non du seul
 * `tab.state`. Un `working` d'enfant sous un onglet dont l'agent a fini remonte donc, ce qui
 * est juste — il se passe encore quelque chose là-dessous.
 */
export function tabStates(tab: SidebarTabNode): readonly AgentState[] {
    const own = tab.state === null ? [] : [tab.state];
    return [...own, ...tab.subagents.map((child) => child.state)];
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
    /**
     * Épinglé : la ligne reste dans la colonne même sans onglet, et survit à la fermeture
     * (spec §5.2).
     *
     * Le fait vient du backend, comme tout le reste de cet arbre : la colonne ne décide pas
     * ce qui est épinglé, elle le rend
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    readonly pinned: boolean;
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
          /**
           * Replié : la ligne du dépôt reste, ses worktrees disparaissent (spec §4.1).
           *
           * La forme à plat n'a pas cette propriété — son unique worktree *est* sa ligne, et
           * c'est le repli de ce worktree qui joue.
           */
          readonly collapsed: boolean;
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
    /**
     * Les worktrees épinglés, déjà situés par le backend.
     *
     * Ceux qui n'ont aucun onglet gagnent une ligne à eux — c'est tout ce qui les distingue
     * d'un worktree ordinaire. Ceux qui en ont une déjà la marquent seulement, pour que le
     * geste de désépinglage soit là où l'épingle a été posée.
     */
    readonly pinned: readonly PinnedWorktree[];
    /**
     * Les lignes repliées — **un seul ensemble pour les deux niveaux**.
     *
     * Un worktree replié y est par sa racine, un groupe de dépôt par sa clé préfixée
     * (`repo:`, `flat:`) : les deux familles ne peuvent pas se confondre. C'est aussi une
     * seule liste dans `~/.ash/state.json`, et le backend est le seul à la détenir — deux
     * ensembles ici obligeraient l'appelant à passer deux fois la même chose, donc à pouvoir
     * les faire mentir.
     */
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
export function buildSidebar(tabs: readonly Tab[], options: SidebarOptions): SidebarTree {
    const groups = new Map<string, MutableGroup>();

    for (const tab of tabs) {
        const place = placeOf(tab);
        const group = groupFor(groups, place);
        const worktree = worktreeFor(group, place);
        // Un onglet de merge n'a ni programme en avant-plan, ni état, ni sous-agents : sa
        // ligne dit ce que l'onglet **est**, et rien qu'il ne détienne pas.
        worktree.tabs.push(
            isShell(tab)
                ? {
                      tabId: tab.tabId,
                      label: truncate(tab.process),
                      title: tab.process,
                      state: tab.state,
                      active: tab.tabId === options.activeTabId,
                      mark: instrumentationMark(tab.agent),
                      subagents: subagentNodes(tab.subagents),
                  }
                : {
                      tabId: tab.tabId,
                      label: truncate(tab.title),
                      title: tab.title,
                      state: null,
                      active: tab.tabId === options.activeTabId,
                      mark: null,
                      subagents: [],
                  },
        );
    }

    // Les épingles **après** les onglets, et jamais avant : l'ordre de la colonne est celui
    // de première apparition des onglets, et une ligne épinglée sans onglet n'a pas d'onglet
    // dont elle tiendrait le rang. Un worktree déjà habité ne bouge donc pas de place le jour
    // où on l'épingle — il gagne seulement sa marque.
    for (const pinned of options.pinned) {
        const place = placeOfWorktree(pinned);
        worktreeFor(groupFor(groups, place), place);
    }

    return {
        groups: [...groups.values()].map((group) => freeze(group, options)),
        tabCount: tabs.length,
        waitingCount: tabs.filter((tab) => isShell(tab) && tab.state === "waiting").length,
    };
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
function placeOf(tab: Tab): Place {
    const location = tab.location;
    if (location === null) {
        // Le repli diffère selon le genre : un shell se range par son répertoire courant,
        // une surface d'outil par la racine du worktree qu'elle traite.
        const path = isShell(tab) ? tab.cwd : tab.worktreeRoot;
        return {
            groupKey: `flat:${path}`,
            repo: null,
            worktreeKey: path,
            worktreeName: basename(path),
        };
    }
    return placeOfWorktree(location);
}

/**
 * Où se range un worktree que le backend a su situer — **la** règle de rangement, écrite une
 * fois.
 *
 * Un onglet et une épingle y passent tous les deux, et c'est ce qui fait qu'épingler un
 * worktree déjà ouvert ne duplique pas sa ligne : deux règles jumelles finiraient par
 * diverger d'un préfixe, et la colonne montrerait deux fois le même worktree sans que rien
 * ne l'annonce. Le contrat s'en assure de son côté — un worktree épinglé **est** un
 * `TabLocation` (`shared/ipc`).
 */
function placeOfWorktree(worktree: TabLocation): Place {
    const repo = worktree.repo;
    return {
        groupKey: repo === null ? `flat:${worktree.worktreeRoot}` : `repo:${repo.id}`,
        repo,
        worktreeKey: worktree.worktreeRoot,
        worktreeName: worktree.worktreeName,
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

function freeze(group: MutableGroup, options: SidebarOptions): SidebarGroup {
    const worktrees = [...group.worktrees.values()];
    const suffixes = suffixesOf(
        worktrees.map((worktree) => worktree.name),
        group.repo !== null,
    );

    const pinned = new Set(options.pinned.map((entry) => entry.worktreeRoot));
    const nodes = worktrees.map((worktree, index) =>
        node(worktree, suffixes[index] ?? null, options.collapsed, pinned),
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
        collapsed: options.collapsed.has(group.key),
        worktrees: nodes,
        state,
    };
}

function node(
    worktree: MutableWorktree,
    suffix: string | null,
    collapsed: ReadonlySet<string>,
    pinned: ReadonlySet<string>,
): WorktreeNode {
    return {
        key: worktree.key,
        label: truncate(worktree.name),
        title: worktree.name,
        suffix,
        collapsed: collapsed.has(worktree.key),
        pinned: pinned.has(worktree.key),
        tabs: worktree.tabs,
        state: bubbleState(worktree.tabs.flatMap(tabStates)),
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
