import { isShell } from "@/shared/ipc";
import type { AgentState, GitOperation, GitStatus, ShellTab, WorktreeMetadata } from "@/shared/ipc";
import {
    agentGlyph,
    elapsedSince as sinceEntering,
    presentAgentState,
} from "@/shared/agent-state";
import { branchOf, locationLabel } from "@/shared/tab-context";
import { activeTab, type TabsState } from "./tabs";

/**
 * La ligne de statut de la zone terminal (spec §4.2) : `cwd` · branche et état de l'arbre ·
 * état de l'agent.
 *
 * Elle parle de l'onglet **actif**, et de son worktree — jamais de son dépôt : deux
 * worktrees du même projet peuvent être sur deux branches et dans deux états d'arbre
 * différents ([ADR-0012](../../../docs/adr/0012-worktree-unite-de-travail.md)).
 *
 * La composition est une fonction pure, séparée du rendu : c'est là que vivent les quatre
 * décisions que la maquette ne dessine pas — le `HEAD` détaché, l'opération en cours, le
 * `git status` absent, et l'onglet hors dépôt. Aucune ne se vérifierait dans le DOM.
 *
 * Rien n'est produit ici : le `cwd` vient de la sonde d'ADR-0005, l'état git de la
 * surveillance d'ADR-0011, l'état d'agent du backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * La seule chose qui se **calcule** ici est la durée de l'état courant — le `15m22s` de la
 * maquette. Elle n'est pas une exception à la règle : le backend envoie la **date d'entrée**
 * dans l'état, une fois, en absolu, et l'écart jusqu'à maintenant est un fait d'affichage.
 * C'est ce qui laisse la fiche d'un onglet identique d'une passe de sonde à l'autre.
 */

/**
 * La couleur d'un morceau de ligne, nommée par ce qu'elle **dit**, pas par sa teinte : le
 * même modèle se peint dans les deux palettes.
 */
export type StatusTone =
    /** Le `cwd`, plus clair que le reste de la ligne. */
    | "path"
    /** La couleur de fond de la ligne. */
    | "text"
    /** Ce qui doit se lire avant le reste — une opération git en cours. */
    | "strong"
    /** Ce qui n'est qu'un rappel, ou une absence. */
    | "faint"
    /** `+3` : des fichiers en plus. */
    | "added"
    /** `~1`, `-2` : des fichiers touchés. */
    | "changed"
    /** Ce qui demande une décision : un conflit, un agent qui attend. */
    | "accent";

/**
 * Ce qu'un morceau de ligne **ouvre**, quand il ouvre quelque chose.
 *
 * Une seule valeur aujourd'hui, et c'est déjà une union : le pied de fenêtre n'est pas un
 * endroit où l'on pose des gestes au fil de l'eau, et un `boolean` ne dirait pas *lequel*.
 */
export type StatusAction = "open-branches";

/** Un morceau de ligne : un mot, sa couleur, et de quoi l'expliquer au survol. */
export interface StatusChip {
    readonly text: string;
    readonly tone: StatusTone;
    /** L'infobulle, quand le mot est plus court que ce qu'il veut dire. */
    readonly title: string | null;
    /**
     * Ce que ce morceau ouvre — absent pour tous sauf un.
     *
     * La branche est l'**ancre** de la popup de branches (spec §7.1), et c'est ce qui la
     * rend atteignable à la souris autant qu'au clavier (spec §4.4). Le champ est optionnel
     * plutôt que `| null` parce que dix morceaux sur onze n'ouvrent rien : les faire tous
     * déclarer une absence serait du bruit dans chaque littéral.
     */
    readonly action?: StatusAction;
}

/** L'état d'agent de l'onglet actif — le troisième segment de la ligne. */
export interface StatusAgent {
    /** `null` quand il n'y a pas d'onglet : le glyphe n'a alors rien à montrer. */
    readonly state: AgentState | null;
    readonly text: string;
    readonly tone: StatusTone;
}

/** Les trois segments de la maquette, plus le rappel poussé à droite. */
export interface StatusLineModel {
    readonly cwd: StatusChip;
    readonly git: readonly StatusChip[];
    readonly agent: StatusAgent;
    /** Le rappel de la sidebar repliée. `null` quand il n'y a rien à rappeler. */
    readonly hint: StatusChip | null;
}

/**
 * Au-delà, le `cwd` est coupé **par la gauche** : c'est la fin d'un chemin qui dit où l'on
 * est, jamais son début.
 */
export const MAX_CWD = 38;

