import { describe, expect, it } from "bun:test";

import { findAll, plainText, type UiChild } from "@/shared/ui";

import { aNotificationsReport } from "../builders";
import { notificationsSection } from "./notifications";

function said(children: readonly UiChild[]): string {
    return children.map(plainText).join("");
}

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
        const composed = notificationsSection(refused);

        // Then
        expect(said(composed)).toContain("macOS notifications are not allowed");
        expect(said(composed)).toContain("System Settings ▸ Notifications ▸ ash");
    });

    it("Given macOS discloses nothing about the permission, when the section is composed, then the path is still shown", () => {
        // Given — le cas réel aujourd'hui, et celui où le chemin sert le plus : Ash ne peut
        // pas dire si la permission manque, donc c'est l'utilisateur qui doit pouvoir aller
        // vérifier. Ne montrer le chemin que sur un refus le cacherait au seul moment où
        // personne d'autre ne peut trancher
        const undisclosed = aNotificationsReport();

        // When
        const composed = notificationsSection(undisclosed);

        // Then
        expect(said(composed)).toContain("System Settings ▸ Notifications ▸ ash");
    });

    it("Given a backend that interrupts for three states, when the section is composed, then it names those three and not the two it was written with", () => {
        // Given — `done` ne notifie pas (spec §8), et le backend l'exclut. Ce `Given`
        // décrit un backend qui l'inclurait : c'est ce qui prouve que la vue **rend** la
        // liste au lieu d'en porter une seconde, qui finirait par promettre une bannière
        // qu'Ash n'envoie pas
        const report = aNotificationsReport("undisclosed", {
            notified: ["waiting", "error", "done"],
        });

        // When
        const composed = notificationsSection(report);

        // Then
        expect(said(composed)).toContain("waiting");
        expect(said(composed)).toContain("error");
        expect(said(composed)).toContain("done");
    });

    it("Given the backend has not answered yet, when the section is composed, then it says what it is waiting for rather than nothing", () => {
        // Given — la navigation traverse la section (`⌥↓`) : un panneau vide se lirait
        // comme une panne. Et rien n'y est cliquable — Ash n'ouvre pas les Réglages
        // Système à la place de l'utilisateur
        const composed = notificationsSection(null);

        // Then
        expect(said(composed)).toContain("asking macOS…");
        expect(composed.flatMap((child) => findAll(child, "ui-button"))).toEqual([]);
    });
});
