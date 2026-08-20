import { button, row, text, type UiChild, type UiComponent } from "@/shared/ui";

import type { ConflictChoice, ShortcutConflict, ShortcutRow, ShortcutsReport } from "../contract";
import { groupShortcuts } from "../model";
import { label, para, spacer, tag } from "./atoms";
import { foot, sectionHeader } from "./chrome";

/**
 * La section `shortcuts` de la fenêtre (spec §4.4, planche `3j`).
 *
 * **Rien n'est décidé ici, et rien n'y est écrit.** Les combinaisons, les défauts, le
 * compteur `n changed`, la phrase d'une combinaison réservée, le diagnostic d'un conflit et
 * les libellés de ses deux issues viennent tous du backend, où les liaisons sont détenues
 * (`src-tauri/src/features/shortcuts/`). C'est le critère de l'issue #110, et il n'a pas
 * changé quand les raccourcis sont devenus réglables : le menu natif et cet écran dérivent de
 * la **même** liste, refaite du même côté
 * ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * Les six formes de ligne de la planche sont ici, et une seule à la fois :
 *
 * | Forme | Ce qui la déclenche |
 * |---|---|
 * | repos | le cas courant |
 * | survol / focus | du CSS, rien d'autre |
 * | modifiée | `row.changed` — l'icône de retour n'existe que là |
 * | capture | `capture.action` désigne la ligne |
 * | conflit | `report.conflict` nomme ses deux côtés |
 * | combinaison avalée | `row.reservation` |
 *
 * **La liste reste plus courte que le tableau de la spec §4.4, et ce n'est pas un oubli** :
 * la famille git (`⌘⌃B`, `G`, `W`, `M`, `I`) n'a pas encore d'entrée de menu — ni popup de
 * branches, ni graphe, ni onglet de merge n'existent —, donc pas encore de liaison. Rien ne
 * peut être ajouté ici pour combler l'écart : ce serait recopier une combinaison à la main, et
 * annoncer un raccourci qui ne fait rien (issue #127).
 */
export interface ShortcutsActions {
    /** Ouvre le bloc de capture sur une ligne — clic, ou `⏎` sur la ligne au focus. */
    openCapture(action: string): void;
    /** L'icône de retour d'une ligne changée. */
    resetShortcut(action: string): void;
    /** `reset all` de l'en-tête. */
    resetAll(): void;
    /** L'une des deux issues nommées du bloc de conflit. */
    resolveConflict(choice: ConflictChoice): void;
}

/**
 * La capture en cours, telle que la fenêtre la tient.
 *
 * C'est le seul état de la section qui vive côté webview, et il n'est pas une liaison : c'est
 * « quelle ligne est ouverte, et qu'est-ce que le backend a dit de la dernière frappe ». La
 * combinaison, elle, n'est posée qu'au `⏎`, et par le backend.
 */
export interface ShortcutCapture {
    action: string;
    /** Ce que le backend a répondu de la dernière frappe, ou `null` avant la première. */
    keys: string;
    /** `null` tant que rien n'est refusé — la phrase vient du backend. */
    why: string | null;
    /** L'avertissement de la planche : annoncé, **jamais** interdit. */
    note: string | null;
}

