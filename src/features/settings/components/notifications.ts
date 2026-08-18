import { presentAgentState } from "@/shared/agent-state";
import { button, row, text, type UiChild } from "@/shared/ui";

import type { AgentState } from "@/shared/ipc";

import type { NotificationsReport, NotificationSwitch } from "../contract";
import { label, para, tag } from "./atoms";
import { sectionHeader } from "./chrome";

/** Le seul geste de la section — il part au backend, et n'y revient qu'en réponse. */
export interface NotificationsActions {
    setNotification(state: AgentState, enabled: boolean): void;
}

/**
 * La section `notifications` de la fenêtre (spec §8, et le `[notifications]` de la spec §9).
 *
 * **Rien n'est décidé ici.** L'état de l'autorisation, sa phrase, sa conséquence, le chemin
 * pour l'accorder, les états qui peuvent interrompre et la position de leurs interrupteurs
 * viennent tous du backend
 * ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — les trois derniers
 * d'`agents`, qui possède ce que « notifier » veut dire, et qui est aussi le seul à consulter
 * ces interrupteurs au moment de poster une bannière. C'est pour ça qu'ils coupent vraiment :
 * une bannière sort quand Ash est en arrière-plan, donc un filtre posé ici ne pourrait cacher
 * que ce qui est déjà passé devant les yeux de l'utilisateur.
 *
 * Ce que cette section garde, et qui se vérifie ici : **le chemin est toujours montré**. La
 * puce de la spec §8 ne demande pas seulement que l'état soit visible, elle demande le
 * chemin avec — un utilisateur qui lit « non accordée » sans savoir où aller n'apprend que
 * sa panne.
 *
 * Elle ne porte **aucun bouton vers macOS**, et c'est cohérent avec le reste : ouvrir le
 * panneau des Réglages Système à la place de l'utilisateur serait un geste qu'Ash n'a pas à
 * faire, et le chemin se lit en trois mots.
 */
export function notificationsSection(
    report: NotificationsReport | null,
    actions: NotificationsActions,
): readonly UiChild[] {
    const head = sectionHeader("notifications", null, []);
    const body = tag("div", "settings-body");

    if (report === null) {
        // L'aller-retour est immédiat en pratique ; un panneau muet ferait quand même
        // croire à une panne le temps qu'il revienne.
        return [head, body.add(para("settings-empty-prose", text("asking macOS…")))];
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
        ...report.switches.map((toggle) => switchRow(toggle, actions)),
        // La seule phrase de la section qui ne vienne pas du backend, et elle ne décrit aucune
        // règle d'état : c'est le rappel de la condition que les trois interrupteurs partagent
        // (spec §8), celle qu'aucun d'eux ne peut lever.
        para(
            "settings-note",
            text("nothing interrupts you while ash is the window you are looking at."),
        ),
    );

    return [head, body];
}

/**
 * Une ligne d'interrupteur : l'état avec le glyphe que la sidebar lui donne, ce qu'il veut
 * dire, et le bouton qui le commande.
 *
 * La présentation est celle de `shared/agent-state` — la même que la sidebar et la ligne de
 * statut — parce que reconnaître ici le `❯` qu'on verra là-bas est tout l'intérêt de le
 * montrer.
 *
 * Le bouton dit **la position**, `on` ou `off`, et non le geste : `aria-pressed` porte la
 * même chose pour qui ne voit pas l'écran. Un libellé qui dirait « turn off » ferait lire
 * l'inverse de l'état à qui parcourt la liste des yeux.
 */
function switchRow(toggle: NotificationSwitch, actions: NotificationsActions): UiChild {
    const shown = presentAgentState(toggle.state);
    return tag("div", "settings-notification-row").add(
        row(
            label("settings-notified-state", `${shown.glyph} ${shown.label}`),
            label("settings-notification-means", toggle.means),
            button(toggle.enabled ? "on" : "off")
                .class("settings-button", toggle.enabled ? "is-primary" : "")
                .attr("aria-pressed", String(toggle.enabled))
                .attr("aria-label", `${shown.label} notifications`)
                .onClick(() => {
                    actions.setNotification(toggle.state, !toggle.enabled);
                }),
        ).class("settings-notification-line"),
    );
}
