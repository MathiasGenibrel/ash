import { presentAgentState } from "@/shared/agent-state";
import { text, type UiChild } from "@/shared/ui";

import type { NotificationsReport } from "../contract";
import { label, para, tag } from "./atoms";
import { sectionHeader } from "./chrome";

/**
 * La section `notifications` de la fenêtre (spec §8).
 *
 * **Rien n'est décidé ici.** L'état de l'autorisation, sa phrase, sa conséquence, le chemin
 * pour l'accorder et les deux états qui interrompent viennent tous du backend
 * ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — les seconds
 * d'`agents`, qui possède ce que « notifier » veut dire.
 *
 * Ce que cette section garde, et qui se vérifie ici : **le chemin est toujours montré**. La
 * puce de la spec §8 ne demande pas seulement que l'état soit visible, elle demande le
 * chemin avec — un utilisateur qui lit « non accordée » sans savoir où aller n'apprend que
 * sa panne.
 *
 * Elle ne porte **aucun bouton**, et c'est cohérent avec le reste : ouvrir le panneau de
 * macOS à la place de l'utilisateur serait un geste qu'Ash n'a pas à faire, et le chemin se
 * lit en trois mots.
 */
export function notificationsSection(report: NotificationsReport | null): readonly UiChild[] {
    const body = tag("div", "settings-body");

    if (report === null) {
        // L'aller-retour est immédiat en pratique ; un panneau muet ferait quand même
        // croire à une panne le temps qu'il revienne.
        return [
            sectionHeader("notifications", null, []),
            body.add(para("settings-empty-prose", text("asking macOS…"))),
        ];
    }

    body.add(
        para(
            "settings-notifications-permission",
            label(`settings-permission is-${report.permission}`, report.summary),
        ),
        para(
            "settings-note",
            text(report.note),
            text(" "),
            label("settings-permission-path", report.path),
        ),
        para("settings-note", text("ash interrupts you for: "), ...interrupting(report)),
    );

    return [sectionHeader("notifications", null, []), body];
}

/**
 * Les états qui interrompent, nommés avec le glyphe que la sidebar leur donne.
 *
 * La présentation est celle de `shared/agent-state` — la même que la sidebar et la ligne de
 * statut — parce que reconnaître ici le `❯` qu'on verra là-bas est tout l'intérêt de le
 * montrer.
 */
function interrupting(report: NotificationsReport): readonly UiChild[] {
    return report.notified.flatMap((state, index) => {
        const shown = presentAgentState(state);
        const named = label("settings-notified-state", `${shown.glyph} ${shown.label}`);
        return index === 0 ? [named] : [text(" · "), named];
    });
}
