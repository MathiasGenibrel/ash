import { text, type UiChild } from "@/shared/ui";

import type { ToolDeclaration } from "../contract";
import { countProblems } from "../model";
import { SETTINGS_SECTIONS, type SettingsSection } from "../sections";
import { label, para, spacer, tag } from "./atoms";

/**
 * La colonne de gauche : les quatre sections, et le rappel de leur parcours.
 *
 * Le compteur de la colonne compte **comme celui de l'en-tête** — les deux montrent le même
 * chiffre au même instant, donc ils le comptent au même endroit (`countProblems`). Il
 * n'apparaît que si la section en a un : un `0` permanent apprendrait à ne plus regarder
 * cet endroit.
 */
export function navColumn(
    active: SettingsSection,
    tools: readonly ToolDeclaration[],
    onSelect: (section: SettingsSection) => void,
): readonly UiChild[] {
    const invalid = countProblems(tools);

    const rows = SETTINGS_SECTIONS.map((section) => {
        // Un vrai bouton, et pas une `div` cliquable : c'est ce qui met la section sur le
        // chemin de `tab` et dans l'arbre d'accessibilité sans une ligne de code.
        const row = tag("button", "settings-nav-row", section === active ? "is-active" : "")
            .attr("type", "button")
            .attr("aria-current", section === active ? "true" : "false")
            .add(label("settings-nav-name", section))
            .on("click", () => {
                onSelect(section);
            });

        if (section === "tools" && invalid > 0) {
            row.add(spacer(), label("settings-nav-count", String(invalid)));
        }
        return row;
    });

    return [...rows, para("settings-nav-hint", text("tab / ⌥↑↓ to move"))];
}