/** Le nombre d'onglets que `⌘1…9` adresse directement (spec §4.4). */
const DIRECT_TABS = 9;

/**
 * Compose la ligne à partir de l'onglet actif et de l'état git de son worktree.
 *
 * `metadata` à `null` veut dire « ce worktree n'est pas dans un dépôt, ou le backend n'a
 * pas su le lire » : les deux se rendent `no repo`, parce que dans les deux cas il n'y a
 * pas de branche à montrer. Un onglet hors dépôt est un cas **nominal**, pas une panne.
 */
export function composeStatusLine(
    state: TabsState,
    metadata: WorktreeMetadata | null,
    sidebarCollapsed: boolean,
    now: number,
): StatusLineModel {
    const tab = activeTab(state);

    // L'état vide (bloc `1d`) : pas d'onglet, donc pas de répertoire, pas de dépôt et pas
    // d'agent. Toute la ligne passe en `faint` — il n'y a rien à hiérarchiser.
    if (tab === null) {
        return {
            cwd: { text: "~", tone: "faint", title: null },
            git: [{ text: "no repo", tone: "faint", title: null }],
            agent: { state: null, text: "no agents", tone: "faint" },
            hint: null,
        };
    }

    // Une surface d'outil n'a **ni processus, ni état, ni durée** : le segment de droite
    // dit ce que l'onglet est, et rien de plus. Y afficher `idle · 12m` serait chronométrer
    // un état qui n'existe pas — c'est précisément ce que le typage des onglets évite.
    if (!isShell(tab)) {
        return {
            cwd: { text: elide(tab.worktreeRoot), tone: "path", title: tab.worktreeRoot },
            git: gitSegment(metadata),
            agent: { state: null, text: tab.title, tone: "text" },
            hint: hint(state, sidebarCollapsed),
        };
    }

    const shown = presentAgentState(tab.state);
    const elapsed = elapsedSince(tab, now);
    return {
        cwd: { text: elide(tab.cwd), tone: "path", title: tab.cwd },
        git: gitSegment(metadata),
        agent: {
            state: tab.state,
            text: `${tab.process} · ${shown.label}${elapsed === null ? "" : ` · ${elapsed}`}`,
            tone: shown.tinted ? "accent" : "text",
        },
        hint: hint(state, sidebarCollapsed),
    };
}

/**
 * Depuis combien de temps l'onglet est dans son état — le `working · 15m22s` de la maquette.
 *
 * Calculé **à l'affichage**, à partir de la date d'entrée que le backend a envoyée une seule
 * fois : c'est ce qui garde `TabInfo` identique d'une passe de sonde à l'autre. Le frontend
 * n'invente rien pour autant — il ne décide ni de l'état, ni de son origine
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), seulement de la façon de
 * lire l'écart jusqu'à maintenant.
 *
 * La seule règle qui reste ici est celle que cette ligne est seule à porter : **rien sur un
 * onglet `idle`**. Un shell à son invite n'a pas d'activité à chronométrer, et un compteur
 * qui tournerait sur les onglets vides ferait du bruit là où il n'y a rien à lire. La mise en
 * forme, elle, est partagée avec les lignes de sous-agents de la sidebar
 * ([`@/shared/agent-state`]).
 */
function elapsedSince(tab: ShellTab, now: number): string | null {
    return tab.state === "idle" ? null : sinceEntering(tab.stateSince, now);
}

/**
 * La branche, l'opération en cours, et l'état de l'arbre.
 *
 * Trois cas que la maquette ne dessine pas, et qui sont tranchés ici :
 *
 * - **`HEAD` détaché** — la maquette ne montre qu'une branche. Le mot à écrire vient de
 *   `shared/tab-context`, qui le rend `@a1b2c3d` — court, et impossible à confondre avec un
 *   nom de branche. Ce qui est tranché **ici**, c'est que « detached » va dans l'infobulle et
 *   pas dans une ligne de 25 px.
 * - **Une opération en cours** — elle prend un morceau à elle, après la branche, dans la
 *   couleur la plus forte de la ligne. Elle ne remplace pas la branche : pendant un rebase
 *   `HEAD` est détaché, et `shared/tab-context` sait que c'est `operation.branch` qui dit
 *   encore où l'on travaille. La place, elle, vient du `flex: 1` qui poussait le rappel à
 *   droite. L'accent n'est **pas** utilisé : il reste au seul état qui attend l'utilisateur.
 * - **`status` à `null`** — `git` absent, lent ou en échec. Ce n'est pas un arbre propre :
 *   on écrit `+? ~?`, qui garde la grammaire des compteurs et se lit comme l'absence
 *   qu'il est. Un arbre réellement propre, lui, n'affiche rien après sa branche.
 */
