/**
 * Le tableau des worktrees, **comme une valeur** (spec §7.3).
 *
 * Rien ici ne touche au DOM et rien ici ne décide d'un état : les lignes arrivent composées
 * du backend ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et ce fichier
 * ne fait que les mettre en mots. C'est le motif de `shared/ui/` — un composant est une
 * valeur —, et c'est ce qui permet de lire ce tableau dans un test sans monter de DOM.
 *
 * Les deux colonnes du milieu sont la raison d'être de l'écran : `agents now` et
 * `last worked by` sont celles que `git worktree list` ne donne pas. Ce fichier ne les
 * calcule pas, il les **rend** — et il rend aussi, littéralement, ce que le backend a choisi
 * de ne pas affirmer : une colonne `last worked by` vide veut dire « ash ne sait pas ».
 */

import {
    badge,
    button,
    column,
    emptyState,
    glyph,
    row,
    text,
    ElementBuilder,
    type UiChild,
    type UiComponent,
} from "@/shared/ui";
import { formatElapsed, presentAgentState } from "@/shared/agent-state";
import type { GitStatus, LastWork, TabId, WorktreeRemoval, WorktreeRow } from "@/shared/ipc";

/** Les six colonnes de la spec §7.3, dans son ordre. */
export const WORKTREE_COLUMNS = [
    "worktree",
    "branch",
    "agents now",
    "last worked by",
    "tree",
    "card",
] as const;

/** Ce que le tableau sait demander, et qu'il ne sait pas faire lui-même. */
export interface WorktreeTableActions {
    /**
     * Aller à l'onglet d'un agent — **sur un geste, jamais tout seul** (ADR-0010).
     */
    selectTab(tabId: TabId): void;
    /** Ouvrir un onglet dans un worktree que plus personne n'habite. */
    openTabIn(worktreeRoot: string): void;
    /** La fiche de branche de ce worktree — livrée par #31 ; ici, un renvoi. */
    showCard(row: WorktreeRow): void;
    /** Demander ce qu'une suppression emporterait. Elle ne supprime rien (spec §5.4). */
    askRemoval(worktreeRoot: string): void;
    /** Refermer la fiche de suppression ouverte. */
    dismissRemoval(): void;
}

/**
 * Le tableau entier.
 *
 * `now` est passée plutôt que lue : les durées affichées sont un fait d'affichage, et un
 * test qui décide de l'heure est un test qui ne casse pas la nuit du changement d'heure.
 */
export function worktreeTable(
    rows: readonly WorktreeRow[],
    now: number,
    showing: WorktreeRemoval | null,
    actions: WorktreeTableActions,
): UiComponent {
    if (rows.length === 0) {
        return column(
            emptyState("no worktree to show yet").prose(
                "ash lists the worktrees of every repository one of your tabs sits in — open a tab in a repository to see them.",
            ),
        ).class("git-worktrees");
    }

    return column(header(), ...rows.map((line) => worktreeLine(line, now, showing, actions))).class(
        "git-worktrees",
    );
}

function header(): UiChild {
    return row(...WORKTREE_COLUMNS.map((name) => cell(name, text(name)))).class(
        "git-worktrees-head",
    );
}

function worktreeLine(
    line: WorktreeRow,
    now: number,
    showing: WorktreeRemoval | null,
    actions: WorktreeTableActions,
): UiChild {
    const shown = showing !== null && showing.worktreeRoot === line.worktreeRoot ? showing : null;

    const built = column(
        row(
            cell("worktree", ...name(line)),
            cell("branch", text(branch(line))),
            cell("agents now", ...agentsNow(line, now, actions)),
            cell("last worked by", ...lastWorked(line.lastWorkedBy, now)),
            cell("tree", text(tree(line))),
            cell("card", ...card(line, actions)),
        ).class("git-worktrees-row"),
    ).class("git-worktree");

    if (shown !== null) built.add(removalNotice(shown, actions));
    if (line.stale) built.class("is-stale");
    if (line.awaitingReview) built.class("is-awaiting-review");
    return built;
}

/**
 * Le nom du worktree, et les deux mots qui le qualifient.
 *
 * `stale` porte sa phrase entière en infobulle : le mot seul se lirait comme un verdict, et
 * ce qu'il dit est une **observation** — sans agent depuis trois jours, avec du travail non
 * validé (spec §5.4). Ash le signale, il ne supprime jamais.
 */
