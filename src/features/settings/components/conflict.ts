import { button, column, row, type UiChild } from "@/shared/ui";

import type { HookChoice, ToolDeclaration } from "../contract";
import { hooksGlyph } from "../verification-state";
import { label, para, tag } from "./atoms";
import { sectionHeader } from "./chrome";
import { diffView } from "./diff-view";
import { hooksNote } from "./hooks-row";

/**
 * L'écran du diff (§4.4) — il **remplace la liste**, et n'écrit rien de lui-même.
 *
 * C'est ici que l'utilisateur tranche. La spec §10 ne demande pas seulement de refuser :
 * elle demande de signaler, de proposer le diff, et de **demander**. Ash montrait le diff et
 * n'offrait rien — « sors d'ici et débrouille-toi dans ton éditeur » — parce que fusionner
 * aurait voulu dire écrire hors de ses marqueurs. Depuis l'amendement du 2026-08-12
 * d'[ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), les marqueurs sont par entrée :
 * Ash sait poser les siennes à côté de celles de l'utilisateur, et les reprendre sans
 * toucher au reste.
 *
 * **Les issues viennent du backend**, libellé compris. L'écran ne décide ni lesquelles sont
 * offertes, ni ce qu'elles promettent : il les rend, et rapporte le clic
 * ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface ConflictActions {
    installHooks(command: string): void;
    removeHooks(command: string): void;
}

export function conflictScreen(
    tool: ToolDeclaration,
    actions: ConflictActions,
    onBack: () => void,
): readonly UiChild[] {
    const back = button("← back to the list").class("settings-button").onClick(onBack);

    const banner = row(
        hooksGlyph(tool.hooks.state, 12),
        label("settings-banner-text", tool.hooks.summary),
    ).class("settings-banner", tool.hooks.state === "conflict" ? "is-error" : "is-caveat");

    // Le fichier concerné : c'est la première question qu'on se pose devant un diff.
    const file = tool.hooks.file ?? "";
    const where = para("settings-locate", label("settings-locate-path", file));

    const body = tag("div", "settings-body", "is-conflict").add(
        banner,
        where,
        diffView(tool.hooks.diff ?? ""),
        hooksNote(tool.hooks),
        choices(tool, actions),
    );

    return [sectionHeader("tools", `${tool.command} · ${file}`, [back]), body];
}

/**
 * Les issues, dans l'ordre où le backend les donne.
 *
 * La première est la principale — c'est celle qui ajoute. `remove` reste secondaire parce
 * qu'elle est destructrice, et chaque bouton porte sa conséquence **à côté de lui** : un
 * bouton qui écrit dans le fichier de quelqu'un ne se lit pas sans savoir ce qu'il y fait.
 */
function choices(tool: ToolDeclaration, actions: ConflictActions): UiChild {
    const list = column().class("settings-choices");
    tool.hooks.choices.forEach((choice: HookChoice, rank: number) => {
        const press = button(choice.label)
            .class("settings-button", rank === 0 && choice.action !== "remove" ? "is-primary" : "")
            .onClick(() => {
                if (choice.action === "remove") actions.removeHooks(tool.command);
                else actions.installHooks(tool.command);
            });
        if (!tool.hooks.enabled) press.disabled(tool.hooks.summary);
        list.add(row(press, label("settings-choice-note", choice.note)).class("settings-choice"));
    });
    return list;
}
