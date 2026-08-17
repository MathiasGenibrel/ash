import { button, row, text, type UiChild } from "@/shared/ui";

import {
    FONT_STEPS,
    THEME_MODES,
    type Appearance,
    type FontStep,
    type ThemeMode,
} from "../contract";
import { label, para, tag } from "./atoms";
import { sectionHeader } from "./chrome";

/** Les deux gestes de la section — l'un et l'autre partent au backend et n'y reviennent pas. */
export interface AppearanceActions {
    chooseTheme(mode: ThemeMode): void;
    stepFontSize(step: FontStep): void;
}

/**
 * La section `appearance` de la fenêtre (spec §9, `[appearance]`).
 *
 * **Elle ne détient rien.** Le thème et la taille de police sont à `features::theme`, en
 * Rust, et cette section est leur **seconde surface** — la première étant le menu Vue
 * ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Elle montre donc ce que
 * le backend dit, et demande un changement sans jamais l'appliquer elle-même : un thème
 * bascule quand `ash://theme-mode` revient, pas quand on clique. C'est ce qui fait que la
 * coche du menu et cet écran ne peuvent pas se contredire.
 *
 * **Les trois thèmes sont visibles ensemble**, et celui en vigueur porte `aria-pressed` :
 * c'est la forme du menu natif — trois coches exclusives — et non un menu déroulant, où le
 * choix courant serait le seul lisible et les deux autres à deviner. Rien de plus n'est
 * dessiné ici : l'aperçu qui montrerait les cinq états d'agent, la police au choix et la
 * densité de la sidebar attendent les planches de l'issue #22.
 */
export function appearanceSection(
    appearance: Appearance | null,
    actions: AppearanceActions,
): readonly UiChild[] {
    const head = sectionHeader("appearance", null, []);
    if (appearance === null) {
        // L'aller-retour est immédiat en pratique ; un panneau muet ferait quand même croire
        // à une panne le temps qu'il revienne — c'est la conduite de la section
        // `notifications`, pour la même raison.
        return [
            head,
            tag("div", "settings-body").add(
                para("settings-empty-prose", text("asking ash what it is set to…")),
            ),
        ];
    }

    return [
        head,
        tag("div", "settings-body").add(
            settingRow(
                "theme",
                THEME_MODES.map((mode) =>
                    button(mode)
                        .class("settings-button", mode === appearance.mode ? "is-primary" : "")
                        .attr("aria-pressed", String(mode === appearance.mode))
                        .onClick(() => {
                            actions.chooseTheme(mode);
                        }),
                ),
                // `system` n'est pas une troisième palette : c'est l'absence de choix, donc
                // celui de macOS, et il suit ses bascules sans redémarrage.
                "system follows macOS, and changes with it.",
            ),
            settingRow(
                "terminal font",
                [
                    label("settings-appearance-value", `${String(appearance.fontSize)} pt`),
                    ...FONT_STEPS.map((step) =>
                        button(step)
                            .class("settings-button")
                            .onClick(() => {
                                actions.stepFontSize(step);
                            }),
                    ),
                ],
                // Aucune combinaison écrite ici : les raccourcis se lisent dans la section
                // `shortcuts`, qui les tient du menu. Une copie de plus finirait par nommer
                // une touche que le menu ne déclare plus.
                "one point at a time, and for every open tab at once — the same setting the View menu steps.",
            ),
        ),
    ];
}

/** Une ligne de réglage : son nom, ses contrôles, et ce que le réglage engage. */
function settingRow(name: string, controls: readonly UiChild[], note: string): UiChild {
    return tag("div", "settings-appearance-row").add(
        row(label("settings-appearance-key", name), ...controls).class("settings-appearance-line"),
        para("settings-note", text(note)),
    );
}
