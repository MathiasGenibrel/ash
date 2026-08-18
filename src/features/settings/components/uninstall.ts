import { banner, button, column, row, text, type UiChild, type UiComponent } from "@/shared/ui";

import type { PlannedRemoval, RemovalPlan, RemovalReport, RemovedFile } from "../contract";
import { label, para, tag } from "./atoms";
import { sectionHeader } from "./chrome";
import { diffView } from "./diff-view";

/**
 * « Retirer Ash de tous les fichiers » — le geste de la spec §10, et ses **deux temps**.
 *
 * Le premier n'écrit rien : il nomme les fichiers, compte les entrées, montre le diff de ce
 * qui partirait, et signale ce qu'une main a touché. Le second est le clic de l'utilisateur.
 * Les séparer n'est pas une précaution d'interface, c'est la règle du produit — « Ash
 * n'écrit que ce qui lui appartient ; sauvegarde, jamais silencieux » — appliquée au geste
 * qui touche le plus de fichiers d'un coup.
 *
 * **Rien n'est décidé ici.** Le compte, les phrases, ce qui est conservé et l'issue de
 * chaque fichier viennent du backend ; l'écran les rend
 * ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). C'est la même
 * discipline que l'écran du diff, et pour la même raison : la vue qui recompose une phrase
 * du backend est celle qui finit par en supprimer une.
 */
export interface UninstallActions {
    /** Demande l'annonce — **n'écrit rien**. */
    planRemoval(): void;
    /** Le clic qui écrit, pris devant l'annonce. */
    removeEverything(): void;
    closeRemoval(): void;
}

/**
 * L'entrée du geste, sous la liste : un bouton, et ce qu'il fera.
 *
 * Il est **hors de l'en-tête** : `add` et `re-verify all` sont les gestes de tous les jours,
 * celui-ci se prend une fois. Le mettre à côté d'eux le rendrait cliquable par habitude.
 */
export function uninstallRow(actions: UninstallActions): UiComponent {
    return tag("section", "settings-uninstall").add(
        row(
            button("remove ash from every file")
                .class("settings-button", "is-danger")
                .onClick(() => {
                    actions.planRemoval();
                }),
            label(
                "settings-uninstall-note",
                "ash shows what it would take out of each file first. nothing is written until you say so.",
            ),
        ).class("settings-uninstall-row"),
    );
}

/** Ce que l'écran de désinstallation montre : l'annonce, ou le compte rendu. */
export type RemovalStage =
    | { readonly step: "asked"; readonly plan: RemovalPlan }
    | { readonly step: "done"; readonly report: RemovalReport };

/**
 * L'écran de désinstallation — il **remplace la liste**, comme celui du diff (§4.4).
 *
 * Ni une modale ni un panneau : ce qui va toucher plusieurs fichiers de l'utilisateur se lit
 * en entier, pas dans une boîte qu'on chasse d'un `esc`.
 */
export function uninstallScreen(
    stage: RemovalStage,
    actions: UninstallActions,
): readonly UiChild[] {
    const back = button(stage.step === "asked" ? "← cancel" : "← back to the list")
        .class("settings-button")
        .onClick(() => {
            actions.closeRemoval();
        });

    return stage.step === "asked"
        ? [
              sectionHeader("remove ash", stage.plan.summary, [back]),
              announcement(stage.plan, actions),
          ]
        : [sectionHeader("remove ash", stage.report.summary, [back]), account(stage.report)];
}

/** Le premier temps : ce qui partirait, fichier par fichier, et le bouton qui l'écrit. */
function announcement(plan: RemovalPlan, actions: UninstallActions): UiChild {
    const body = tag("div", "settings-body", "is-conflict");

    if (plan.files.length === 0) {
        // Le bouton reste **visible et éteint, avec sa raison** — la discipline de la
        // maquette : le masquer ferait croire que la désinstallation n'existe pas.
        body.add(
            para("settings-uninstall-empty", text(plan.summary)),
            ...kept(plan.kept),
            row(
                button("remove ash's entries")
                    .class("settings-button")
                    .disabled(plan.summary),
            ).class("settings-choice"),
        );
        return body;
    }

    if (plan.handEdited) {
        // Spec §10 : « si un bloc géré a été modifié à la main, Ash ne réécrit pas
        // silencieusement — il signale, propose le diff, et demande ». Le retrait emporte
        // l'entrée éditée, marqueur oblige : le taire serait la seule vraie faute.
        body.add(
            banner(
                "one of these entries was edited by hand. removing it takes those edits with it — the diff below shows exactly what goes.",
                "warning",
            ).class("settings-banner", "is-caveat"),
        );
    }

    body.add(...plan.files.map(plannedFile), ...kept(plan.kept));
    body.add(
        row(
            button("remove ash's entries")
                .class("settings-button", "is-danger")
                .onClick(() => {
                    actions.removeEverything();
                }),
            label(
                "settings-choice-note",
                "a .bak is written before each file is touched, and only the entries carrying ash's marker are taken out.",
            ),
        ).class("settings-choice"),
    );
    return body;
}

/** Un fichier de l'annonce : où, pour qui, combien, et le diff de ce qui part. */
function plannedFile(planned: PlannedRemoval): UiChild {
    const head = row(
        label("settings-locate-path", planned.file),
        label(
            "settings-uninstall-count",
            `${entryCount(planned.entries)} · ${planned.commands.join(", ")}`,
        ),
    ).class("settings-uninstall-head");

    const file = column(head).class("settings-uninstall-file");
    if (planned.deletesTheFile) {
        file.add(
            para(
                "settings-uninstall-fate",
                text("this file carried nothing else: ash created it, and it goes with them."),
            ),
        );
    }
    return file.add(diffView(planned.diff));
}

/** Le second temps : ce que chaque fichier est devenu, dans les mots du backend. */
function account(report: RemovalReport): UiChild {
    return tag("div", "settings-body", "is-conflict").add(
        ...report.files.map(removedFile),
        ...kept(report.kept),
    );
}

function removedFile(done: RemovedFile): UiChild {
    return row(
        label("settings-locate-path", done.file),
        label("settings-uninstall-count", fate(done)),
    ).class("settings-uninstall-head");
}

/**
 * Ce qu'un fichier est devenu, en une phrase.
 *
 * Le `switch` couvre les quatre issues **sans `default`** : le jour où le backend en ajoute
 * une, `bun run typecheck` échoue ici plutôt que l'écran n'en montre aucune.
 */
function fate(done: RemovedFile): string {
    switch (done.outcome.kind) {
        case "removed":
            return `${entryCount(done.entries)} removed`;
        case "removedTheFile":
            return `${entryCount(done.entries)} removed · the file went with them`;
        case "nothingLeft":
            return "nothing of ash left here — untouched";
        case "refused":
            return `left untouched — ${done.outcome.why}`;
    }
}

/** Ce que le geste ne fait pas, tel que le backend l'écrit. */
function kept(sentences: readonly string[]): readonly UiChild[] {
    return sentences.map((sentence) => para("settings-uninstall-kept", text(sentence)));
}

function entryCount(entries: number): string {
    return `${entries} ${entries === 1 ? "entry" : "entries"}`;
}