function name(line: WorktreeRow): UiChild[] {
    const parts: UiChild[] = [
        text(line.worktreeName),
        ...(line.repo === null ? [] : [badge(line.repo.name).class("git-worktrees-repo")]),
    ];
    if (line.stale) {
        parts.push(
            badge("stale")
                .class("git-worktrees-stale")
                .title(
                    "no agent seen here for over three days, and it still holds uncommitted work",
                ),
        );
    }
    return parts;
}

/** La branche, ou l'opération en cours — qui l'emporte, comme dans la ligne de statut. */
function branch(line: WorktreeRow): string {
    const metadata = line.metadata;
    if (metadata === null) return "—";
    if (metadata.operation !== null) {
        const onto = metadata.operation.onto === null ? "" : ` onto ${metadata.operation.onto}`;
        return `${metadata.operation.kind}${onto}`;
    }
    return metadata.head.kind === "branch" ? metadata.head.name : `@${metadata.head.commit}`;
}

/**
 * `agents now` — la première colonne que `git worktree list` ne donne pas.
 *
 * Le glyphe et la classe viennent de `shared/agent-state`, la **même** source que la sidebar
 * et la ligne de statut : une quatrième présentation des cinq états serait une divergence que
 * rien n'attraperait. C'est le caractère qui est posé, et non le tracé de `working` — la
 * présentation dit elle-même que le caractère est le repli des contextes textuels.
 *
 * `done · waiting for your review` est la phrase que la spec §7.3 appelle l'état le plus utile
 * du tableau. Elle ne se calcule pas ici : `awaitingReview` est décidé par le backend, où la
 * notion de « personne n'a regardé » a déjà sa seule définition (spec §6.4).
 */
function agentsNow(line: WorktreeRow, now: number, actions: WorktreeTableActions): UiChild[] {
    if (line.agentsNow.length === 0) {
        return [text("—")];
    }

    return line.agentsNow.map((agent) => {
        const shown = presentAgentState(agent.state);
        const said =
            agent.state === "done" && line.awaitingReview
                ? "done · waiting for your review"
                : `${shown.label}${duration(agent.since, now)}`;
        // Le glyphe **avant** le nom, comme dans la sidebar, et hors du bouton : ce qui se
        // clique est la ligne de l'agent, ce qui se reconnaît au coin de l'œil est sa forme.
        return row(
            glyph(shown.glyph, shown.label).class(shown.className),
            button(`${agent.command} · ${said}`)
                .class("git-worktrees-agent")
                .title("go to this agent's tab")
                .onClick(() => {
                    actions.selectTab(agent.tabId);
                }),
        ).class("git-worktrees-agent-line");
    });
}

/**
 * `last worked by` — la seconde colonne que `git worktree list` ne donne pas.
 *
 * Vide **veut dire « ash ne sait pas »**, jamais « personne » : un agent qui a travaillé une
 * nuit sans rien valider et dont l'onglet est fermé n'a laissé aucune trace qu'Ash ait le
 * droit d'invoquer ([ADR-0014](../../../docs/adr/0014-attribution-locale-des-commits.md)).
 * Le tiret le dit, et l'infobulle l'explique — un blanc laisserait croire à une panne.
 */
function lastWorked(worked: LastWork | null, now: number): UiChild[] {
    if (worked === null) {
        return [
            span("git-worktrees-unknown", "—").title(
                "ash has not seen an agent work here — it does not mean nobody did",
            ),
        ];
    }
    const why =
        worked.source === "commit"
            ? "the last commit ash saw an agent write here"
            : "an agent ash sees in a tab here";
    return [span("git-worktrees-worked", `${worked.agent} · ${aged(worked.at, now)}`).title(why)];
}

/**
 * `+3 ~1 -2 !1 ↑2 ↓1`, ou rien du tout.
 *
 * Les mêmes signes que la ligne de statut, et pour cause : c'est le même état, lu du même
 * `git status`. La composition, elle, est écrite deux fois — celle de la ligne de statut est
 * interne à `features/terminal`, et une feature n'importe pas l'intérieur d'une autre. Le
 * jour où une troisième vue en aura besoin, c'est un module de `shared/` qu'il faudra, pas
 * une troisième copie.
 */
function tree(line: WorktreeRow): string {
    const status: GitStatus | null = line.metadata?.status ?? null;
    if (status === null) return "+? ~?";

    const marks: string[] = [];
    const push = (count: number, mark: string): void => {
        if (count > 0) marks.push(`${mark}${count}`);
    };
    push(status.tree.added, "+");
    push(status.tree.modified, "~");
    push(status.tree.deleted, "-");
    push(status.tree.conflicted, "!");
    if (status.upstream !== null) {
        push(status.upstream.ahead, "↑");
        push(status.upstream.behind, "↓");
    }
    return marks.length === 0 ? "clean" : marks.join(" ");
}

