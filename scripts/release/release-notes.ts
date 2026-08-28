/**
 * Le corps d'une section de `CHANGELOG.md`, pour en faire les notes d'une release.
 *
 * Le format est Keep a Changelog : un titre de niveau 2 par version (`## [1.2.0] - …`), des
 * titres de niveau 3 par catégorie (`### Ajouté`). Seul un `## ` termine une section — un
 * `### ` en fait partie.
 *
 * Une section présente mais vide est un **échec**, pas une chaîne vide : publier une release
 * sans notes est une erreur qu'on préfère voir avant la publication.
 *
 * Comme `version.ts` : les fonctions pures prennent le contenu du fichier, la CLI en dessous
 * lit le disque, imprime et choisit le code de sortie.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export type ReleaseNotes =
    | { readonly ok: true; readonly body: string }
    | { readonly ok: false; readonly message: string };

/** `## [1.2.0]`, éventuellement suivi d'une date — mais pas `## [1.2.0-rc.1]`. */
function isHeadingOf(line: string, version: string): boolean {
    const matched = /^##(?!#)\s*\[([^\]]+)\]/.exec(line);
    return matched?.[1] === version;
}

function isVersionHeading(line: string): boolean {
    return /^##(?!#)\s/.test(line);
}

export function releaseNotesFor(changelog: string, version: string): ReleaseNotes {
    const lines = changelog.split("\n");
    const start = lines.findIndex((line) => isHeadingOf(line, version));
    if (start === -1) {
        return { ok: false, message: `CHANGELOG.md : aucune section [${version}]` };
    }

    const body: string[] = [];
    for (const line of lines.slice(start + 1)) {
        if (isVersionHeading(line)) break;
        body.push(line);
    }

    const trimmed = body.join("\n").trim();
    if (trimmed === "") {
        return { ok: false, message: `CHANGELOG.md : la section [${version}] est vide` };
    }
    return { ok: true, body: trimmed };
}

const USAGE = "usage : bun scripts/release/release-notes.ts X.Y.Z";

if (import.meta.main) {
    const [version] = process.argv.slice(2);
    if (version === undefined) {
        console.error(USAGE);
        process.exit(1);
    }

    const root = fileURLToPath(new URL("../../", import.meta.url));
    const result = releaseNotesFor(readFileSync(`${root}CHANGELOG.md`, "utf8"), version);

    if (!result.ok) {
        console.error(result.message);
        process.exit(1);
    }
    console.log(result.body);
}
