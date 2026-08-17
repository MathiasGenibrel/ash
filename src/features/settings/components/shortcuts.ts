import { row, text, type UiChild } from "@/shared/ui";

import type { Shortcut } from "../contract";
import { groupShortcuts } from "../model";
import { label, para, spacer, tag } from "./atoms";
import { foot, sectionHeader } from "./chrome";

/**
 * La section `shortcuts` de la fenêtre (spec §4.4).
 *
 * **Rien n'est écrit ici : les combinaisons viennent du menu natif**, où elles sont déclarées
 * (`src-tauri/src/menu.rs`). C'est le critère dur de l'issue #110, et il est dur pour une
 * raison : deux listes finissent toujours par diverger, et c'est l'écran des réglages qu'on
 * croit quand elles ne disent pas la même chose. Les groupes sont les sous-menus, dans
 * l'ordre du menu — voir [`groupShortcuts`](../model.ts).
 *
 * **La liste est donc plus courte que le tableau de la spec §4.4, et ce n'est pas un oubli** :
 * la famille git (`⌘⌃B`, `G`, `W`, `M`, `I`) n'a pas encore d'entrée de menu — ni popup de
 * branches, ni graphe, ni onglet de merge n'existent —, donc pas encore d'accélérateur déclaré.
 * Rien ne peut être ajouté ici pour combler l'écart : ce serait recopier une combinaison à la
 * main, et annoncer un raccourci qui ne fait rien. La décision vit du côté qui déclare —
 * `menu_shortcuts` dans `src-tauri/src/menu.rs`, où elle est écrite.
 *
 * **Elle est en lecture seule, et le dit.** Aucun bouton, aucun champ : Ash ne sait pas encore
 * rebinder, et la capture d'une combinaison, les conflits et le retour au défaut sont l'issue
 * #22. Un contrôle posé là en attendant promettrait un geste qui ne mène nulle part — et la
 * spec §4.4 avertit elle-même que trois combinaisons `Cmd+Ctrl` sont prises par le système,
 * ce qui n'est pas une chose à découvrir en cliquant.
 */
export function shortcutsSection(shortcuts: readonly Shortcut[] | null): readonly UiChild[] {
    const head = sectionHeader("shortcuts", null, []);
    if (shortcuts === null) {
        return [
            head,
            tag("div", "settings-body").add(
                para("settings-empty-prose", text("reading them from the menu…")),
            ),
        ];
    }

    const body = tag("div", "settings-body");
    for (const grouped of groupShortcuts(shortcuts)) {
        body.add(
            label("settings-shortcut-group", grouped.group),
            ...grouped.shortcuts.map((shortcut) =>
                row(
                    label("settings-shortcut-name", shortcut.label),
                    spacer(),
                    // La combinaison est déjà écrite comme macOS l'écrit — le backend la
                    // rend en glyphes, parce que c'est lui qui connaît les noms de touches
                    // que `muda` exige et ceux qu'il faut lire à l'envers.
                    label("settings-shortcut-keys", shortcut.keys),
                ).class("settings-shortcut"),
            ),
        );
    }

    return [
        head,
        body,
        foot("read-only: these come from the native menu, and ash can't rebind them yet."),
    ];
}