/**
 * La colonne `card` : le renvoi vers la fiche de branche, et le seul geste destructeur de
 * l'écran — qui ne détruit rien.
 *
 * Le bouton de suppression **ouvre une fiche**, il ne supprime pas : la spec §5.4 exige
 * d'énoncer ce que la suppression emporte, et Ash s'arrête là (ADR-0015). Sur le worktree
 * principal, le bouton reste **visible et éteint avec sa raison** — le masquer ferait croire
 * qu'il n'existe pas.
 */
function card(line: WorktreeRow, actions: WorktreeTableActions): UiChild[] {
    const remove = button("remove…").class("git-worktrees-remove");
    if (line.main) {
        remove.disabled("this is the repository's main worktree — git refuses to remove it");
    } else {
        remove.onClick(() => {
            actions.askRemoval(line.worktreeRoot);
        });
    }

    const open =
        line.agentsNow.length === 0
            ? [
                  button("open a tab")
                      .class("git-worktrees-open")
                      .onClick(() => {
                          actions.openTabIn(line.worktreeRoot);
                      }),
              ]
            : [];

    return [
        button("branch card")
            .class("git-worktrees-card")
            .onClick(() => {
                actions.showCard(line);
            }),
        ...open,
        remove,
    ];
}

/**
 * Ce qu'une suppression emporterait — **et ce qu'Ash ne fera pas**.
 *
 * Il n'y a pas de bouton qui supprime, et il n'y en aura pas ici : la commande est rendue
 * comme du texte, à copier dans un terminal, exactement comme les sorties de secours d'un
 * rebase arrêté (ADR-0015). Ce que la fiche de suppression énonce vient du backend, qui l'a
 * relu au moment du geste.
 *
 * **`fiche` a deux sens dans ce fichier, et aucun ne se dit tout court** : la *fiche de
 * branche* est le `.ash/worktree.md` d'ADR-0013, que #31 apportera et que la colonne `card`
 * ne fait que renvoyer ; la *fiche de suppression* est celle-ci, l'énoncé de ce qu'un geste
 * destructeur emporterait — la même forme que la fiche de purge du journal.
 */
function removalNotice(plan: WorktreeRemoval, actions: WorktreeTableActions): UiChild {
    const said = column(
        span("git-worktrees-removal-title", `removing ${plan.worktreeName} would carry:`),
        ...(plan.carries.length === 0
            ? [span("git-worktrees-removal-line", "nothing — everything here is committed")]
            : plan.carries.map((line) => span("git-worktrees-removal-line", line))),
        ...(plan.refused === null ? [] : [span("git-worktrees-removal-refused", plan.refused)]),
        span("git-worktrees-removal-command", plan.command).title(
            "ash never runs this — copy it into a terminal if that is what you want",
        ),
        button("close")
            .class("git-worktrees-removal-close")
            .onClick(() => {
                actions.dismissRemoval();
            }),
    ).class("git-worktrees-removal");
    return said;
}

/** ` · 15m22s`, ou rien pour une date à venir. */
function duration(since: number, now: number): string {
    const elapsed = now - since;
    return elapsed < 0 ? "" : ` · ${formatElapsed(elapsed)}`;
}

/**
 * `3d ago`, `2h ago`, `12m ago` — l'âge d'une observation, à l'échelle où elle se lit.
 *
 * `formatElapsed` de `shared/agent-state` n'est **pas** réutilisée ici, et c'est délibéré :
 * elle est faite pour la durée d'un état d'agent, où l'heure est le plafond utile, et elle
 * écrirait `72h00m` là où cette colonne parle de jours. Les deux mesurent le temps ; elles ne
 * répondent pas à la même question.
 */
export function aged(at: number, now: number): string {
    const seconds = Math.floor((now - at) / 1000);
    if (seconds < 0) return "just now";
    if (seconds < 60) return "just now";

    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;

    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;

    return `${Math.floor(hours / 24)}d ago`;
}

class Cell extends ElementBuilder {
    constructor(column: string, children: readonly UiChild[]) {
        super("div", "git-worktrees-cell");
        // La colonne est nommée sur la cellule : c'est ce qui rend le tableau lisible dans un
        // test — et, sous 900 px, ce que le CSS affiche en étiquette quand les colonnes se
        // replient les unes sous les autres.
        this.attr("data-column", column).add(...children);
    }
}

function cell(column: string, ...children: readonly UiChild[]): Cell {
    return new Cell(column, children);
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
