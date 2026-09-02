import type { ConflictFile, MergeHunk, MergeView } from "@/shared/ipc";
import { badge, button, column, row, text, type UiComponent } from "@/shared/ui";

import { editor } from "./editor";

/**
 * L'écran de l'onglet de merge — **une fonction pure** (spec §7.4, issue #30).
 *
 * Une vue entre, une description sort : le fichier ne touche pas au DOM, ne lit rien, et
 * n'invoque rien. C'est ce qui rend vérifiables les trois choses que le ticket demande —
 * que les côtés portent le nom de leur branche, que `continue` reste **visible et éteint**
 * avec son compte, et que le panneau central est éditable — sans monter de fenêtre.
 *
 * **Rien n'est calculé ici.** Le compte, les hunks, les noms des côtés et le droit de
 * continuer viennent tous du backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). L'écran choisit
 * seulement *quel* fichier et *quel* hunk sont sous les yeux, et ce qui est tapé dans le
 * panneau du milieu tant que ce n'est pas appliqué — deux faits d'affichage, comme la
 * sélection d'onglet.
 */

/** Ce que l'écran garde de son côté : la sélection, et la frappe en cours. */
export interface MergeSelection {
    /** Le fichier regardé, ou `null` — le premier de la liste par défaut. */
    readonly path: string | null;
    /** Le rang du hunk regardé dans ce fichier. */
    readonly hunk: number;
    /** Ce qui est tapé dans le panneau du milieu. Vide au départ : Ash ne choisit pas. */
    readonly draft: string;
}

export const NO_SELECTION: MergeSelection = { path: null, hunk: 0, draft: "" };

/** Les gestes de l'écran. Aucun n'est pris sans un clic. */
export interface MergeActions {
    // Des **propriétés** de type fonction, et non des méthodes : une action est passée
    // telle quelle à `onClick`, détachée de son objet, et une méthode détachée emporterait
    // un `this` qui n'existe plus. Le lint du dépôt le refuse, et il a raison.
    readonly selectFile: (path: string) => void;
    readonly selectHunk: (index: number) => void;
    /** Le panneau central a changé. */
    readonly edit: (draft: string) => void;
    /** Recopier un côté dans le panneau central — un point de départ, pas une décision. */
    readonly take: (side: "left" | "right") => void;
    /** Trancher ce hunk : le fichier est réécrit, et mis dans l'index s'il est fini. */
    readonly apply: () => void;
    /** `git <op> --continue`. */
    readonly proceed: () => void;
    /** Passer les conflits restants à l'agent — le prompt est **écrit**, jamais envoyé. */
    readonly handOverRest: () => void;
}

/** Le fichier regardé : celui de la sélection, ou le premier qui reste à trancher. */
export function currentFile(view: MergeView, selection: MergeSelection): ConflictFile | null {
    const files = view.stopped?.files ?? [];
    const chosen = files.find((file) => file.path === selection.path);
    return chosen ?? files.find((file) => !file.resolved) ?? files[0] ?? null;
}

/** Le hunk regardé dans ce fichier, ou `null` quand il n'en reste aucun. */
export function currentHunk(
    file: ConflictFile | null,
    selection: MergeSelection,
): MergeHunk | null {
    if (file === null) return null;
    return file.hunks[selection.hunk] ?? file.hunks[0] ?? null;
}

/**
 * L'écran entier.
 *
 * `notice` est ce que la dernière action a répondu — la sortie de git, ou la phrase du
 * prompt rédigé. Elle s'affiche telle quelle : c'est le backend qui compose les messages
 * qui nomment leurs deux côtés (spec §7.1), pas la webview.
 */
export function mergeScreen(
    view: MergeView,
    selection: MergeSelection,
    notice: string | null,
    actions: MergeActions,
): UiComponent {
    const stopped = view.stopped;
    if (stopped === null) {
        // L'opération s'est terminée ailleurs — dans un terminal, par un agent. L'onglet
        // reste ouvert et le dit ; rien ne se ferme sans un geste (ADR-0010).
        return column(
            row(text(view.title)).class("merge-head"),
            column(
                text("Nothing is stopped in this worktree any more."),
                text("Close this tab when you are done with it — it holds nothing."),
            ).class("merge-empty"),
        ).class("merge-tab");
    }

    const file = currentFile(view, selection);
    const hunk = currentHunk(file, selection);

    return column(
        head(view, stopped, actions),
        fileStrip(stopped.files, file, actions),
        hunk === null || file === null
            ? column(text("No conflict markers left in this file.")).class("merge-empty")
            : panels(stopped, file, hunk, selection, actions),
        foot(stopped, notice),
    ).class("merge-tab");
}

