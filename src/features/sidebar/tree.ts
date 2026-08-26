import { isShell } from "@/shared/ipc";
import type { AgentState, PinnedWorktree, RepoRef, Tab, TabId, TabLocation } from "@/shared/ipc";
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

/**
 * Ce qu'une ligne **écrit d'elle-même** : son nom tronqué, son nom entier, son suffixe.
 *
 * C'est un type à part depuis l'amendement du 2026-08-26 à ADR-0012, parce que la ligne
 * unique d'un groupe à plat n'écrit plus forcément le nom de son worktree : quand un dépôt
 * n'héberge qu'un worktree, elle porte le nom du **dépôt**. Ce que la ligne montre et ce
 * qu'elle désigne se séparent donc, et les deux se lisent dans le type plutôt que dans une
 * condition du rendu — l'épingle et le repli continuent de viser le worktree.
 */
export interface RowLabel {
    readonly label: string;
    readonly title: string;
    /**
     * Le `·sidebar` du design, aligné à droite.
     *
     * `null` quand il ne distinguerait rien : un worktree seul n'a pas de frère dont se
     * démarquer, et le suffixe n'y serait qu'un ornement.
     */
    readonly suffix: string | null;
}

export interface WorktreeNode extends RowLabel {
    /** La racine du worktree : c'est elle qui l'identifie, pas son nom. */
    readonly key: string;
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
 * `flat` : **au plus un** worktree — un dépôt qui n'en héberge qu'un, un dépôt sans
 * worktree lié, ou un dossier hors dépôt. Deux niveaux visibles.
 * `repo` : un dépôt qui héberge **deux worktrees ou plus**. Trois niveaux.
 *
 * Le critère est le nombre de worktrees, et non la présence d'un dépôt : c'est
 * l'amendement du 2026-08-26 à ADR-0012. Un niveau intermédiaire qui ne porterait qu'une
 * ligne répéterait le dépôt du dessus (`ash` → `ash ·ash` → `claude`) et son compteur
 * dirait `1 worktree`, ce qui n'informe de rien. Le niveau revient dès qu'il porte deux
 * vérités différentes — deux worktrees ont deux états d'arbre.
 */
export type SidebarGroup =
    | {
          /**
           * Une forme à plat n'a **pas de clé**, et c'est ce que le type dit ici.
           *
           * La clé d'un groupe ne sert qu'à une chose : replier sa ligne. Un groupe à plat
           * n'a pas de ligne à lui — son worktree *est* sa ligne —, donc une clé posée là
           * ne pourrait qu'être passée par erreur à `toggleRowCollapsed`, qui écrirait dans
           * `state.json` un repli que plus rien ne relit. « Jamais consulté » ne se garde
           * pas dans un commentaire : ici, c'est le compilateur qui le tient.
           */
          readonly kind: "flat";
          /**
           * Ce que la ligne **unique** écrit : le nom du dépôt quand il y en a un, celui du
           * dossier sinon — et le suffixe seulement s'il ajoute quelque chose.
           *
           * Séparé de `worktree` parce que les deux ne disent plus la même chose : la ligne
           * montre le dépôt, mais l'épingle, le repli et le clic visent toujours le
           * worktree, qui reste l'unité de rattachement (ADR-0012).
           */
          readonly row: RowLabel;
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
           * c'est le repli de ce worktree qui joue. Les deux clés sont distinctes
           * (`repo:<id>` et la racine du worktree), donc un dépôt qui passe d'une forme à
           * l'autre ne perd ni l'un ni l'autre repli en chemin.
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
     * (`repo:<id>`) : les deux familles ne peuvent pas se confondre — un chemin absolu ne
     * commence pas par `repo:`. Une forme à plat n'écrit **jamais** ici : elle n'a pas de
     * ligne de groupe à replier (voir [`SidebarGroup`]), donc une clé `flat:` laissée par
     * une version antérieure y dort sans rien replier, et sans rien casser. C'est aussi une
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
    // **Un worktree, une ligne** : c'est le critère de la forme à plat depuis l'amendement
    // du 2026-08-26 à ADR-0012, et il remplace « pas de dépôt du tout ». Un dépôt sans
    // worktree lié y tombe toujours — le backend n'en rend alors qu'un.
    const single = worktrees.length === 1 ? worktrees[0] : undefined;
    const suffixes = suffixesOf(
        worktrees.map((worktree) => worktree.name),
        single === undefined,
    );

    const pinned = new Set(options.pinned.map((entry) => entry.worktreeRoot));
    const nodes = worktrees.map((worktree, index) =>
        node(worktree, suffixes[index] ?? null, options.collapsed, pinned),
    );
    const state = bubbleState(nodes.map((worktree) => worktree.state));

    const only = nodes[0];
    if (single !== undefined && only !== undefined) {
        // La ligne unique porte la clé du **worktree** pour se replier et s'épingler, et le
        // nom du **dépôt** pour se lire : c'est toute la mise à plat.
        return {
            kind: "flat",
            row: flatRow(group.repo, single.name, only),
            worktree: only,
            state,
        };
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

/**
 * Ce que la ligne unique d'un groupe à plat écrit.
 *
 * Deux cas, et ils ne montrent pas la même chose — c'est pourquoi la variante `flat` porte
 * ce texte au lieu de le laisser déduire au rendu :
 *
 * - **hors dépôt** : il n'y a pas de nom de dépôt à montrer, la ligne garde celui du
 *   dossier ;
 * - **un dépôt, un worktree** : la ligne porte le nom du dépôt, et le suffixe seulement
 *   s'il ajoute quelque chose — c'est la règle de [`suffixesOf`], appliquée à une ligne qui
 *   écrit déjà un autre nom que le sien. `ash ·ash` ne dirait rien de plus que `ash`, alors
 *   que le `·backoffice` de `democratic-backoffice` dit dans quel dossier on est.
 */
function flatRow(repo: RepoRef | null, name: string, only: WorktreeNode): RowLabel {
    if (repo === null) {
        return { label: only.label, title: only.title, suffix: null };
    }
    return {
        label: truncate(repo.name),
        title: repo.name,
        suffix: repeatsRepoName(name, repo) ? null : `·${shortSuffix(name)}`,
    };
}

/**
 * Le dossier du worktree porte-t-il déjà le nom que la ligne écrit ?
 *
 * C'est **tout** ce que la question demande, et c'est pour cela qu'elle est nommée ainsi
 * plutôt que « est-ce l'arbre principal ». Le cas courant est celui de l'arbre principal —
 * il vit dans le dossier du dépôt, donc les deux noms coïncident —, mais un worktree lié
 * qu'on aurait posé dans un dossier `ash` répondrait oui lui aussi, et le rendu resterait
 * juste : un suffixe qui répète le libellé ne distingue rien, exactement comme deux
 * `·sidebar` dans [`suffixesOf`].
 *
 * Le fait « arbre principal », lui, existe côté Rust (`features/git/table.rs`, où il
 * compare deux `git_dir`) mais **ne traverse pas** la frontière : `TabLocation` ne porte ni
 * `is_main` ni le `git_dir` du worktree. La colonne ne le redérive donc pas depuis
 * `repo.id` — elle rendrait un fait que le backend détient
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — et se contente de la
 * question d'affichage qu'elle a réellement à trancher.
 */
function repeatsRepoName(worktreeName: string, repo: RepoRef): boolean {
    return worktreeName === repo.name;
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
 * Les suffixes d'un groupe, ou aucun quand une seule ligne le remplit.
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
