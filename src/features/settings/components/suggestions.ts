import { button, column, row, text, type UiChild } from "@/shared/ui";

import type { ToolSuggestion } from "../contract";
import { hooksGlyph, presentHooks } from "../verification-state";
import { label, para, spacer, tag } from "./atoms";

/**
 * Les outils qu'Ash a **vus tourner** et que personne n'a déclarés — sous les cartes
 * ([ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)).
 *
 * La fenêtre ouvrait sur « no tools declared » pendant qu'Ash savait très bien que `claude`
 * tenait l'avant-plan de trois onglets : cette connaissance ne servait qu'au marqueur discret
 * de la sidebar, et il fallait deviner qu'on passait par là pour déclarer un outil.
 *
 * **Une ligne ne porte qu'un geste, et il n'écrit rien chez l'utilisateur.** Déclarer fait
 * rejoindre les cartes et repartir dans le flux qui existe déjà — vérification en deux temps,
 * puis bouton d'installation. Aucun hook n'est posé par ce clic
 * ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)), et c'est ce que la ligne dit.
 *
 * L'état des hooks est celui du backend, dans **les cinq** états de `HookState` : un conflit
 * ne se lit pas comme une absence, et un adaptateur qui n'instrumente rien le dit plutôt que
 * de laisser lire une panne. Rien n'est décidé ici
 * ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface SuggestionActions {
    declareSuggestion(suggestion: ToolSuggestion): void;
}

/** L'en-tête du bloc : ce que c'est, et combien il y en a. */
function heading(count: number): UiChild {
    return row(
        label("settings-suggest-title", count === 1 ? "seen running" : `seen running · ${count}`),
    ).class("settings-suggest-head");
}

export function suggestionList(
    suggestions: readonly ToolSuggestion[],
    actions: SuggestionActions,
): readonly UiChild[] {
    if (suggestions.length === 0) return [];

    return [
        tag("section", "settings-suggest").add(
            heading(suggestions.length),
            column(...suggestions.map((suggestion) => suggestionRow(suggestion, actions))).class(
                "settings-suggest-list",
            ),
            // La phrase qui compte : ce que le clic **ne** fait pas. Sans elle, un bouton
            // sur une ligne qui parle de hooks se lirait comme un bouton qui en pose.
            para(
                "settings-suggest-note",
                text(
                    "ash saw these in the foreground of a tab — it looked nowhere else, and read no folder to find them. declaring one writes nothing: it starts the checks, and installing the hooks stays a separate press.",
                ),
            ),
        ),
    ];
}

/**
 * Une ligne : le nom, l'adaptateur, l'état de sa configuration, et le seul geste offert.
 *
 * Le fichier est en pastille quand le backend en nomme un — et la table de présentation
 * décide, comme pour la ligne `hooks` d'une carte : un refus `blocked` le nomme déjà dans sa
 * phrase.
 */
function suggestionRow(suggestion: ToolSuggestion, actions: SuggestionActions): UiChild {
    const shown = presentHooks(suggestion.hooks);
    const line = row(
        label("settings-suggest-name", suggestion.command),
        label("settings-suggest-adapter", suggestion.adapter),
    ).class("settings-suggest-row");

    const state = row(
        hooksGlyph(suggestion.hooks),
        label("settings-hooks-reason", suggestion.summary),
    ).class("settings-suggest-state", shown.rowClassName);
    if (suggestion.file !== null && shown.showsFile) {
        state.add(label("settings-hooks-file", suggestion.file));
    }

    return line.add(
        state,
        spacer(),
        button("declare")
            .class("settings-button", "is-primary")
            .attr("aria-label", `declare ${suggestion.command}`)
            .onClick(() => {
                actions.declareSuggestion(suggestion);
            }),
    );
}