function head(
    view: MergeView,
    stopped: NonNullable<MergeView["stopped"]>,
    actions: MergeActions,
): UiComponent {
    // `continue` **reste visible**, éteint, avec son compte à droite (spec §7.4). Le socle
    // de composants rend l'extinction muette impossible : `disabled` exige sa raison.
    const proceed = button(stopped.continueCommand).class("merge-continue");
    if (stopped.canContinue) {
        proceed.onClick(actions.proceed);
    } else {
        proceed.disabled(`${String(remaining(stopped))} conflict(s) still to resolve`);
    }

    return row(
        text(view.title),
        badge(`${String(remaining(stopped))} left`).class("merge-count"),
        row().spacer(),
        button("hand the rest to the agent").class("merge-hand-over").onClick(actions.handOverRest),
        proceed,
    ).class("merge-head");
}

/** Combien de conflits restent, listés ou non. C'est le compte que le bouton porte. */
function remaining(stopped: NonNullable<MergeView["stopped"]>): number {
    return stopped.unresolved + stopped.hidden;
}

function fileStrip(
    files: readonly ConflictFile[],
    current: ConflictFile | null,
    actions: MergeActions,
): UiComponent {
    return row(
        ...files.map((file) => {
            const entry = button(file.path).class("merge-file");
            if (file.path === current?.path) entry.class("is-current");
            if (file.resolved) entry.class("is-resolved");
            if (file.unreadable) {
                // Un chemin que git a dû échapper n'est jamais ouvert ni réécrit par Ash.
                // Il reste **listé et compté** : un conflit invisible ferait un bouton
                // `continue` éteint sans raison lisible.
                entry.disabled(
                    "Ash does not open a path git had to quote — resolve it in an editor",
                );
            } else {
                entry.onClick(() => {
                    actions.selectFile(file.path);
                });
            }
            return entry;
        }),
    ).class("merge-files");
}

/**
 * Les trois panneaux — et le seul endroit où les côtés sont nommés.
 *
 * Ils portent `sides.left.name` et `sides.right.name`, jamais `ours` ni `theirs` : le
 * jargon de git s'inverse entre un rebase et un merge, et le backend a déjà tranché lequel
 * des deux sens s'applique (spec §7.4).
 */
function panels(
    stopped: NonNullable<MergeView["stopped"]>,
    file: ConflictFile,
    hunk: MergeHunk,
    selection: MergeSelection,
    actions: MergeActions,
): UiComponent {
    const at = file.hunks.indexOf(hunk);
    return column(
        row(
            button("◀")
                .class("merge-step")
                .onClick(() => {
                    actions.selectHunk(at - 1);
                }),
            text(`hunk ${String(at + 1)}/${String(file.hunks.length)} · ${file.path}`),
            button("▶")
                .class("merge-step")
                .onClick(() => {
                    actions.selectHunk(at + 1);
                }),
        ).class("merge-hunk-bar"),
        row(
            side(stopped.sides.left.name, stopped.sides.left.role, hunk.ours, () => {
                actions.take("left");
            }),
            column(
                row(text("result"), text("editable")).class("merge-panel-head"),
                editor("resolution", selection.draft).onInput(actions.edit),
                row(button("apply this hunk").class("merge-apply").onClick(actions.apply)).class(
                    "merge-panel-foot",
                ),
            ).class("merge-panel is-result"),
            side(stopped.sides.right.name, stopped.sides.right.role, hunk.theirs, () => {
                actions.take("right");
            }),
        ).class("merge-panels"),
    ).class("merge-body");
}

function side(name: string, role: string, content: string, take: () => void): UiComponent {
    return column(
        row(text(name), text(role)).class("merge-panel-head"),
        column(text(content)).class("merge-side-body"),
        row(button(`take ${name}`).class("merge-take").onClick(take)).class("merge-panel-foot"),
    ).class("merge-panel");
}

/**
 * Le filet de secours et les deux sorties — **du texte**.
 *
 * `abort` et `skip` restent visibles avant d'entrer (spec §7.4), et Ash ne les exécute
 * pas : `--abort` jette le travail de l'utilisateur, et Ash ne valide rien à sa place
 * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
 */
function foot(stopped: NonNullable<MergeView["stopped"]>, notice: string | null): UiComponent {
    const escapes = row(text("escapes"), ...stopped.escapes.map((escape) => text(escape))).class(
        "merge-escapes",
    );

    return column(
        stopped.origHead === null
            ? row().class("merge-rescue")
            : row(text(`ORIG_HEAD ${stopped.origHead}`)).class("merge-rescue"),
        escapes,
        notice === null ? row().class("merge-notice") : row(text(notice)).class("merge-notice"),
    ).class("merge-foot");
}
