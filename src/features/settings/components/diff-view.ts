import { row, text, type UiComponent } from "@/shared/ui";

import { parseDiff } from "../model";
import { label, para, tag } from "./atoms";

/**
 * Le diff de ce qu'Ash écrirait — les lignes qui changeraient, et celles qu'il ne touche pas.
 *
 * La légende dit le sens **du diff qu'on affiche** : `−` le fichier tel qu'il est, `+` tel
 * qu'Ash le laisserait. C'est celui du backend, et l'annoncer autrement ferait lire chaque
 * ligne à l'envers — la seule faute qu'un diff ne pardonne pas. Le découpage, lui, est une
 * décision de `model` ([`parseDiff`](../model.ts)) et n'est pas refait ici.
 *
 * Le sens s'est inversé le 2026-08-12 avec la fusion
 * ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), amendement) : ce diff n'est plus
 * la forme d'un refus — « voici ce que j'allais mettre, voici ce que tu as mis » — mais
 * **ce sur quoi l'utilisateur tranche**, avant toute écriture.
 */
export function diffView(diff: string): UiComponent {
    const head = row(
        label("settings-diff-legend is-removed", "− the file as it is"),
        label("settings-diff-legend is-added", "+ what ash would write"),
    ).class("settings-diff-head");

    const body = tag("pre", "settings-diff-body");
    for (const line of parseDiff(diff)) {
        const sign = line.kind === "removed" ? "−" : line.kind === "added" ? "+" : " ";
        body.add(label(`settings-diff-line is-${line.kind}`, `${sign} ${line.text}`));
    }

    return tag("div", "settings-diff").add(
        head,
        body,
        para(
            "settings-diff-foot",
            text(
                "nothing is written until you choose below. ash only ever writes — and later removes — the entries carrying its own marker.",
            ),
        ),
    );
}
