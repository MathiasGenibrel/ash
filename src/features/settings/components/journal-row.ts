import { button, row, type UiChild } from "@/shared/ui";

import type { JournalReport } from "../contract";
import { label, tag } from "./atoms";

/**
 * Le journal d'attribution, et le geste qui l'efface (spec §10,
 * [ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md)).
 *
 * Il est sous la liste des outils, à côté de « retirer ash de tous les fichiers », parce que
 * les deux répondent à la même question — *que reste-t-il d'ash sur cette machine ?* — et que
 * la spec §10 les met sur la même page. Il n'en dépend pourtant pas : le journal se remplit
 * sans qu'aucun outil soit déclaré ni instrumenté, l'attribution ne demandant aucun hook.
 *
 * **Il ne montre aucune ligne du fichier.** Le journal contient des prompts ; un écran qui
 * les affiche est un endroit de plus où ils passent, et Ash n'en a pas besoin pour dire ce
 * qu'il pèse.
 *
 * **Un seul temps, contrairement au retrait des hooks**, et c'est une différence de nature,
 * pas de soin : le retrait touche des fichiers de l'utilisateur, donc il s'annonce et se
 * montre avant d'écrire. Ici, Ash efface **ce qu'il a écrit lui-même**, dans son propre
 * dossier — et ce que le clic emporte est écrit sur le bouton.
 */
export interface JournalActions {
    purgeJournal(): void;
}

export function journalRow(report: JournalReport | null, actions: JournalActions): UiChild {
    const section = tag("section", "settings-journal");

    if (report === null) {
        // L'aller-retour est immédiat en pratique ; un bouton qui prétendrait un compte
        // avant de l'avoir lu proposerait d'effacer on ne sait quoi.
        return section.add(label("settings-uninstall-note", "reading the journal…"));
    }

    const purge = button(report.entries === 0 ? "purge" : `purge ${report.summary}`)
        .class("settings-button", report.entries === 0 ? "" : "is-danger")
        .attr("aria-label", "purge the attribution journal")
        .onClick(() => {
            actions.purgeJournal();
        });
    // Rien à effacer : le bouton reste **visible et éteint, avec sa raison**, comme celui de
    // l'installation des hooks. Le faire disparaître ferait croire que la promesse de la
    // spec §10 dépend de ce qu'il y a dans le fichier.
    if (report.entries === 0) purge.disabled("ash has not attributed any commit yet");

    return section.add(
        row(
            label("settings-journal-name", "attribution journal"),
            label("settings-uninstall-note", `${report.summary} · ${report.note}`),
            purge,
        ).class("settings-uninstall-row"),
        row(label("settings-permission-path", report.path)).class("settings-journal-path"),
    );
}