export function shortcutsSection(
    report: ShortcutsReport | null,
    capture: ShortcutCapture | null,
    actions: ShortcutsActions,
): readonly UiChild[] {
    if (report === null) {
        return [
            sectionHeader("shortcuts", null, []),
            tag("div", "settings-body").add(
                para("settings-empty-prose", text("reading them from the menu…")),
            ),
        ];
    }

    const head = sectionHeader(
        "shortcuts",
        report.changed === 0 ? null : `${report.changed} changed`,
        [
            button("reset all")
                .class("settings-reset-all")
                .onClick(() => {
                    actions.resetAll();
                }),
        ],
    );

    const body = tag("div", "settings-body");
    // Les deux lignes d'un conflit sont réunies dans **un seul bloc** : il prend la place de
    // la première des deux, et la seconde ne se redessine pas ailleurs. Les laisser à leur
    // place respective aurait montré deux lignes qui se contredisent sans dire qu'elles se
    // répondent — et elles peuvent être dans deux groupes différents, d'où le drapeau tenu
    // hors de la boucle des groupes.
    const conflicting = sides(report.conflict);
    let posed = false;
    for (const grouped of groupShortcuts(report.rows)) {
        body.add(label("settings-shortcut-group", grouped.group));
        for (const line of grouped.shortcuts) {
            if (conflicting.includes(line.action)) {
                if (posed) continue;
                posed = true;
                // Les libellés sont cherchés dans **toutes** les lignes, pas seulement celles
                // du groupe courant : rien n'oblige les deux fautives à être voisines.
                body.add(conflictBlock(report.conflict, actions));
                continue;
            }
            body.add(
                capture !== null && capture.action === line.action
                    ? captureBlock(line, capture)
                    : restRow(line, actions),
            );
        }
    }

    return [
        head,
        body,
        // Le pied dit une **conséquence**, pas une aide : c'est la règle de tous les pieds de
        // cette fenêtre. Ici, il explique pourquoi l'icône de retour n'est pas sur toutes les
        // lignes — et où sont les deux gestes qu'aucun cadre de la planche ne dessine.
        foot("only appears on changed rows. · tab walks the rows · ⏎ opens capture"),
    ];
}

/** Les deux actions que le conflit oppose, ou rien. */
function sides(conflict: ShortcutConflict | null): readonly string[] {
    return conflict === null ? [] : [conflict.holder, conflict.asked];
}

/**
 * Une ligne au repos — et son icône de retour si elle a changé.
 *
 * Une ligne rebindable est un **vrai bouton** : c'est ce qui la met sur le chemin de `tab` et
 * dans l'arbre d'accessibilité sans une ligne de code, et ce qui fait que `⏎` ouvre la capture
 * sans qu'on écrive un gestionnaire de touche. La famille `⌘1 … ⌘9`, elle, ne se capture pas,
 * donc elle n'est pas un bouton : un bouton qui ne fait rien est pire qu'un texte.
 */
function restRow(line: ShortcutRow, actions: ShortcutsActions): UiComponent {
    const pill = label(
        line.keys === "" ? "settings-shortcut-keys is-none" : "settings-shortcut-keys",
        line.keys === "" ? "no shortcut" : line.keys,
    );
    if (line.reservation !== null) pill.class("is-swallowed");

    const inside: UiChild[] = [spacer()];
    // L'avertissement se lit **avant** la pastille, comme sur la planche : c'est lui qui
    // explique pourquoi la combinaison est éteinte, et on le lit dans cet ordre.
    if (line.reservation !== null) {
        inside.push(label("settings-shortcut-swallowed", line.reservation.note));
    }
    inside.push(pill);

    const holder = tag("div", "settings-shortcut");
    if (line.changed) holder.class("is-changed");

    holder.add(
        line.rebindable
            ? button(line.label)
                  .class("settings-shortcut-open")
                  .title("change this shortcut")
                  .add(...inside)
                  .onClick(() => {
                      actions.openCapture(line.action);
                  })
            : row(label("settings-shortcut-name", line.label), ...inside).class(
                  "settings-shortcut-fixed",
              ),
    );

    if (line.changed) {
        holder.add(
            button("↺")
                .class("settings-shortcut-reset")
                .title("back to default")
                .onClick(() => {
                    actions.resetShortcut(line.action);
                }),
        );
    }
    return holder;
}

/**
 * Le bloc de capture — il **s'agrandit sur place**, il n'ouvre pas de modale.
 *
 * La note de la planche insiste, et c'est la raison de cette forme : « la ligne en capture
 * s'agrandit au lieu d'ouvrir une modale : le contexte reste lisible pendant qu'on appuie ».
 * L'ancienne valeur reste donc lisible (`was: ⌘T`), et les trois issues sont écrites sous la
 * combinaison.
 *
 * Il est **focalisable** (`tabindex`) parce qu'il consomme les frappes : sans focus dedans,
 * `esc`, `⌫` et `⏎` n'arriveraient jamais jusqu'à la fenêtre.
 */
