import { row, text, type UiComponent } from "@/shared/ui";

import { parseDiff } from "../model";
import { label, para, tag } from "./atoms";

/**
 * Le diff d'un conflit — les lignes qui divergent, et ce qui n'est pas touché.
 *
 * La légende dit le sens **du diff qu'on affiche** : `−` ce qu'Ash écrirait, `+` ce que le
 * fichier porte. C'est celui du backend, et l'annoncer autrement ferait lire chaque ligne à
 * l'envers — la seule faute qu'un diff ne pardonne pas. Le découpage, lui, est une décision
 * de `model` ([`parseDiff`](../model.ts)) et n'est pas refait ici.
 */
export function diffView(diff: string): UiComponent {
    const head = row(
        label("settings-diff-legend is-removed", "− the ash block"),
        label("settings-diff-legend is-added", "+ this file"),
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
                "outside the ash block the file is untouched — ash changes nothing between its markers either, until this is settled.",
            ),
        ),
    );
}
