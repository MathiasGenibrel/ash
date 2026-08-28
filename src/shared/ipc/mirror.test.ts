import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { describe, expect, it } from "bun:test";
import ts from "typescript";

/**
 * La preuve que le filet mord.
 *
 * `mirror.ts` est une assertion de type : elle ne s'exécute pas, donc aucun test ne peut
 * l'appeler. Ce qui se teste, en revanche, c'est **`tsc` lui-même** — on lui donne les
 * vrais fichiers du dépôt, puis les mêmes avec un champ Rust renommé, et on regarde s'il
 * change d'avis.
 *
 * Sans ça, le dispositif entier tiendrait sur la parole de celui qui l'a posé : une
 * assertion mal écrite — un `extends` dans un seul sens, une distribution sur une union —
 * se réduit silencieusement à `true` et ne protège plus rien. Les deux scénarios ci-dessous
 * sont les deux moitiés de la même garantie : il se tait quand tout va bien, il crie
 * quand le contrat bouge d'un côté seulement.
 *
 * Le renommage est appliqué au **texte** du fichier généré, jamais au dépôt : c'est ce que
 * `cargo test` y écrirait après un `worktree_root` renommé en `root` dans
 * `src-tauri/src/features/pty/locate.rs`.
 */

const here = dirname(import.meta.path);

/** Les vrais fichiers du dépôt, tels que la vérification les voit. */
function sources(): Record<string, string> {
    return {
        "/mirrored/RepoRef.ts": readFileSync(join(here, "generated/RepoRef.ts"), "utf8"),
        "/mirrored/TabLocation.ts": readFileSync(join(here, "generated/TabLocation.ts"), "utf8"),
        "/mirrored/mirroring.ts": readFileSync(join(here, "mirroring.ts"), "utf8"),
        "/mirrored/contract.ts": readFileSync(join(here, "index.ts"), "utf8"),
        "/mirrored/check.ts": [
            `import type { TabLocation as RustTabLocation } from "./TabLocation";`,
            `import type { TabLocation } from "./contract";`,
            `import type { Assert, Mirrors } from "./mirroring";`,
            `export type TabLocationStillMirrorsRust = Assert<Mirrors<RustTabLocation, TabLocation>>;`,
        ].join("\n"),
    };
}

const options: ts.CompilerOptions = {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    strict: true,
    noUncheckedIndexedAccess: true,
    exactOptionalPropertyTypes: true,
    noEmit: true,
    skipLibCheck: true,
};

/** Ce que `bun run typecheck` dirait de ces fichiers-là. */
function typecheck(files: Record<string, string>): string[] {
    const lib = ts.getDefaultLibFilePath(options);
    const host: ts.CompilerHost = {
        fileExists: (name) => name in files || ts.sys.fileExists(name),
        readFile: (name) => files[name] ?? ts.sys.readFile(name),
        getSourceFile: (name, language) => {
            const text = files[name] ?? ts.sys.readFile(name);
            return text === undefined ? undefined : ts.createSourceFile(name, text, language, true);
        },
        getDefaultLibFileName: () => lib,
        writeFile: () => undefined,
        getCurrentDirectory: () => "/mirrored",
        getCanonicalFileName: (name) => name,
        useCaseSensitiveFileNames: () => true,
        getNewLine: () => "\n",
    };

    const program = ts.createProgram(Object.keys(files), options, host);
    return ts
        .getPreEmitDiagnostics(program)
        .map((diagnostic) => ts.flattenDiagnosticMessageText(diagnostic.messageText, " "));
}

describe("the Rust contract and its TypeScript mirror", () => {
    it("Given the contract as the repository has it, when it is typechecked, then nothing is reported", () => {
        // Given
        const asCommitted = sources();

        // When
        const reported = typecheck(asCommitted);

        // Then — si ce scénario échoue, les deux côtés ont déjà divergé
        expect(reported).toEqual([]);
    });

    it("Given a field renamed on the Rust side, when the contract is typechecked, then it refuses to compile", () => {
        // Given — `worktree_root` devient `root` dans `locate.rs` : voilà ce que `cargo
        // test` écrirait alors dans `generated/TabLocation.ts`
        const afterTheRename = sources();
        afterTheRename["/mirrored/TabLocation.ts"] = (
            afterTheRename["/mirrored/TabLocation.ts"] ?? ""
        ).replace("worktreeRoot", "root");

        // When
        const reported = typecheck(afterTheRename);

        // Then — et il dit dans quel sens ça diverge, pas seulement que ça diverge
        expect(reported).toEqual([
            "Type '\"the hand-written type does not accept what the Rust type sends\"' does not satisfy the constraint 'true'.",
        ]);
    });
});
