import { describe, expect, it } from "bun:test";

import { parseThemeMode, resolveTheme } from "./theme";

describe("le thème de la fenêtre", () => {
    it("Given the system mode, when macOS switches to dark, then the window follows it", () => {
        // Given — le mode *système* n'est pas une troisième palette : c'est l'absence de
        // choix, donc celui de macOS, et il doit suivre à chaud
        const mode = "system";

        // When
        const before = resolveTheme(mode, false);
        const after = resolveTheme(mode, true);

        // Then
        expect(before).toBe("light");
        expect(after).toBe("dark");
    });

    it("Given a light theme chosen explicitly, when macOS is dark, then the explicit choice wins", () => {
        // Given / When — c'est tout l'objet d'un choix explicite : ne pas être repris par
        // la préférence du système
        const theme = resolveTheme("light", true);

        // Then
        expect(theme).toBe("light");
    });

    it("Given a dark theme chosen explicitly, when macOS is light, then the explicit choice wins", () => {
        // Given / When
        const theme = resolveTheme("dark", false);

        // Then
        expect(theme).toBe("dark");
    });

    it("Given a mode this webview does not know, when it crosses the boundary, then it is refused instead of being painted", () => {
        // Given — ce qui traverse la frontière est du JSON : un backend plus récent, ou un
        // fichier de préférence bricolé à la main, ne doit pas laisser la fenêtre sans palette
        const unknown = ["solarized", null, 3, undefined];

        // When
        const parsed = unknown.map(parseThemeMode);

        // Then
        expect(parsed).toEqual([null, null, null, null]);
        expect(parseThemeMode("system")).toBe("system");
    });
});
