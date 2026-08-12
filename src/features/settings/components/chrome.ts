import { emptyState, row, text, type UiComponent, type UiChild } from "@/shared/ui";

import type { TestDescription } from "../contract";
import { label, para, spacer, tag } from "./atoms";

/**
 * Ce qui entoure le contenu d'une section : son en-tête, sa note de barème, son pied, et ce
 * qu'elle montre quand elle est vide.
 */

/** L'en-tête d'une section : titre, compteur, puis les actions poussées à droite. */
export function sectionHeader(
    title: string,
    count: string | null,
    actions: readonly UiChild[],
): UiComponent {
    const head = row(tag("h1", "settings-title").add(text(title))).class("settings-head");
    if (count !== null) head.add(label("settings-count", count));
    return head.add(spacer(), ...actions);
}

/**
 * La note de barème, sous l'en-tête de `tools` et nulle part ailleurs.
 *
 * Les quatre libellés viennent du **contrat** : les tests existent en Rust, donc c'est là
 * qu'ils se nomment. Recopiés ici, ils finiraient par décrire un test que la séquence ne
 * lance plus.
 */
export function scaleNote(tests: readonly TestDescription[]): UiComponent {
    const note = para(
        "settings-note",
        text("one command = one tool. ash re-runs the tests on every path or adapter change."),
        tag("br"),
        text("tests · "),
    );
    tests.forEach((test, index) => {
        if (index > 0) note.add(text(" · "));
        note.add(label("settings-note-index", String(test.number)), text(` ${test.shortLabel}`));
    });
    return note;
}

export function foot(sentence: string): UiComponent {
    return tag("footer", "settings-foot").add(label("settings-foot-text", sentence));
}

/**
 * L'état vide de la liste : le constat, et ce qu'il coûte.
 *
 * Le titre seul serait un cul-de-sac — c'est la raison d'être de `prose` dans le socle.
 */
export function noToolsYet(): UiComponent {
    return emptyState("no tools declared")
        .class("settings-empty")
        .prose(
            "ash already shows your tabs, but it doesn't know which ones are agents. until a tool is declared, everything stays idle — no waiting, no notifications.",
        );
}
