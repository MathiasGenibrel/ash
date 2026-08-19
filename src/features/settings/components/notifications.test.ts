import { describe, expect, it } from "bun:test";

import type { AgentState } from "@/shared/ipc";
import { findAll, plainText, type UiChild, type UiElementNode } from "@/shared/ui";

import { aNotificationsReport } from "../builders";
import { notificationsSection, type NotificationsActions } from "./notifications";

function said(children: readonly UiChild[]): string {
    return children.map(plainText).join("");
}

/** Les boutons de la section — les trois interrupteurs, et rien d'autre. */
function buttons(children: readonly UiChild[]): UiElementNode[] {
    return children.flatMap((child) => findAll(child, "ui-button"));
}

const IDLE: NotificationsActions = { setNotification: () => undefined };

describe("la section notifications de la fenêtre de réglages", () => {
    it("Given macOS refused the permission, when the section is composed, then it says so and gives the path to grant it", () => {
        // Given — la dernière puce de la spec §8. Sans elle, une notification qui n'arrive
        // jamais est indiscernable d'un agent qui n'attend rien : l'utilisateur conclut
        // qu'Ash ne marche pas, et le seul critère de sortie du jalon tombe sans un bruit
        const refused = aNotificationsReport("denied", {
            summary: "macOS notifications are not allowed",
            note: "an agent waiting for an answer while ash is behind another window will go unnoticed until you come back. grant them here:",
        });

        // When
        const composed = notificationsSection(refused, IDLE);

        // Then
        expect(said(composed)).toContain("macOS notifications are not allowed");
        expect(said(composed)).toContain("System Settings ▸ Notifications ▸ ash");
    });

    it("Given macOS discloses nothing about the permission, when the section is composed, then the path is still shown", () => {
        // Given — le cas réel hors application empaquetée, et celui où le chemin sert le
        // plus : Ash ne peut pas dire si la permission manque, donc c'est l'utilisateur qui
        // doit pouvoir aller vérifier. Ne montrer le chemin que sur un refus le cacherait au
        // seul moment où personne d'autre ne peut trancher
        const undisclosed = aNotificationsReport();

        // When
        const composed = notificationsSection(undisclosed, IDLE);

        // Then
        expect(said(composed)).toContain("System Settings ▸ Notifications ▸ ash");
    });

    it("Given a backend where waiting is off and done is on, when the section is composed, then the switches show that instead of the defaults", () => {
        // Given — les défauts de la spec §8 sont `waiting` et `error` allumés, `done`
        // éteint. Ce `Given` décrit l'exact contraire pour deux d'entre eux : c'est ce qui
        // prouve que la vue **rend** la position que le backend détient au lieu d'en porter
        // une seconde, qui redessinerait un réglage que l'utilisateur a changé
        const report = aNotificationsReport("granted", {
            switches: [
                { state: "waiting", enabled: false, means: "an agent is waiting for an answer" },
                { state: "error", enabled: true, means: "an agent failed" },
                { state: "done", enabled: true, means: "an agent finished" },
            ],
        });

        // When
        const positions = buttons(notificationsSection(report, IDLE)).map((one) => [
            one.attrs["aria-label"],
            one.attrs["aria-pressed"],
        ]);

        // Then
        expect(positions).toEqual([
            ["waiting notifications", "false"],
            ["error notifications", "true"],
            ["done notifications", "true"],
        ]);
    });

    it("Given a switch clicked in the screen, when the click is played, then it asks the backend for the opposite and changes nothing itself", () => {
        // Given — l'interrupteur est la surface d'un choix que `features::agents` détient et
        // consulte au moment de poster (ADR-0009). Une bascule appliquée ici afficherait
        // `off` sur un état qui continuerait d'interrompre — la pire des deux erreurs
        // possibles, puisqu'elle se découvre par une bannière
        const asked: [AgentState, boolean][] = [];
        const shown = aNotificationsReport("granted");
        const composed = notificationsSection(shown, {
            setNotification: (state, enabled) => asked.push([state, enabled]),
        });

        // When — `waiting` est allumé par défaut : le clic doit demander à l'éteindre
        const waiting = buttons(composed).find(
            (one) => one.attrs["aria-label"] === "waiting notifications",
        );
        waiting?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(asked).toEqual([["waiting", false]]);
        expect(shown.switches[0]?.enabled).toBe(true);
    });

    it("Given the backend has not answered yet, when the section is composed, then it says what it is waiting for rather than showing switches it does not hold", () => {
        // Given — la navigation traverse la section (`⌥↓`) : un panneau vide se lirait comme
        // une panne. Et trois interrupteurs dessinés à leurs défauts avant la réponse
        // feraient lire à qui a coupé `waiting` un réglage qui n'est pas le sien
        const composed = notificationsSection(null, IDLE);

        // Then
        expect(said(composed)).toContain("asking macOS…");
        expect(buttons(composed)).toEqual([]);
    });

    it("Given any state of the section, when it is composed, then its content starts under the title instead of being centred", () => {
        // Given — le corps centré était celui des sections vides, et le critère de l'issue
        // demande que le contenu commence sous son titre
        const composed = notificationsSection(aNotificationsReport(), IDLE);

        // When
        const bodies = composed.flatMap((child) => findAll(child, "settings-body"));

        // Then
        expect(bodies).toHaveLength(1);
        expect(bodies[0]?.classes).not.toContain("is-empty");
    });
});