function gitSegment(metadata: WorktreeMetadata | null): readonly StatusChip[] {
    if (metadata === null) return [{ text: "no repo", tone: "faint", title: null }];

    const operation = metadata.operation;

    // La branche est nommée par `shared/tab-context`, comme dans la bande de titre : c'est la
    // même phrase à deux endroits de la fenêtre, et deux lectures finiraient par désigner deux
    // branches. Ce qui reste d'ici, c'est l'infobulle — un détachement mérite une phrase, et
    // une bande de titre n'a pas d'infobulle.
    const branch = branchOf(metadata);

    const chips: StatusChip[] = [
        {
            text: branch.label,
            tone: "text",
            title: branch.detachedAt === null ? null : `detached HEAD at ${branch.detachedAt}`,
            // Le seul morceau de la ligne qui ouvre quelque chose : la popup de branches est
            // ancrée ici (spec §7.1), et un `HEAD` détaché en est une ancre aussi valable —
            // on peut vouloir en sortir.
            action: "open-branches",
        },
    ];

    if (operation !== null) {
        chips.push({ text: operationLabel(operation), tone: "strong", title: null });
    }

    chips.push(...counts(metadata.status));
    return chips;
}

/** `rebasing onto main · 2/5`, `applying · 1/3`, `merging feat`. */
function operationLabel(operation: GitOperation): string {
    const verb = { rebase: "rebasing", am: "applying", merge: "merging" }[operation.kind];
    // Un merge n'a pas de cible : il ramène une branche **dans** celle où l'on est. Dire
    // « merging onto feat » inverserait le sens de l'opération.
    const target =
        operation.onto === null
            ? ""
            : operation.kind === "merge"
              ? ` ${operation.onto}`
              : ` onto ${operation.onto}`;
    const progress =
        operation.progress === null
            ? ""
            : ` · ${operation.progress.step}/${operation.progress.total}`;
    return `${verb}${target}${progress}`;
}

/**
 * `+3 ~1 -2 !1 ↑2 ↓1` — et rien du tout quand tout est à zéro.
 *
 * Les conflits prennent l'accent : ce sont la seule partie de l'état d'un arbre qui
 * **demande** une décision, comme un agent qui attend.
 */
function counts(status: GitStatus | null): StatusChip[] {
    if (status === null) {
        return [
            {
                text: "+? ~?",
                tone: "faint",
                title: "git status unavailable for this worktree",
            },
        ];
    }

    const chips: StatusChip[] = [];
    const push = (count: number, mark: string, tone: StatusTone, title: string): void => {
        if (count > 0) chips.push({ text: `${mark}${count}`, tone, title });
    };

    push(status.tree.added, "+", "added", "added or untracked files");
    push(status.tree.modified, "~", "changed", "modified files");
    push(status.tree.deleted, "-", "changed", "deleted files");
    push(status.tree.conflicted, "!", "accent", "conflicted files");

    if (status.upstream !== null) {
        push(status.upstream.ahead, "↑", "text", "commits ahead of the upstream branch");
        push(status.upstream.behind, "↓", "text", "commits behind the upstream branch");
    }

    return chips;
}

/**
 * Le rappel poussé à droite par le `flex: 1`.
 *
 * Replié, le rail de 46 px ne nomme plus les agents : la ligne de statut reprend celui qui
 * attend, avec son raccourci — c'est ce qui rend `⌘B` supportable (bloc `1b`).
 *
 * **Le reste du temps, il n'y a rien à rappeler.** La maquette y mettait `⌘K commands`, mais
 * la palette de commandes n'existe pas, et `⌘K` appartient désormais au shell : Ash ne le
 * pose sur aucune entrée de menu, précisément pour que le terminal le reçoive (#132). Une
 * ligne de statut qui annonce une touche ne faisant rien coûte plus qu'un coin vide.
 */
function hint(state: TabsState, sidebarCollapsed: boolean): StatusChip | null {
    const waiting = state.tabs.filter((tab) => isShell(tab) && tab.state === "waiting");
    const first = waiting[0];

    if (!sidebarCollapsed || first === undefined) {
        return null;
    }

    const position = state.tabs.indexOf(first) + 1;
    const shortcut = position <= DIRECT_TABS ? ` ⌘${position}` : "";
    return {
        // `omelette-web/claude` : le dépôt, puis le programme qui tient l'avant-plan. C'est
        // ce que le libellé d'onglet portait quand la barre existait, et la colonne repliée
        // est justement le moment où le contexte manque.
        text: `${waiting.length} waiting · ${locationLabel(first)}/${isShell(first) ? first.process : first.title}${shortcut}`,
        tone: "accent",
        title: null,
    };
}

