import { describe, expect, it } from "bun:test";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

/**
 * La frontière du dossier, vérifiée plutôt que promise.
 *
 * Tout le bénéfice de cette couche tient à une seule chose : un composant décrit, il ne
 * peint pas. Le jour où un second fichier touche le DOM, les composants qui en dépendent
 * sortent des tests sans que personne ne s'en aperçoive — c'est exactement l'histoire de
 * `features/settings/view.ts`. Ce test rend la dérive visible à la première ligne.
 */
describe("la frontière de shared/ui", () => {
    it("Given the ui folder, when its sources are scanned, then paint is the only one that touches the DOM", () => {
        // Given — les fichiers de test sont exclus : ils ne peuvent de toute façon pas
        // monter de DOM, `bun test` n'en a pas
        const folder = fileURLToPath(new URL(".", import.meta.url));
        const sources = readdirSync(folder)
            .filter((name) => name.endsWith(".ts") && !name.endsWith(".test.ts"))
            .sort();

        // When
        const touching = sources.filter((name) =>
            /\b(document|window|HTMLElement|Node)\b\s*[.(]/.test(
                readFileSync(join(folder, name), "utf8"),
            ),
        );

        // Then
        expect(touching).toEqual(["paint.ts"]);
        expect(sources.length).toBeGreaterThan(1);
    });
});
