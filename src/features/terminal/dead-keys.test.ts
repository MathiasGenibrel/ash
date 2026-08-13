import { describe, expect, it } from "bun:test";

import { press } from "./builders";
import { DeadKeyRepair } from "./dead-keys";

/**
 * Les deux séquences réelles, relevées sur une page xterm.js nue sous WebKit.
 *
 * Elles sont écrites ici telles quelles parce que c'est **l'observation** qui fonde le
 * contournement, pas un raisonnement sur le code de xterm.js : la composition qui échoue
 * clôt sur l'accent seul puis livre la chaîne entière sur le `keydown` de la lettre, celle
 * qui aboutit clôt sur le caractère composé et le livre seul. Voir `dead-keys.ts`.
 */
describe("DeadKeyRepair", () => {
    it("Given a dead key whose composition failed, when the next keydown carries the whole composed string, then the lost letter is recovered", () => {
        // Given
        const repair = new DeadKeyRepair();
        repair.compositionEnded("^");
        // When
        const missing = repair.resolveKeyDown(press("^d").build());
        // Then
        expect(missing).toBe("d");
    });

    it("Given a dead key whose composition succeeded, when the next keydown carries the composed character, then nothing is recovered", () => {
        // Given
        const repair = new DeadKeyRepair();
        repair.compositionEnded("ê");
        // When
        const missing = repair.resolveKeyDown(press("ê").build());
        // Then
        expect(missing).toBeNull();
    });

    it("Given no composition, when an ordinary letter is pressed, then nothing is recovered", () => {
        // Given
        const repair = new DeadKeyRepair();
        // When
        const missing = repair.resolveKeyDown(press("d").build());
        // Then
        expect(missing).toBeNull();
    });

    it("Given a composition already consumed by one keydown, when a second keydown follows, then nothing is recovered", () => {
        // Given
        const repair = new DeadKeyRepair();
        repair.compositionEnded("^");
        repair.resolveKeyDown(press("^d").build());
        // When
        const missing = repair.resolveKeyDown(press("^x").build());
        // Then
        expect(missing).toBeNull();
    });

    it("Given a composition, when the next keydown is unrelated to it, then nothing is recovered", () => {
        // Given
        const repair = new DeadKeyRepair();
        repair.compositionEnded("^");
        // When
        const missing = repair.resolveKeyDown(press("Enter").build());
        // Then
        expect(missing).toBeNull();
    });

    it("Given a composition, when the following chord holds Command, then nothing is recovered", () => {
        // Given
        const repair = new DeadKeyRepair();
        repair.compositionEnded("^");
        // When
        const missing = repair.resolveKeyDown(press("^d").withCommand().build());
        // Then
        expect(missing).toBeNull();
    });

    it("Given a composition, when a keyup arrives before any keydown, then nothing is recovered", () => {
        // Given
        const repair = new DeadKeyRepair();
        repair.compositionEnded("^");
        // When
        const missing = repair.resolveKeyDown(press("^d").released().build());
        // Then
        expect(missing).toBeNull();
    });

    it("Given a dead key followed by a letter, when the composed string holds several characters, then only the part xterm.js loses is recovered", () => {
        // Given — `⌥n` puis `d` : la même panne, avec le tilde.
        const repair = new DeadKeyRepair();
        repair.compositionEnded("~");
        // When
        const missing = repair.resolveKeyDown(press("~d").build());
        // Then
        expect(missing).toBe("d");
    });
});
