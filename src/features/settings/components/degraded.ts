import { text, type UiChild } from "@/shared/ui";

import { cell, label, lineBreak, para } from "./atoms";

/**
 * L'avertissement du mode dégradé (§3.8).
 *
 * C'est le seul endroit de l'interface où du texte courant est teint par état : les quatre
 * mots portent les classes de `app/styles.css` que la sidebar et la ligne de statut
 * utilisent déjà, donc les mêmes couleurs, définies au même endroit.
 *
 * Le **sujet** vient de `model` (`degradedModeSubject`, `degradedFixSubject`), qui décide
 * s'il y a lieu d'avertir ; ce composant ne fait que l'écrire.
 */
export function degradedNotice(subject: string): UiChild {
    // La maquette écrit « ash reads the process output ». Ash ne lit **jamais** la sortie
    // du PTY pour en déduire un état — [ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)
    // l'écarte explicitement. Il observe le processus (la sonde d'ADR-0005). La phrase est
    // corrigée ; ce qu'elle apprend — trois états au lieu de cinq — est identique.
    return para(
        "settings-degraded",
        text("without a dedicated adapter, ash watches the process, not its hooks."),
        lineBreak(),
        text(`${subject} will show as `),
        stateWord("idle"),
        text(" · "),
        stateWord("done"),
        text(" · "),
        stateWord("error"),
        text(" — never "),
        stateWord("waiting"),
        text(". no “waiting for a reply” notification for this tool."),
    );
}

/** L'avertissement posé dans une grille, sous ce qu'il commente. */
export function degradedRow(subject: string): readonly UiChild[] {
    return [cell(), degradedNotice(subject)];
}

function stateWord(state: "idle" | "done" | "error" | "waiting"): UiChild {
    return label(`ash-state-word is-${state}`, state);
}
