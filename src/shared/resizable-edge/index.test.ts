import { describe, expect, it } from "bun:test";

import {
    clampedSize,
    grabOffset,
    handleOffset,
    sizePercent,
    type EdgeBounds,
} from "@/shared/resizable-edge";

/** Des bornes quelconques : ce module ne connaît celles d'aucune feature. */
const BOUNDS: EdgeBounds = { min: 0.1, max: 0.8 };

describe("la géométrie d'un bord réglable", () => {
    it("Given a size below the floor of its extent, when it is laid out, then the floor is what shows", () => {
        // Given
        const asked = 20;
        // When
        const shown = clampedSize(asked, 1000, BOUNDS);
        // Then — sous le plancher, la surface s'arrête ; c'est le relâchement, et lui seul,
        // qui referme, et cette décision-là appartient à la feature
        expect(shown).toBe(100);
    });

    it("Given an extent that shrinks under a size already set, when it is laid out again, then the size comes back into range", () => {
        // Given — une taille réglée dans une grande fenêtre, gardée telle quelle sur le disque
        const kept = 700;
        // When — la fenêtre rétrécit
        const shown = clampedSize(kept, 500, BOUNDS);
        // Then
        expect(shown).toBe(400);
    });

    it("Given a pointer grabbing the widened zone beside the line, when the grab offset is kept, then the line follows the pointer instead of jumping to it", () => {
        // Given — la zone déborde de 7 px, et on l'attrape 3 px avant le trait
        const grab = grabOffset(200, 190, 7);
        // When — le pointeur avance de 40 px
        const edge = 240 - grab;
        // Then — le trait a bougé de 40 px, pas de 43
        expect(grab).toBe(3);
        expect(edge).toBe(237);
    });

    it("Given a pointer running past the end of the edge, when the handle follows it, then it stays inside its margins", () => {
        // Given
        const margin = 18;
        // When
        const offsets = [-50, 60, 400].map((pointer) => handleOffset(pointer, 0, 200, margin));
        // Then
        expect(offsets).toEqual([18, 60, 182]);
    });

    it("Given a size assistive technology asks about, when it is announced, then it is a percentage of the extent within the bounds", () => {
        // Given — une taille sous le plancher : ce qui s'annonce est ce qui se voit
        const sizes = [20, 250, 900];
        // When
        const announced = sizes.map((size) => sizePercent(size, 1000, BOUNDS));
        // Then
        expect(announced).toEqual([10, 25, 80]);
    });
});
