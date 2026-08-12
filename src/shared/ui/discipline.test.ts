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
 *
 * **Ce test lit du texte, il ne comprend pas le code** : il attrape la dérive ordinaire —
 * la ligne écrite sans y penser — pas quelqu'un qui cherche à la contourner. La frontière
 * vraiment structurelle est ailleurs : `UiNode` ne peut décrire aucun morceau de DOM, donc
 * un composant qui voudrait en toucher un devrait d'abord sortir du type.
 */
const DOM_TOKEN = /\b(document|window|globalThis|Element|Node|(HTML|SVG)\w*Element)\b\s*[.(]/;

/** Peindre est une sortie, pas une étape : un seul fichier la franchit, pas un seul appel. */
const PAINT_IMPORT = /from\s+"\.\/paint"/;

describe("la frontière de shared/ui", () => {
    const folder = fileURLToPath(new URL(".", import.meta.url));
    const sources = readdirSync(folder)
        .filter((name) => name.endsWith(".ts") && !name.endsWith(".test.ts"))
        .sort();
    const read = (name: string): string => readFileSync(join(folder, name), "utf8");

    it("Given the ui folder, when its sources are scanned, then paint is the only one that touches the DOM", () => {
        // Given — les fichiers de test sont exclus : ils ne peuvent de toute façon pas
        // monter de DOM, `bun test` n'en a pas
        expect(sources.length).toBeGreaterThan(1);

        // When
        const touching = sources.filter((name) => DOM_TOKEN.test(read(name)));

        // Then
        expect(touching).toEqual(["paint.ts"]);
    });

    it("Given the ui folder, when its sources are scanned, then only the public surface names paint", () => {
        // Given — la dérive la plus probable n'est pas d'écrire `document` : c'est d'ajouter
        // un `mount.ts` qui peint puis retouche le nœud rendu. Il ne nommerait aucun global
        // du DOM, et le composant qui en dépendrait sortirait des tests sans bruit.
        expect(sources).toContain("index.ts");

        // When
        const importing = sources.filter((name) => PAINT_IMPORT.test(read(name)));

        // Then — `index.ts` la réexporte, personne d'autre ne l'appelle depuis le dossier
        expect(importing).toEqual(["index.ts"]);
    });
});
