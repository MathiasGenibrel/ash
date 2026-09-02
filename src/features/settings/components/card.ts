import { badge, button, choice, field, row, type UiComponent, type UiChild } from "@/shared/ui";

import type { TestDescription, ToolDeclaration } from "../contract";
import { ADAPTER_DEFAULT, describeReset, describeTool, type ToolHeading } from "../model";
import { presentVerification } from "../verification-state";
import { cell, label, spacer, tag } from "./atoms";
import { hooksNote, hooksRow, type HooksRowActions } from "./hooks-row";
import { testDetail, testRow, type TestDetailActions } from "./test-row";

/**
 * La carte d'une entrée déclarée : son en-tête, puis la grille `config` / `test` / `hooks`.
 *
 * Elle **assemble** — elle ne juge rien. Les deux teintes qu'elle porte viennent l'une du
 * registre (un doublon), l'autre de la vérification ; les quatre pastilles, la phrase de la
 * ligne `test` et les cinq états de la ligne `hooks` viennent du backend
 * ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface CardActions extends HooksRowActions, TestDetailActions {
    forgetTool(command: string): void;
    typePath(command: string, value: string): void;
    commitPath(command: string): void;
    selectAdapter(command: string, adapter: string): void;
    verifyTool(command: string): void;
    resetTool(command: string): void;
    undoReset(command: string): void;
}

/** Ce qu'une carte a besoin de savoir en plus de son entrée. */
export interface CardContext {
    readonly adapters: readonly string[];
    readonly tests: readonly TestDescription[];
    /** Ce qui est tapé dans le champ de chemin, tant que le backend n'a pas répondu. */
    readonly edits: ReadonlyMap<string, string>;
}

export function toolCard(
    tool: ToolDeclaration,
    context: CardContext,
    actions: CardActions,
): UiComponent {
    const shown = describeTool(tool);
    const state = presentVerification(tool.verification.state);
    // Deux teintes possibles, et le doublon l'emporte : une entrée valide qu'une autre
    // double n'écrira rien, et c'est ça qu'il faut voir en premier.
    const duplicated = tool.duplicates.length > 0 ? "is-duplicate" : "";

    const head = row(label("settings-card-name", shown.name)).class("settings-card-head");
    // La commande reste visible : c'est la clé du fichier et ce qu'on tape dans le shell.
    if (shown.badge !== null) head.add(badge(shown.badge).class("settings-card-badge"));
    head.add(
        adapterMenu(tool, context.adapters, actions),
        spacer(),
        verifyButton(tool, actions),
        resetButton(tool, actions),
        deleteButton(tool.command, actions),
    );

    // La grille `44px 1fr` de la maquette : un libellé, ce qu'il désigne, et des lignes de
    // détail à cellule de libellé vide.
    const body = tag("div", "settings-card-body").add(
        label("settings-card-key", "config"),
        pathField(tool, shown, context.edits, actions),
        ...wasRow(tool, actions),
        label("settings-card-key is-test", "test"),
        testRow(tool.verification, context.tests),
        ...testDetail(tool, actions),
        label("settings-card-key is-hooks", "hooks"),
        hooksRow(tool, actions),
        cell(),
        hooksNote(tool.hooks),
    );

    return tag("article", "settings-card", duplicated || state.cardClassName).add(head, body);
}

/**
 * Le menu d'adaptateur — **modifiable**, parce que le changer relance la séquence.
 *
 * Un contrôle qu'on peut bouger sans que rien ne re-juge dirait qu'Ash a accepté le nouvel
 * adaptateur. C'est la vérification qui le rend honnête, et un changement de menu ne peut
 * pas être suivi d'une frappe — il relance donc tout de suite, sans les 400 ms.
 */
function adapterMenu(
    tool: ToolDeclaration,
    adapters: readonly string[],
    actions: CardActions,
): UiComponent {
    return choice(`adapter for ${tool.command}`)
        .class("settings-card-adapter")
        .options(adapters, tool.adapter)
        .onSelect((adapter) => {
            actions.selectAdapter(tool.command, adapter);
        });
}

function verifyButton(tool: ToolDeclaration, actions: CardActions): UiComponent {
    // `cancel` n'annule rien tant que rien n'est annulable : la commande du test 4 est déjà
    // partie, et prétendre l'arrêter serait mentir. Elle relance, comme les autres.
    return button(presentVerification(tool.verification.state).action)
        .class("settings-button", "is-small")
        .onClick(() => {
            actions.verifyTool(tool.command);
        });
}

/**
 * Le `↺` : retour au **dernier dossier valide de cette entrée** (spec §9.1).
 *
 * Il reste visible même quand il ne peut rien faire, avec sa raison — la même règle que
 * celle du bouton d'installation, et pour la même raison : le masquer ferait croire que le
 * geste n'existe pas.
 */
