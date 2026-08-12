import { button, row, type UiChild } from "@/shared/ui";

import type { ToolDeclaration } from "../contract";
import { describeDuplicates } from "../model";
import { hooksGlyph } from "../verification-state";
import { label, spacer } from "./atoms";

/**
 * La bannière de doublon (§3.7) — **entre l'en-tête et la liste**, parce qu'elle ne décrit
 * aucune des deux cartes en particulier.
 *
 * Elle n'existe que si `describeDuplicates` en produit une : la phrase, les entrées en
 * cause et le droit d'annuler sont des règles de `model`, et ce composant ne les rejoue pas.
 * Le `undo the reset` n'apparaît que si une réinitialisation a créé la collision — proposer
 * d'annuler un geste qui n'a pas eu lieu ferait chercher lequel.
 */
export interface DuplicateBannerActions {
    undoReset(command: string): void;
}

export function duplicateBanner(
    tools: readonly ToolDeclaration[],
    actions: DuplicateBannerActions,
): readonly UiChild[] {
    const shown = describeDuplicates(tools);
    if (shown === null) return [];

    const line = row(hooksGlyph("outdated", 12), label("settings-banner-text", shown.sentence)).class(
        "settings-banner",
        "is-warning",
    );

    const undo = shown.undo;
    if (undo !== null) {
        line.add(
            spacer(),
            button("undo the reset")
                .class("settings-button", "is-small", "is-nowrap")
                .onClick(() => {
                    actions.undoReset(undo);
                }),
        );
    }
    return [line];
}
