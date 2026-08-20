import type { StoppedOperation } from "@/shared/ipc";
import { badge, button, column, paint, row, text, type UiComponent } from "@/shared/ui";

/**
 * La vue `conflicts` du panneau bas (spec §7.4) — **la porte d'entrée, et rien de plus**.
 *
 * Elle est écrite ici au **minimum** de ce que #30 en a besoin : dire ce qui est arrêté, et
 * ouvrir l'onglet de merge. La vue complète que #29 attend — l'opération, les chemins, le
 * `2/5`, `ORIG_HEAD`, les `escapes` et le bouton « passer à l'agent » — est un ticket à
 * elle, et ce fichier est fait pour être remplacé par elle, pas pour la préempter.
 *
 * Ce qu'elle respecte déjà, et qui ne se renégocie pas : **rien n'est exécuté**. Les deux
 * sorties de secours sont du texte
 * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)), et le seul
 * geste offert ouvre un onglet — il n'écrit pas une ligne dans le dépôt.
 */

export interface ConflictsActions {
    /** Ouvrir l'onglet de merge de ce worktree — la seconde route de la spec §7.4. */
    readonly resolveInAsh: () => void;
}

/**
 * `stopped` à `null` couvre les deux cas qui se rendent pareil : aucun worktree sous les
 * yeux, et un worktree où rien n'est en cours. C'est **le cas courant**, et il se rend en
 * ne proposant rien.
 */
export function conflictsView(
    stopped: StoppedOperation | null,
    actions: ConflictsActions,
): UiComponent {
    if (stopped === null) {
        return column(text("Nothing is stopped in this worktree.")).class("git-conflicts");
    }

    const count = stopped.conflictedTotal ?? stopped.conflicts.length;
    return column(
        row(
            text(describe(stopped)),
            badge(`${String(count)} conflicted`).class("git-conflicts-count"),
            row().spacer(),
            button("resolve in ash").class("git-conflicts-open").onClick(actions.resolveInAsh),
        ).class("git-conflicts-head"),
        row(...stopped.conflicts.map((path) => text(path))).class("git-conflicts-paths"),
        // `abort` et `skip` : **visibles avant d'entrer**, et pas exécutables.
        row(text("escapes"), ...stopped.escapes.map((escape) => text(escape))).class(
            "git-conflicts-escapes",
        ),
    ).class("git-conflicts");
}

function describe(stopped: StoppedOperation): string {
    const operation = stopped.operation;
    const step =
        operation.progress === null
            ? ""
            : ` · ${String(operation.progress.step)}/${String(operation.progress.total)}`;
    const rescue = stopped.origHead === null ? "" : ` · ORIG_HEAD ${stopped.origHead}`;
    return `${operation.kind} stopped${step}${rescue}`;
}

/** Peint la vue dans le corps du panneau. Le panneau ne connaît pas git ; git ne le connaît pas. */
export function paintConflicts(
    body: HTMLElement,
    stopped: StoppedOperation | null,
    actions: ConflictsActions,
): void {
    body.replaceChildren(paint(conflictsView(stopped, actions).build()));
}
