import {
    badge,
    banner,
    button,
    column,
    ElementBuilder,
    emptyState,
    row,
    SVG_NAMESPACE,
    text,
    type UiComponent,
} from "@/shared/ui";

import type { CommitGraph, CommitRow, GraphLink } from "./contract";

/**
 * Le graphe de commits, **comme une valeur** (spec §7.2, design 4c).
 *
 * Tout ce fichier est pur : un état entre, une description d'interface sort. C'est le motif
 * du dépôt — ce qui décide vit dans des fonctions testables, ce qui touche le DOM ne décide
 * rien — et il compte doublement ici, parce que `bun test` ne monte pas de DOM et qu'un
 * graphe posé impérativement serait le seul écran du produit que rien ne pourrait relire.
 *
 * **Rien n'est décidé ici non plus.** Les couloirs viennent de Rust (critère d'acceptation de
 * l'issue #27), la colonne `by` aussi, et la phrase qui remplace un prompt absent également.
 * Ce fichier place des pixels ; il ne sait ni ce qu'est un agent, ni ce qu'est un rebase.
 *
 * # Ce que `shared/agent-state` n'apporte pas ici
 *
 * La colonne `by` nomme un **agent**, mais elle ne montre aucun des cinq **états** : un
 * commit est un fait passé, il n'est ni `working` ni `waiting`. Emprunter un glyphe d'état
 * pour décorer un nom d'auteur donnerait à lire un état qui n'existe pas. La seule chose que
 * cette colonne distingue est « observé » de « nom d'auteur git », et c'est `attributed` qui
 * le dit — un attribut, pas un glyphe.
 */

/** La hauteur d'une ligne, en pixels. **`graph.css` porte la même valeur.** */
export const ROW_HEIGHT = 22;

/** L'écart entre deux couloirs, en pixels. */
export const LANE_STEP = 14;

/** De combien la fenêtre grandit quand on demande à voir plus loin. */
export const WINDOW_STEP = 200;

/** Ce que la vue sait faire faire, et qu'elle ne fait pas elle-même. */
export interface CommitGraphActions {
    /** Une ligne est choisie — ou la même reprise, ce qui referme le détail. */
    select(sha: string): void;
    /** Voir plus loin : la fenêtre est redemandée plus grande. */
    widen(window: number): void;
}

/** Ce que la vue rend. */
export interface CommitGraphState {
    /**
     * La fenêtre lue, ou `null` — pas encore lue, ou répertoire hors de tout dépôt. Les deux
     * se rendent pareil : il n'y a rien à montrer, et rien ne s'est cassé.
     */
    readonly graph: CommitGraph | null;
    /** Le `sha` dont le détail est ouvert. */
    readonly selected: string | null;
}

export function commitGraphView(state: CommitGraphState, actions: CommitGraphActions): UiComponent {
    const graph = state.graph;
    if (graph === null || graph.rows.length === 0) {
        return column(
            emptyState("no history here").prose(
                "this directory is not inside a git repository, or it has no commit yet.",
            ),
        ).class("git-graph", "is-empty");
    }

    const selected = graph.rows.find((commit) => commit.sha === state.selected) ?? null;

    return column(
        column(...graph.rows.map((commit) => commitLine(commit, graph.lanes, state, actions))).class(
            "git-graph-rows",
        ),
        ...foldedNotice(graph),
        ...moreButton(graph, actions),
        ...(selected === null ? [] : [commitDetail(selected)]),
    ).class("git-graph");
}

/**
 * Une ligne : son dessin, son identifiant, son sujet, **sa colonne `by`**, sa date.
 *
 * L'ordre des colonnes est celui du design 4c, et `by` est avant la date parce que c'est
 * l'information que cet écran apporte et qu'aucun autre client git n'a.
 */
function commitLine(
    commit: CommitRow,
    lanes: number,
    state: CommitGraphState,
    actions: CommitGraphActions,
): UiComponent {
    const chosen = commit.sha === state.selected;
    return row(
        laneDrawing(commit, lanes),
        span("git-graph-sha", commit.short),
        row(...commit.refs.map((name) => badge(name).class("git-graph-ref")), span("git-graph-subject", commit.subject)).class(
            "git-graph-title",
        ),
        // La colonne `by` : le nom, et **d'où il vient**. `attributed` est porté par un
        // attribut plutôt que par un mot, parce que la colonne doit rester lisible d'un
        // coup d'œil — mais l'infobulle, elle, le dit en toutes lettres.
        span("git-graph-by", commit.by)
            .attr("data-attributed", commit.attributed ? "agent" : "git")
            .title(
                commit.attributed
                    ? `ash saw ${commit.by} write this commit`
                    : `git author — ${commit.author}`,
            ),
        span("git-graph-day", day(commit.authorDate)),
    )
        .class("git-graph-row", ...(chosen ? ["is-selected"] : []))
        .attr("data-sha", commit.sha)
        .attr("role", "button")
        .attr("tabindex", "-1")
        .on("click", () => {
            actions.select(commit.sha);
        });
}

/**
 * Le dessin d'une ligne : son point, et les traits qui en descendent.
 *
 * Le `<svg>` fait la hauteur d'**une** ligne mais déborde d'une demi-ligne vers le bas —
 * c'est ce qui laisse chaque ligne se peindre seule, sans jamais regarder sa voisine, tout en
 * rejoignant le point d'en dessous. `graph.css` laisse ce débordement visible.
 */
