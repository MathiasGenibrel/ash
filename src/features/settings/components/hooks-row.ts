import { button, row, text, type UiComponent } from "@/shared/ui";

import type { HooksReport, ToolDeclaration } from "../contract";
import { hookActionLabel } from "../model";
import { hooksGlyph, presentHooks } from "../verification-state";
import { label, lineBreak, para, spacer } from "./atoms";

/**
 * La ligne `hooks` d'une carte, dans **les cinq états** que le backend distingue.
 *
 * Rien n'est décidé ici : l'état, la phrase, le fichier, l'action et le fait que le bouton
 * soit allumé viennent tous de `tool.hooks`, calculé en Rust — c'est la règle qui autorise
 * Ash à écrire chez l'utilisateur, et elle n'a qu'un propriétaire
 * ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
 *
 * Ce que ce composant tranche, et qui se vérifie ici : **quel geste le bouton déclenche**.
 * Les trois ne coûtent pas la même chose — deux écrivent dans un fichier de l'utilisateur,
 * `see the diff` n'écrit rien et ouvre un écran. Les confondre serait la seule faute de
 * cette ligne qu'on ne pourrait pas défaire.
 */
export interface HooksRowActions {
    installHooks(command: string): void;
    removeHooks(command: string): void;
    openConflict(command: string): void;
}

export function hooksRow(tool: ToolDeclaration, actions: HooksRowActions): UiComponent {
    const { hooks } = tool;
    const shown = presentHooks(hooks.state);
    const line = row(hooksGlyph(hooks.state), label("settings-hooks-reason", hooks.summary)).class(
        "settings-hooks",
        shown.rowClassName,
    );

    // Le fichier en pastille — ou pas, et c'est la table de `verification-state` qui le
    // dit : un refus `blocked` nomme déjà le fichier dans sa phrase.
    if (hooks.file !== null && shown.showsFile) {
        line.add(label("settings-hooks-file", hooks.file));
    }

    // Un bouton principal, toujours présent, éteint quand il ne peut rien faire. La raison
    // est celle du backend : c'est lui qui sait pourquoi la ligne ne laisse rien faire.
    const action = button(hookActionLabel(hooks.action))
        .class("settings-button", hooks.action === "remove" ? "" : "is-primary")
        .onClick(() => {
            if (hooks.action === "remove") actions.removeHooks(tool.command);
            else if (hooks.action === "seeTheDiff") actions.openConflict(tool.command);
            else actions.installHooks(tool.command);
        });
    if (!hooks.enabled) action.disabled(hooks.summary);

    line.add(spacer());

    // **Le diff s'ouvre avant toute écriture, pas seulement devant un conflit.** Un bouton
    // qui va écrire dans le fichier de quelqu'un doit pouvoir dire ce qu'il y écrira, et le
    // backend porte ce diff dès qu'il y a quelque chose à écrire
    // ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), amendement du 2026-08-12).
    if (hooks.diff !== null && hooks.action !== "seeTheDiff") {
        line.add(
            button(hookActionLabel("seeTheDiff"))
                .class("settings-button")
                .onClick(() => {
                    actions.openConflict(tool.command);
                }),
        );
    }

    return line.add(action);
}

/**
 * La prose sous la ligne `hooks` : **la conséquence**, et la copie annoncée avant l'action.
 *
 * La phrase vient du backend, qui seul sait dans quel état est le fichier. Ce que l'écran
 * ajoute est la promesse de sauvegarde, écrite **avant** le geste et pas après (§4.2).
 */
export function hooksNote(hooks: HooksReport): UiComponent {
    const note = para("settings-hooks-note", text(hooks.note));
    if (hooks.backup !== null) {
        note.add(lineBreak(), text(`before writing: ${hooks.backup}`));
    }
    return note;
}
