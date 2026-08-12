import { button, row, type UiChild } from "@/shared/ui";

import type { ToolDeclaration } from "../contract";
import { hooksGlyph } from "../verification-state";
import { label, para, tag } from "./atoms";
import { sectionHeader } from "./chrome";
import { diffView } from "./diff-view";
import { hooksNote } from "./hooks-row";

/**
 * L'écran de conflit (§4.4) — il **remplace la liste**, et n'écrit rien.
 *
 * C'est le refus lui-même : la spec §10 ne demande pas seulement de refuser, elle demande
 * de signaler, de proposer le diff, et de demander. Ash ne propose donc ici ni `replace`,
 * ni `merge` : l'un écraserait les lignes de l'utilisateur, l'autre demanderait d'écrire
 * hors des marqueurs — ce que toute la feature `hooks` interdit
 * ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)). Le seul geste qui reste est de
 * sortir, et de décider dans son éditeur.
 */
export function conflictScreen(tool: ToolDeclaration, onBack: () => void): readonly UiChild[] {
    const back = button("← back to the list")
        .class("settings-button")
        .onClick(onBack);

    const banner = row(
        hooksGlyph("conflict", 12),
        label(
            "settings-banner-text",
            "the ash block in this file was edited by hand — ash writes nothing until this is settled",
        ),
    ).class("settings-banner", "is-error");

    // Le fichier concerné : c'est la première question qu'on se pose devant un refus.
    const file = tool.hooks.file ?? "";
    const where = para("settings-locate", label("settings-locate-path", file));

    const body = tag("div", "settings-body", "is-conflict").add(
        banner,
        where,
        diffView(tool.hooks.diff ?? ""),
        hooksNote(tool.hooks),
    );

    return [sectionHeader("tools", `${tool.command} · ${file}`, [back]), body];
}
