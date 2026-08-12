import { describe, expect, it } from "bun:test";

import { DEFAULT_FONT_SIZE, parseFontSize } from "./font-size";

describe("la taille de police du terminal", () => {
    it("Given a value that is not a font size, when it crosses the boundary, then it is refused instead of being painted", () => {
        // Given — ce qui traverse la frontière est du JSON : un backend plus récent que la
        // webview, ou un event bricolé, ne doit pas laisser un terminal sans taille lisible
        const garbage = [null, undefined, "13", 0, -3, 12.5, Number.NaN, Number.POSITIVE_INFINITY];

        // When
        const parsed = garbage.map(parseFontSize);

        // Then
        expect(parsed).toEqual(garbage.map(() => null));
    });

    it("Given the size the backend holds, when it crosses the boundary, then it is taken as it is", () => {
        // Given — les bornes sont décidées en Rust, et nulle part ailleurs : la webview ne
        // rediscute pas une taille que le backend a déjà jugée lisible
        const held = 32;

        // When
        const parsed = parseFontSize(held);

        // Then
        expect(parsed).toBe(32);
        expect(parseFontSize(DEFAULT_FONT_SIZE)).toBe(13);
    });
});
