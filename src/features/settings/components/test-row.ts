import { badge, button, row, text, type UiComponent, type UiChild } from "@/shared/ui";

import type { FixAction, TestDescription, ToolDeclaration, Verification } from "../contract";
import { degradedFixSubject, describeStop } from "../model";
import { testTileClass, testTileLabel, verificationGlyph } from "../verification-state";
import { cell, label, para, spacer } from "./atoms";
import { degradedRow } from "./degraded";

/**
 * La ligne `test` — le glyphe, la phrase, les quatre pastilles, et où la chaîne s'est
 * arrêtée.
 *
 * La même dans une carte et dans le formulaire d'ajout : c'est la même question posée à une
 * entrée déclarée et à une saisie, et une seconde rangée écrite à côté finirait par
 * répondre autrement.
 *
 * `stopped at test n` ne se déduit pas de `stoppedAt` : la séquence le pose **aussi** sur
 * une réserve, et l'écrire là ferait lire un échec là où le dossier a été reconnu. La règle
 * est dans `describeStop`, et elle n'est pas rejouée ici.
 */
export function testRow(
    verification: Verification,
    tests: readonly TestDescription[],
): UiComponent {
    const line = row(
        verificationGlyph(verification.state, 13),
        label("settings-test-summary", verification.summary),
        spacer(),
        tileRow(verification, tests),
    ).class("settings-test");

    const stop = describeStop(verification);
    if (stop !== null) line.add(label("settings-stopped", stop));
    return line;
}

/**
 * Les quatre pastilles, dans l'ordre où les tests se lancent.
 *
 * Les libellés viennent du **contrat** : les tests existent en Rust, donc c'est là qu'ils
 * se nomment. Une pastille sans réponse est `pending` — jamais rien, sans quoi la rangée
 * changerait de longueur selon l'avancement.
 */
function tileRow(verification: Verification, tests: readonly TestDescription[]): UiComponent {
    const tiles = tests.map((test, index) => {
        const outcome = verification.tests[index] ?? "pending";
        const said = testTileLabel(outcome, test);
        // Le chiffre seul ne dit rien à un lecteur d'écran : ni de quel test il s'agit, ni
        // ce qu'il a donné.
        return badge(String(test.number))
            .class(testTileClass(outcome))
            .title(said)
            .attr("aria-label", said);
    });
    return row(...tiles).class("settings-tiles");
}

/** Ce que le détail d'un résultat sait demander — un seul geste, et il n'écrit rien. */
export interface TestDetailActions {
    applyFix(command: string, fix: FixAction): void;
    /** Ramener le curseur dans le champ de chemin — le seul geste que la vue seule sait faire. */
    focusPath(command: string): void;
}

/**
 * Ce qu'un état ajoute sous la ligne `test` — des lignes de grille à **cellule de libellé
 * vide**, donc rangées sous elle.
 */
export function testDetail(tool: ToolDeclaration, actions: TestDetailActions): readonly UiChild[] {
    const { verification } = tool;
    const rows: UiChild[] = [];

    if (verification.launched !== null) {
        // La commande réellement lancée. Ce qui part sans qu'on l'ait tapé doit être
        // lisible : c'est la contrepartie du fait qu'Ash lance un programme tout seul.
        rows.push(cell(), para("settings-inset is-command", text(verification.launched)));
    }

    if (verification.detail !== null) {
        rows.push(
            cell(),
            para(
                "settings-recall",
                text("expected: "),
                label("settings-recall-expected", verification.detail.expected),
                text(` — found: ${verification.detail.found}`),
            ),
        );
    }

    if (verification.fix !== null) {
        rows.push(cell(), fixInset(tool, actions));
        // `generic` est un mode dégradé, et l'écran le dit **avant** qu'on l'applique : le
        // bouton `apply` juste au-dessus est celui qui y bascule.
        const degrading = degradedFixSubject(tool);
        if (degrading !== null) rows.push(...degradedRow(degrading));
    }

    return rows;
}

/** La correction proposée : la question, et ce qu'on peut en faire. */
function fixInset(tool: ToolDeclaration, actions: TestDetailActions): UiComponent {
    const fix = tool.verification.fix;
    const box = row(label("settings-fix-question", fix?.question ?? ""), spacer()).class(
        "settings-fix",
    );

    const apply = fix?.apply ?? null;
    if (apply !== null) {
        box.add(
            button("apply")
                .class("settings-button", "is-primary", "is-small")
                .onClick(() => {
                    actions.applyFix(tool.command, apply);
                }),
        );
    }

    // Toujours là, et secondaire : quand rien ne peut être appliqué, c'est la seule chose
    // qui reste à faire — et elle ne se fait pas à la place de l'utilisateur
    // ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
    return box.add(
        button("choose another folder…")
            .class("settings-button", "is-small")
            .onClick(() => {
                actions.focusPath(tool.command);
            }),
    );
}