function captureBlock(line: ShortcutRow, capture: ShortcutCapture): UiComponent {
    const held =
        capture.keys === ""
            ? label("settings-capture-prompt", "press a key combination")
            : label("settings-capture-keys", capture.keys);

    const block = tag("div", "settings-shortcut settings-capture")
        .attr("tabindex", "0")
        .add(
            row(
                label("settings-shortcut-name", line.label),
                spacer(),
                held,
                tag("span", "settings-capture-caret"),
            ),
            row(
                label("settings-capture-key", "esc"),
                label("settings-capture-word", "cancel"),
                label("settings-capture-dot", "·"),
                label("settings-capture-key", "⌫"),
                label("settings-capture-word", "no shortcut"),
                label("settings-capture-dot", "·"),
                label("settings-capture-key", "⏎"),
                label("settings-capture-word", "confirm"),
                spacer(),
                label("settings-capture-word", "was:"),
                label("settings-capture-was", line.keys === "" ? "no shortcut" : line.keys),
            ).class("settings-capture-help"),
        );

    // L'avertissement macOS est **dans** le bloc, sous un filet — et il n'empêche rien : la
    // combinaison reste posable. « Annoncée comme inefficace, jamais interdite. »
    if (capture.note !== null) {
        block.add(
            row(
                label("settings-capture-warn-glyph", "△"),
                label("settings-capture-warn", `${capture.keys} ${capture.note}`),
            ).class("settings-capture-notice"),
        );
    }
    // Un refus, lui, est autre chose : la frappe n'est pas une combinaison, et `⏎` ne la
    // posera pas. La phrase vient du backend, qui possède la règle.
    if (capture.why !== null) {
        block.add(
            row(label("settings-capture-warn", capture.why)).class("settings-capture-notice"),
        );
    }
    return block;
}

/**
 * Le bloc de conflit : les **deux** lignes fautives, le diagnostic, et ses issues nommées.
 *
 * « Un conflit interne se résout par un choix explicite : ash ne réattribue jamais en
 * silence. » Le premier bouton est le conséquent — il donne la combinaison —, le second le
 * secondaire. Aucun des deux n'est un défaut : rien n'est appliqué tant que le bloc est là.
 *
 * **Il n'y en a qu'un quand le backend n'offre pas `give`** : la combinaison est alors tenue
 * par une ligne qui ne se règle pas, et le bloc est un refus (issue #137). L'écran ne fait
 * pas ce partage, il le lit — le diagnostic dit déjà pourquoi c'est sans appel.
 */
function conflictBlock(
    conflict: ShortcutConflict | null,
    actions: ShortcutsActions,
): UiComponent {
    const block = tag("div", "settings-conflict-block");
    if (conflict === null) return block;

    // Le libellé vient du **contrat**, pas de la liste affichée : une combinaison peut être
    // tenue par une ligne que l'écran ne montre pas — les huit positions d'onglet cachées
    // derrière « Tab 1 … Tab 9 » (issue #137). Les chercher dans `lines` rendait alors
    // l'identifiant interne, `tab:select:2`, dans une fenêtre de réglages. Le backend a déjà
    // décidé sous quel nom chaque détenteur se lit ; l'écran le rend, il ne le redevine pas.
    const named = (name: string, mention: string): UiComponent =>
        row(
            label("settings-shortcut-name", name),
            spacer(),
            label("settings-conflict-mention", mention),
            label("settings-shortcut-keys is-conflicting", conflict.keys),
        ).class("settings-shortcut");

    return block.add(
        named(conflict.holderLabel, "already assigned"),
        named(conflict.askedLabel, "just now"),
        row(
            label("settings-capture-warn-glyph", "△"),
            label("settings-conflict-diagnosis", conflict.diagnosis),
            spacer(),
            ...(conflict.give === null
                ? []
                : [
                      button(conflict.give)
                          .class("settings-conflict-give")
                          .onClick(() => {
                              actions.resolveConflict("give");
                          }),
                  ]),
            button(conflict.keep)
                .class("settings-conflict-keep")
                .onClick(() => {
                    actions.resolveConflict("keep");
                }),
        ).class("settings-conflict-verdict"),
    );
}