/** Coupe un chemin par la gauche : `…/dev/omelette-web/src`. */
export function elide(path: string, max: number = MAX_CWD): string {
    return path.length <= max ? path : `…${path.slice(path.length - max + 1)}`;
}

/**
 * Le rendu de la ligne. Il ne décide rien : il pose le modèle dans le DOM, comme la barre
 * d'onglets pose son `TabsState`.
 */
export class StatusLine {
    readonly element: HTMLElement;
    /**
     * Le morceau qui porte la branche, une fois peint — l'ancre de la popup.
     *
     * `null` quand la ligne ne montre pas de branche : hors dépôt, ou avant le premier
     * rendu. Celui qui l'ouvre décide alors où la poser ; ce n'est pas à la ligne de statut
     * de le savoir.
     */
    private anchorElement: HTMLElement | null = null;

    /**
     * `onAction` est un rappel et non un event : la ligne de statut ne connaît pas la popup
     * de branches, et elle n'a aucune raison de la connaître. C'est le composition root qui
     * relie les deux, comme il relie déjà la sidebar aux onglets.
     */
    constructor(private readonly onAction: (action: StatusAction) => void = () => undefined) {
        this.element = document.createElement("div");
        this.element.className = "terminal-status";
        this.element.setAttribute("role", "status");
    }

    /** Sur quoi la popup de branches doit s'ancrer, si la ligne montre une branche. */
    get anchor(): HTMLElement | null {
        return this.anchorElement;
    }

    render(model: StatusLineModel): void {
        this.anchorElement = null;
        const paint = (piece: StatusChip): HTMLElement => {
            if (piece.action === undefined) return chip(piece);
            const opener = actionChip(piece, piece.action, this.onAction);
            this.anchorElement = opener;
            return opener;
        };

        const nodes: Node[] = [chip(model.cwd), rule(), ...joinChips(model.git, paint), rule()];

        if (model.agent.state !== null) nodes.push(agentGlyph(model.agent.state));
        nodes.push(
            chip({ text: model.agent.text, tone: model.agent.tone, title: null }),
            spacer(),
        );
        if (model.hint !== null) nodes.push(chip(model.hint));

        this.element.replaceChildren(...nodes);
    }
}

/** Les morceaux d'un même segment sont séparés d'une espace, pas d'un `│`. */
function joinChips(
    chips: readonly StatusChip[],
    paint: (piece: StatusChip) => HTMLElement,
): Node[] {
    return chips.flatMap((piece, index) =>
        index === 0 ? [paint(piece)] : [document.createTextNode(" "), paint(piece)],
    );
}

/**
 * Un morceau qui ouvre quelque chose : un vrai `<button>`, pas un `<span>` cliquable.
 *
 * C'est ce qui le met sur le chemin de `tab` et dans l'arbre d'accessibilité sans une ligne
 * de code — la même raison que le socle de composants donne pour ses boutons. Le raccourci
 * est annoncé par `aria-keyshortcuts` plutôt qu'écrit dans le libellé : la ligne de statut
 * fait 25 px, et le mot qu'on doit y lire est le nom de la branche.
 */
function actionChip(
    piece: StatusChip,
    action: StatusAction,
    onAction: (action: StatusAction) => void,
): HTMLElement {
    const element = document.createElement("button");
    element.type = "button";
    element.className = `status-${piece.tone} status-branch-anchor`;
    element.textContent = piece.text;
    element.title = piece.title ?? "branch actions";
    element.setAttribute("aria-keyshortcuts", "Meta+Control+B");
    element.addEventListener("click", () => {
        onAction(action);
    });
    return element;
}

function chip(piece: StatusChip): HTMLElement {
    const element = document.createElement("span");
    element.className = `status-${piece.tone}`;
    element.textContent = piece.text;
    if (piece.title !== null) element.title = piece.title;
    return element;
}

function rule(): HTMLElement {
    const element = document.createElement("span");
    element.className = "status-rule";
    element.textContent = "│";
    element.setAttribute("aria-hidden", "true");
    return element;
}

function spacer(): HTMLElement {
    const element = document.createElement("span");
    element.className = "status-spacer";
    return element;
}