function resetButton(tool: ToolDeclaration, actions: CardActions): UiComponent {
    const reset = describeReset(tool);
    const control = button("↺")
        .class("settings-icon-button", tool.resetFrom === null ? "" : "is-warning")
        .attr("aria-label", `reset ${tool.command}: ${reset.reason}`)
        .onClick(() => {
            actions.resetTool(tool.command);
        });
    if (!reset.enabled) control.disabled(reset.reason);
    // L'infobulle dit ce qui vient de se passer quand il s'est passé quelque chose, et la
    // raison sinon : juste après une réinitialisation, « reset just now » est l'information ;
    // le reste du temps, c'est `back to ~/.claude` — le dossier où le geste ramène, et le
    // seul endroit visible où il est nommé. Un bouton **allumé** ne passe pas par
    // `disabled(reason)`, donc sa raison ne voyage que s'il la pose lui-même : la poser
    // seulement quand il est éteint la ferait disparaître au moment précis où elle sert.
    return control.title(tool.resetFrom === null ? reset.reason : "reset just now");
}

function deleteButton(command: string, actions: CardActions): UiComponent {
    return button("✕")
        .class("settings-icon-button")
        .title("delete")
        .attr("aria-label", `delete ${command}`)
        .onClick(() => {
            actions.forgetTool(command);
        });
}

/**
 * Le chemin de configuration, **modifiable** : chaque frappe arme la relance de la séquence,
 * 400 ms plus tard — ou tout de suite sur `⏎`.
 *
 * `⏎` ne valide rien à la place de l'utilisateur
 * ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)) : il dit
 * seulement « j'ai fini de taper », et abrège l'attente.
 */
function pathField(
    tool: ToolDeclaration,
    shown: ToolHeading,
    edits: ReadonlyMap<string, string>,
    actions: CardActions,
): UiComponent {
    const invalid = tool.verification.state === "invalid";
    const duplicated = tool.duplicates.length > 0;
    // Le champ lui-même prend la teinte : c'est **le dossier** qui est en cause, dans les
    // deux cas, et c'est lui qu'on va corriger.
    const marking = duplicated ? "is-duplicate" : invalid ? "is-invalid" : "";

    const input = field(`configuration folder for ${tool.command}`)
        .class("settings-path")
        .value(edits.get(tool.command) ?? shown.path)
        // La chaîne affichée quand rien n'est saisi n'est pas un chemin : c'est ce que
        // l'absence veut dire, et le modèle en est le propriétaire. La mettre dans la valeur
        // en ferait un dossier nommé « adapter default ».
        .placeholder(ADAPTER_DEFAULT)
        .focusKey(pathFocusKey(tool.command))
        .onInput((value) => {
            actions.typePath(tool.command, value);
        })
        .onSubmit(() => {
            actions.commitPath(tool.command);
        })
        .onBlur(() => {
            actions.commitPath(tool.command);
        });

    const line = row(input).class("settings-field", marking);

    if (duplicated) {
        // L'étiquette est sur **les deux** cartes, pas seulement sur celle qu'on vient de
        // toucher (spec §9.1) : c'est le registre qui l'a posée sur chacune.
        line.add(label("settings-duplicate-tag", `duplicate · also ${tool.duplicates.join(", ")}`));
    }

    if (!tool.verified) {
        // La pastille « modifié, non enregistré » de la maquette. Ce qu'elle dit a changé
        // avec la persistance : la déclaration, elle, est bien gardée dans
        // `~/.ash/tools.json` dès qu'elle est faite — ce qui n'est pas écrit tant que les
        // quatre tests n'ont pas parlé, c'est le bloc de hooks dans le fichier de
        // l'utilisateur ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)). C'est la
        // seule écriture que la vérification garde, et donc la seule qu'elle puisse annoncer.
        const unsaved = "not verified — no hooks written for this entry";
        line.add(tag("span", "settings-unsaved").title(unsaved).attr("aria-label", unsaved));
    }

    return line;
}

/** La clé de focus du champ de chemin d'une entrée — la vue la retient d'un rendu à l'autre. */
export function pathFocusKey(command: string): string {
    return `path:${command}`;
}

/**
 * La ligne `was` — elle n'existe **que** juste après une réinitialisation.
 *
 * À ne pas confondre avec l'étiquette de doublon, qui existe dès que deux entrées
 * collisionnent : ce sont deux conditions indépendantes (§7.3).
 */
function wasRow(tool: ToolDeclaration, actions: CardActions): readonly UiChild[] {
    if (tool.resetFrom === null) return [];

    const line = row(label("settings-was-path", tool.resetFrom)).class("settings-was");
    line.add(
        button("restore")
            .class("settings-link")
            .onClick(() => {
                actions.undoReset(tool.command);
            }),
    );
    return [label("settings-card-key", "was"), line];
}
