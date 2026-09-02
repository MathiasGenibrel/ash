import { describe, expect, it } from "bun:test";

import { createRelaunch, RELAUNCH_DELAY, type Timer } from "./relaunch";

/**
 * Une horloge de test : elle n'attend rien, elle **enregistre** les reports et les fait
 * échoir à la demande.
 *
 * Le dépôt ne dort dans aucun test — un test qui dort finit par être désactivé, et un
 * debounce de 400 ms se prouve exactement de la même façon avec du temps injecté qu'avec
 * du temps réel.
 */
function aFakeTimer(): Timer & { elapse(): void; scheduled(): number } {
    let waiting: { delay: number; action: () => void }[] = [];
    return {
        after(delay, action) {
            const entry = { delay, action };
            waiting.push(entry);
            return () => {
                waiting = waiting.filter((other) => other !== entry);
            };
        },
        elapse() {
            const due = waiting;
            waiting = [];
            for (const entry of due) entry.action();
        },
        scheduled: () => waiting.length,
    };
}

describe("la relance automatique de la vérification", () => {
    it("Given a path typed one character at a time, when the 400 ms go by, then ash verifies once and not once per key", () => {
        // Given — huit frappes lanceraient huit vérifications, dont sept décriraient un
        // chemin que personne n'a fini d'écrire
        const timer = aFakeTimer();
        const verified: string[] = [];
        const relaunch = createRelaunch((key) => verified.push(key), timer);

        // When
        relaunch.soon("claude");
        relaunch.soon("claude");
        relaunch.soon("claude");
        timer.elapse();

        // Then
        expect(verified).toEqual(["claude"]);
    });

    it("Given a relaunch still waiting out its delay, when the change cannot be followed by another key, then it fires right away and the pending one is dropped", () => {
        // Given — `⏎`, un menu d'adaptateur qu'on referme, et demain un chemin choisi dans
        // le Finder : trois gestes qui ne passent par aucune frappe suivante
        const timer = aFakeTimer();
        const verified: string[] = [];
        const relaunch = createRelaunch((key) => verified.push(key), timer);
        relaunch.soon("claude");

        // When
        relaunch.now("claude");
        timer.elapse();

        // Then — une seule vérification, celle qu'on a demandée
        expect(verified).toEqual(["claude"]);
        expect(timer.scheduled()).toBe(0);
    });

    it("Given two entries being edited, when one of them is typed in, then the other's pending relaunch survives", () => {
        // Given — un report unique ferait qu'une frappe dans une carte annule la
        // vérification d'une autre, sans que rien ne le dise
        const timer = aFakeTimer();
        const verified: string[] = [];
        const relaunch = createRelaunch((key) => verified.push(key), timer);

        // When
        relaunch.soon("claude");
        relaunch.soon("codex");
        relaunch.soon("claude");
        timer.elapse();

        // Then
        expect(verified.sort()).toEqual(["claude", "codex"]);
    });

    it("Given an entry whose card is gone, when its relaunch is cancelled, then nothing is verified", () => {
        // Given — supprimer une entrée pendant que son report court verrait Ash vérifier
        // une carte qui n'est plus à l'écran
        const timer = aFakeTimer();
        const verified: string[] = [];
        const relaunch = createRelaunch((key) => verified.push(key), timer);
        relaunch.soon("claude");

        // When
        relaunch.cancel("claude");
        timer.elapse();

        // Then
        expect(verified).toEqual([]);
    });

    it("Given the delay the mockup asks for, when a relaunch is scheduled, then it waits exactly that long", () => {
        // Given — 400 ms est une valeur du design, pas un réglage : plus court, la
        // vérification part au milieu d'un mot ; plus long, l'écran a l'air en panne
        const timer = aFakeTimer();
        let waited = 0;
        const relaunch = createRelaunch(() => undefined, {
            after(delay, action) {
                waited = delay;
                return timer.after(delay, action);
            },
        });

        // When
        relaunch.soon("claude");

        // Then
        expect(waited).toBe(RELAUNCH_DELAY);
    });
});
