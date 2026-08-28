import { button, row, text, type UiChild } from "@/shared/ui";

import type { UsageReport } from "../contract";
import { label, para, tag } from "./atoms";
import { sectionHeader } from "./chrome";

/** Le seul geste de la section — il part au backend, et n'y revient qu'en réponse. */
export interface UsageActions {
    setPolling(enabled: boolean): void;
}

/**
 * La section `usage` de la fenêtre — **ce qu'Ash appelle, et comment l'en empêcher**.
 *
 * [ADR-0016](../../../../docs/adr/0016-ash-sort-sur-le-reseau.md), condition 3 : « en succès,
 * l'utilisateur doit pouvoir savoir qu'Ash appelle, et **le couper** ». Les deux moitiés sont
 * là, dans cet ordre : l'hôte est nommé en toutes lettres avant l'interrupteur qui le coupe.
 *
 * **Rien n'est décidé ici, et rien n'est déclenché non plus.** Les phrases, l'adresse, la
 * position de l'interrupteur et l'issue de la lecture du trousseau viennent toutes du
 * backend ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — et l'ouvrir ne
 * lit aucun trousseau : la section rapporte un souvenir que le fil de fond a laissé. Une
 * section qui irait chercher ferait surgir un dialogue macOS sur un chemin de rendu, ce que
 * la condition 1 de la même ADR interdit.
 *
 * Elle ne porte **aucun bouton vers Trousseaux d'accès**, exactement comme la section
 * `notifications` n'en porte aucun vers les Réglages Système : ouvrir un panneau du système à
 * la place de l'utilisateur est un geste qu'Ash n'a pas à faire, et le chemin se lit en trois
 * mots.
 */
export function usageSection(
    report: UsageReport | null,
    actions: UsageActions,
): readonly UiChild[] {
    const head = sectionHeader("usage", null, []);
    const body = tag("div", "settings-body");

    if (report === null) {
        // L'aller-retour est immédiat — il ne traverse ni le réseau ni le trousseau —, mais
        // un panneau muet ferait quand même croire à une panne le temps qu'il revienne.
        return [head, body.add(para("settings-empty-prose", text("reading what ash knows…")))];
    }

    body.add(
        // L'hôte d'abord : on ne coupe pas quelque chose dont on ignore ce que c'est.
        para(
            "settings-usage-endpoint",
            text("session and weekly quotas come from "),
            label("settings-permission-path", report.endpoint),
        ),
        pollingRow(report, actions),
        para(
            "settings-notifications-permission",
            label(`settings-permission is-${report.token}`, report.summary),
        ),
        para(
            "settings-note",
            text(report.note),
            text(" "),
            label("settings-permission-path", report.path),
        ),
        // La limite des deux comptes (ADR-0017, conséquences). Elle est en bas parce qu'elle
        // ne demande aucun geste : c'est une chose qu'Ash ne sait pas et préfère dire.
        para("settings-note", text(report.accounts)),
    );

    return [head, body];
}

/**
 * L'interrupteur d'ADR-0016 : sa position, et ce qu'il coupe.
 *
 * Le bouton dit **la position**, `on` ou `off`, et non le geste — la règle de la section
 * `notifications`, et pour la même raison : un libellé qui dirait « turn off » ferait lire
 * l'inverse de l'état à qui parcourt la fenêtre des yeux. `aria-pressed` porte la même chose
 * pour qui ne voit pas l'écran.
 */
function pollingRow(report: UsageReport, actions: UsageActions): UiChild {
    return tag("div", "settings-notification-row").add(
        row(
            label("settings-notified-state", "quotas"),
            // La seule phrase de la section qui ne vienne pas du backend, et elle ne décrit
            // aucune règle : elle nomme ce que le bouton d'à côté commande. Ce que couper
            // *coûte* — la valeur disparaît de la barre d'état — est en Rust, dans la note.
            label("settings-notification-means", "ask the host how much is left"),
            button(report.polling ? "on" : "off")
                .class("settings-button", report.polling ? "is-primary" : "")
                .attr("aria-pressed", String(report.polling))
                .attr("aria-label", "usage quota calls")
                .onClick(() => {
                    actions.setPolling(!report.polling);
                }),
        ).class("settings-notification-line"),
    );
}
