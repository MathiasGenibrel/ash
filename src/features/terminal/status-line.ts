import type {
    AgentState,
    GitHead,
    GitOperation,
    GitStatus,
    TabInfo,
    WorktreeMetadata,
} from "@/shared/ipc";
import { agentGlyph, presentAgentState } from "@/shared/agent-state";
import { tabTitle } from "./tab-bar";
import type { TabsState } from "./tabs";

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

/** Un morceau de ligne : un mot, sa couleur, et de quoi l'expliquer au survol. */
export interface StatusChip {
    readonly text: string;
    readonly tone: StatusTone;
    /** L'infobulle, quand le mot est plus court que ce qu'il veut dire. */
    readonly title: string | null;
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
    /** `⌘K commands`, ou le rappel de la sidebar repliée. `null` à vide. */
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
    const tab = state.tabs.find((candidate) => candidate.tabId === state.activeTabId) ?? null;

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
 * Calculé **ici**, à chaque rendu, à partir de la date d'entrée que le backend a envoyée
 * une seule fois : c'est ce qui garde `TabInfo` identique d'une passe de sonde à l'autre.
 * Le frontend n'invente rien pour autant — il ne décide ni de l'état, ni de son origine
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), seulement de la façon de
 * lire l'écart jusqu'à maintenant.
 *
 * `null` sur un onglet `idle` : un shell à son invite n'a pas d'activité à chronométrer, et
 * un compteur qui tournerait sur les onglets vides ferait du bruit là où il n'y a rien à
 * lire. `null` aussi sur une date à venir — une horloge recalée entre le backend et le
 * rendu — parce qu'écrire `-3s` serait pire que ne rien écrire.
 */
function elapsedSince(tab: TabInfo, now: number): string | null {
    if (tab.state === "idle") return null;
    const elapsed = now - tab.stateSince;
    return elapsed < 0 ? null : formatElapsed(elapsed);
}

/**
 * `45s`, `15m22s`, `2h05m` — au plus deux unités, et jamais plus de sept caractères.
 *
 * La ligne de statut fait 25 px de haut et partage sa largeur avec un chemin et un état
 * git : la seconde disparaît au-delà de l'heure, où elle n'apprend plus rien.
 */
export function formatElapsed(millis: number): string {
    const seconds = Math.floor(millis / 1000);
    if (seconds < 60) return `${seconds}s`;

    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m${pad(seconds % 60)}s`;

    return `${Math.floor(minutes / 60)}h${pad(minutes % 60)}m`;
}

function pad(value: number): string {
    return value.toString().padStart(2, "0");
}

/**
 * La branche, l'opération en cours, et l'état de l'arbre.
 *
 * Trois cas que la maquette ne dessine pas, et qui sont tranchés ici :
 *
 * - **`HEAD` détaché** — la maquette ne montre qu'une branche. On écrit `@a1b2c3d` :
 *   court, et impossible à confondre avec un nom de branche. Le mot « detached » est dans
 *   l'infobulle, pas dans une ligne de 25 px.
 * - **Une opération en cours** — elle prend un morceau à elle, après la branche, dans la
 *   couleur la plus forte de la ligne. Elle ne remplace pas la branche : pendant un rebase
 *   `HEAD` est détaché, et c'est `head-name` — donc `operation.branch` — qui dit encore où
 *   l'on travaille. La place, elle, vient du `flex: 1` qui poussait le rappel à droite.
 *   L'accent n'est **pas** utilisé : il reste au seul état qui attend l'utilisateur.
 * - **`status` à `null`** — `git` absent, lent ou en échec. Ce n'est pas un arbre propre :
 *   on écrit `+? ~?`, qui garde la grammaire des compteurs et se lit comme l'absence
 *   qu'il est. Un arbre réellement propre, lui, n'affiche rien après sa branche.
 */
function gitSegment(metadata: WorktreeMetadata | null): readonly StatusChip[] {
    if (metadata === null) return [{ text: "no repo", tone: "faint", title: null }];

    const operation = metadata.operation;
    const where = operation?.branch ?? null;

    const chips: StatusChip[] = [
        where !== null
            ? { text: where, tone: "text", title: null }
            : headChip(metadata.head),
    ];

    if (operation !== null) {
        chips.push({ text: operationLabel(operation), tone: "strong", title: null });
    }

    chips.push(...counts(metadata.status));
    return chips;
}

function headChip(head: GitHead): StatusChip {
    return head.kind === "branch"
        ? { text: head.name, tone: "text", title: null }
        : { text: `@${head.commit}`, tone: "text", title: `detached HEAD at ${head.commit}` };
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
 * attend, avec son raccourci — c'est ce qui rend `⌘B` supportable (bloc `1b`). Sans agent
 * en attente, la maquette y remet le rappel de la palette de commandes.
 */
function hint(state: TabsState, sidebarCollapsed: boolean): StatusChip {
    const waiting = state.tabs.filter((tab) => tab.state === "waiting");
    const first = waiting[0];

    if (!sidebarCollapsed || first === undefined) {
        // `⌘K` est déjà « effacer le scrollback » (spec §4.4) : le rappel est celui de la
        // maquette, la palette de commandes n'existe pas encore, et la collision se
        // tranchera avec elle.
        return { text: "⌘K commands", tone: "faint", title: null };
    }

    const position = state.tabs.indexOf(first) + 1;
    const shortcut = position <= DIRECT_TABS ? ` ⌘${position}` : "";
    return {
        text: `${waiting.length} waiting · ${tabTitle(first, true)}${shortcut}`,
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

    constructor() {
        this.element = document.createElement("div");
        this.element.className = "terminal-status";
        this.element.setAttribute("role", "status");
    }

    render(model: StatusLineModel): void {
        const nodes: Node[] = [chip(model.cwd), rule(), ...joinChips(model.git), rule()];

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
function joinChips(chips: readonly StatusChip[]): Node[] {
    return chips.flatMap((piece, index) =>
        index === 0 ? [chip(piece)] : [document.createTextNode(" "), chip(piece)],
    );
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
