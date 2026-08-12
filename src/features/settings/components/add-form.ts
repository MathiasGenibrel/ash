import { button, choice, field, row, type UiComponent, type UiChild } from "@/shared/ui";

import type { SettingsSnapshot, ToolDraft, Verification } from "../contract";
import {
    type AddAction,
    degradedModeSubject,
    describeAddAction,
    NOTHING_VERIFIED_YET,
} from "../model";
import { label, spacer, tag } from "./atoms";
import { degradedRow } from "./degraded";
import { sectionHeader } from "./chrome";
import { testRow } from "./test-row";

/**
 * Le formulaire d'ajout — il **remplace le contenu de la section**, ce n'est ni une modale
 * ni un panneau latéral (§3.8).
 *
 * Ce qu'il montre à gauche du bouton `add`, et le fait que ce bouton soit allumé, viennent
 * de `describeAddAction` : la précédence entre un refus local et un refus du backend est
 * une règle, et elle n'est pas rejouée ici. La ligne `test` est **la même** que celle d'une
 * carte, et l'attente qu'elle affiche est `NOTHING_VERIFIED_YET`, une vérification vide du
 * modèle — pas un cas particulier dessiné par l'écran.
 */
export interface AddFormActions {
    cancelAdding(): void;
    editDraft(patch: Partial<ToolDraft>): void;
    submitDraft(): void;
}

/** La clé de focus d'un champ du formulaire — la vue la retient d'un rendu à l'autre. */
export function draftFocusKey(name: string): string {
    return `draft:${name}`;
}

export function addForm(
    draft: ToolDraft,
    snapshot: SettingsSnapshot,
    draftVerification: Verification | null,
    failure: string | null,
    actions: AddFormActions,
): readonly UiChild[] {
    const escape = button("esc to cancel")
        .class("settings-link")
        .onClick(() => {
            actions.cancelAdding();
        });

    const grid = tag("div", "settings-form").add(
        ...formField("command", draft.command, "the name you type in the shell", (value) => {
            actions.editDraft({ command: value });
        }),
        ...formField(
            "label",
            draft.label,
            "shown instead of the command",
            (value) => {
                actions.editDraft({ label: value });
            },
            "optional",
        ),
        ...adapterField(draft, snapshot.adapters, actions),
        ...formField("config", draft.config, "adapter default", (value) => {
            actions.editDraft({ config: value });
        }),
        label("settings-form-key", "test"),
        testRow(draftVerification ?? NOTHING_VERIFIED_YET, snapshot.tests),
    );

    const body = tag("div", "settings-body", "is-form").add(
        grid,
        formActions(
            describeAddAction(draft, snapshot.tools, failure, draftVerification),
            actions,
        ),
    );

    return [sectionHeader("new tool", null, [escape]), body];
}

function formField(
    name: string,
    value: string,
    gloss: string,
    onInput: (value: string) => void,
    placeholder?: string,
): readonly UiChild[] {
    const input = field(name)
        .class("settings-input")
        .value(value)
        .focusKey(draftFocusKey(name))
        .onInput(onInput);
    if (placeholder !== undefined) input.placeholder(placeholder);

    const line = row(input, label("settings-gloss", gloss)).class("settings-form-line");
    return [label("settings-form-key", name), line];
}

/**
 * Le menu d'adaptateur du formulaire, et l'avertissement qui le suit.
 *
 * `generic` est un mode dégradé, et l'écran le dit **avant** l'ajout (§3.8) : `model`
 * décide s'il y a lieu d'avertir et de qui, la ligne ne fait que le ranger sous le menu
 * qu'il commente — une ligne de grille à cellule de libellé vide.
 */
function adapterField(
    draft: ToolDraft,
    adapters: readonly string[],
    actions: AddFormActions,
): readonly UiChild[] {
    const menu = choice("adapter")
        .class("settings-input", "is-menu")
        .options(adapters, draft.adapter)
        .onSelect((adapter) => {
            actions.editDraft({ adapter });
        });

    const subject = degradedModeSubject(draft);
    const line = row(
        menu,
        label(subject === null ? "settings-gloss" : "settings-gloss is-warning", subject === null ? "" : "degraded mode"),
    ).class("settings-form-line");

    const rows: UiChild[] = [label("settings-form-key", "adapter"), line];
    if (subject !== null) rows.push(...degradedRow(subject));
    return rows;
}

/** La barre d'action, poussée en bas : la raison à gauche, les boutons à droite. */
function formActions(action: AddAction, actions: AddFormActions): UiComponent {
    const cancel = button("cancel")
        .class("settings-button")
        .onClick(() => {
            actions.cancelAdding();
        });

    // Éteint, jamais masqué : « le masquer ferait croire que ça n'existe pas ». La raison
    // reste lisible à gauche, et le socle exige qu'elle voyage aussi avec le bouton.
    const add = button("add")
        .class("settings-button", "is-primary")
        .onClick(() => {
            actions.submitDraft();
        });
    if (!action.enabled) add.disabled(action.reason);

    return row(label("settings-gloss", action.reason), spacer(), cancel, add).class(
        "settings-form-actions",
    );
}