export function laneDrawing(commit: CommitRow, lanes: number): UiComponent {
    const middle = ROW_HEIGHT / 2;
    return new Svg("git-graph-lanes", Math.max(lanes, 1) * LANE_STEP, ROW_HEIGHT).add(
        ...commit.links.map((link) => new Path(linkShape(link, middle)).class("git-graph-link")),
        new Dot(center(commit.lane), middle).class(
            commit.attributed ? "git-graph-dot is-attributed" : "git-graph-dot",
        ),
    );
}

/**
 * Le tracé d'un trait, d'une ligne vers la suivante.
 *
 * Droit quand la colonne ne change pas — le cas de l'immense majorité des traits —, une
 * courbe sinon : deux tangentes verticales, donc un raccord qui ne fait pas d'angle au point
 * de départ ni à l'arrivée. Une diagonale nue donnerait un « V » à chaque fusion.
 */
export function linkShape(link: GraphLink, middle: number): string {
    const from = center(link.from);
    const to = center(link.to);
    const bottom = middle + ROW_HEIGHT;
    if (from === to) return `M${from} ${middle}V${bottom}`;
    return `M${from} ${middle}C${from} ${middle + ROW_HEIGHT / 2} ${to} ${middle + ROW_HEIGHT / 2} ${to} ${bottom}`;
}

/** Le milieu d'un couloir, en pixels. */
function center(lane: number): number {
    return lane * LANE_STEP + LANE_STEP / 2;
}

/**
 * Ce que le repli des branches inactives dit de lui-même (spec §7.2).
 *
 * Replier sans le dire ferait croire à une histoire perdue. La bannière nomme les branches :
 * c'est pour ça que le backend rend leur nom plutôt qu'un compte.
 */
function foldedNotice(graph: CommitGraph): readonly UiComponent[] {
    if (graph.folded.length === 0) return [];
    const names = graph.folded.map((branch) => branch.name).join(", ");
    return [
        banner(
            `${graph.folded.length} branch${graph.folded.length === 1 ? "" : "es"} untouched for 30 days are folded: ${names}`,
        ).class("git-graph-folded"),
    ];
}

function moreButton(graph: CommitGraph, actions: CommitGraphActions): readonly UiComponent[] {
    if (!graph.hasMore) return [];
    return [
        button("show more")
            .class("git-graph-more")
            .onClick(() => {
                actions.widen(graph.window + WINDOW_STEP);
            }),
    ];
}

/**
 * Le panneau de détail : ce que le graphe garde d'un commit, **et le prompt qui l'a produit**
 * ([ADR-0014](../../../docs/adr/0014-attribution-locale-des-commits.md)).
 *
 * Quand il n'y a pas de prompt, il le dit — avec la phrase que le backend a composée, qui
 * distingue « Ash a vu cet agent écrire mais n'a pas gardé la question » de « aucun agent n'a
 * été observé ». **Rien n'est fabriqué à la place**, et le prompt n'est pas cherché ailleurs.
 */
export function commitDetail(commit: CommitRow): UiComponent {
    return column(
        span("git-graph-detail-subject", commit.subject),
        row(
            badge(commit.short).class("git-graph-detail-sha"),
            span("git-graph-detail-meta", `${commit.author} · ${commit.authorDate}`),
        ).class("git-graph-detail-head"),
        span(
            "git-graph-detail-by",
            commit.attributed
                ? `written by ${commit.by}${commit.tabId === null ? "" : `, in tab ${commit.tabId}`}`
                : `no agent attribution — git author is ${commit.author}`,
        ),
        commit.prompt === null
            ? span("git-graph-detail-note", commit.promptNote)
            : new Pre("git-graph-detail-prompt", commit.prompt),
    ).class("git-graph-detail");
}

/** La partie « jour » d'une date ISO, ou rien si ce n'en est pas une. */
export function day(authorDate: string): string {
    return /^\d{4}-\d{2}-\d{2}/.test(authorDate) ? authorDate.slice(0, 10) : "";
}

class Span extends ElementBuilder {
    constructor(className: string, content: string) {
        super("span", className);
        this.add(text(content));
    }
}

function span(className: string, content: string): Span {
    return new Span(className, content);
}

/** Le prompt tel qu'il a été tapé : ses retours à la ligne sont de l'information. */
class Pre extends ElementBuilder {
    constructor(className: string, content: string) {
        super("pre", className);
        this.add(text(content));
    }
}

/**
 * Un `<svg>`, dans l'espace de noms qui le rend visible.
 *
 * Un dessin et pas des caractères : les traits d'un graphe suivent des colonnes de 14 px, et
 * aucune police monospace ne garantit qu'un `│` s'aligne avec le `●` de la ligne du dessus.
 */
class Svg extends ElementBuilder {
    constructor(className: string, width: number, height: number) {
        super("svg", className);
        this.inNamespace(SVG_NAMESPACE)
            .attr("width", String(width))
            .attr("height", String(height))
            .attr("viewBox", `0 0 ${width} ${height}`)
            .attr("fill", "none")
            // Le dessin double la colonne `by` et le sujet ; il n'apporte rien à qui écoute.
            .attr("aria-hidden", "true")
            .attr("focusable", "false");
    }
}

class Path extends ElementBuilder {
    constructor(shape: string) {
        super("path");
        this.inNamespace(SVG_NAMESPACE)
            .attr("d", shape)
            .attr("stroke", "currentColor")
            .attr("stroke-width", "1.25");
    }
}

class Dot extends ElementBuilder {
    constructor(x: number, y: number) {
        super("circle");
        this.inNamespace(SVG_NAMESPACE)
            .attr("cx", String(x))
            .attr("cy", String(y))
            .attr("r", "3")
            .attr("fill", "currentColor");
    }
}
