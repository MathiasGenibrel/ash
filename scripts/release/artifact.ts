/**
 * Le nom de l'archive publiée par une release, et les chemins du bundle qu'elle contient.
 *
 * Tout ce qui est une **règle de nommage** vit ici, et nulle part ailleurs : le nom du
 * `.zip`, la cible Rust qu'on construit, l'étiquette d'architecture qui en découle, le
 * chemin du bundle produit par `tauri build`, et celui d'`ash-event` à l'intérieur. Le
 * workflow ne compose aucune de ces chaînes : il les demande.
 *
 * La raison est la même que pour `version.ts` : une règle recomposée dans une étape shell
 * est une seconde définition, hors des tests, et silencieuse quand elle diverge. Un
 * `Ash-$TAG-macos.zip` écrit dans le YAML publierait `Ash-v1.2.0-macos.zip` sans que rien
 * ne le contredise.
 *
 * Ce fichier ne redécide pas non plus la forme d'un numéro de version : il la demande à
 * `version.ts`, qui la détient. C'est ce qui laisse la CI passer `$GITHUB_REF_NAME` tel
 * quel — `v1.2.0` comme `1.2.0`.
 *
 * Comme les deux autres scripts de release : les fonctions pures prennent des valeurs et
 * rendent une chaîne ou `null`, la CLI en dessous lit le disque, imprime et choisit le code
 * de sortie.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { versionOf } from "./version";

/**
 * La seule cible construite aujourd'hui. `x86_64-apple-darwin` et la cible universelle sont
 * hors périmètre ; les ajouter, c'est une entrée de plus dans `ARCH_LABELS` et un argument
 * de plus au workflow — pas une règle à réécrire ailleurs.
 */
export const TARGET = "aarch64-apple-darwin";

/**
 * Le triplet Rust dit `aarch64` ; Apple, et donc quiconque télécharge, dit `arm64`. Le nom
 * de l'archive parle la langue de celui qui la télécharge, et la table fait la traduction
 * une fois. Une cible inconnue est refusée plutôt qu'étiquetée au jugé : mieux vaut une
 * pipeline qui s'arrête qu'une archive dont le nom ment sur la machine qui l'exécutera.
 */
const ARCH_LABELS: Readonly<Record<string, string>> = {
    "aarch64-apple-darwin": "macos-arm64",
};

/**
 * `productName` et `identifier` sont décidés dans ce fichier : le bundle porte l'un comme
 * nom et l'autre comme identité de code. Ni le workflow ni ce module ne les réécrivent.
 */
const TAURI_CONF = "src-tauri/tauri.conf.json";

/**
 * `ash-event` est le client du socket de hooks (ADR-0007). L'application le cherche **à
 * côté d'elle**, dans son propre dossier `MacOS/` : absent du bundle, Ash s'installe, se
 * lance, et n'a plus aucun état d'agent — sans rien dire. D'où le chemin nommé ici et
 * vérifié par le job de build.
 */
const EVENT_BINARY = "ash-event";

function archLabelOf(target: string): string | null {
    return ARCH_LABELS[target] ?? null;
}

/**
 * `Ash-1.2.0-macos-arm64.zip`. `asked` est le tag ou le numéro nu ; le `v` ne survit jamais
 * dans le nom du fichier.
 */
export function artifactName(
    productName: string,
    asked: string,
    target: string = TARGET,
): string | null {
    const version = versionOf(asked);
    const arch = archLabelOf(target);
    if (version === null || arch === null) return null;
    return `${productName}-${version}-${arch}.zip`;
}

/**
 * Là où `tauri build --target <triplet>` dépose le bundle : cargo insère le triplet dans le
 * chemin. La release construit **toujours** pour une cible nommée — le chemin sans triplet,
 * celui d'un `bun run package` local, n'est demandé par personne ici et n'est donc pas une
 * variante que ce module propose.
 */
export function bundlePath(productName: string, target: string): string {
    return `src-tauri/target/${target}/release/bundle/macos/${productName}.app`;
}

export function eventBinaryPath(productName: string, target: string): string {
    return `${bundlePath(productName, target)}/Contents/MacOS/${EVENT_BINARY}`;
}

function stringFieldOf(tauriConf: string, field: string): string | null {
    let parsed: unknown;
    try {
        parsed = JSON.parse(tauriConf);
    } catch {
        return null;
    }
    if (typeof parsed !== "object" || parsed === null) return null;
    const value = (parsed as Record<string, unknown>)[field];
    return typeof value === "string" && value !== "" ? value : null;
}

export function productNameFrom(tauriConf: string): string | null {
    return stringFieldOf(tauriConf, "productName");
}

/**
 * L'identifiant de paquet, tel que `codesign -dv` le rendra sur un bundle correctement
 * signé. Le job de build le compare à ce que porte le bundle : un `codesign --verify` seul
 * passerait sur une application signée sous l'identifiant que l'éditeur de liens invente
 * (`ash-<hash>`), qui est exactement la panne de #206 — macOS refusait alors
 * silencieusement d'enregistrer l'application auprès du centre de notifications.
 */
export function identifierFrom(tauriConf: string): string | null {
    return stringFieldOf(tauriConf, "identifier");
}

const USAGE = [
    "usage :",
    "  bun scripts/release/artifact.ts --name vX.Y.Z   nom de l'archive",
    "  bun scripts/release/artifact.ts --bundle-path   le .app que tauri build produit",
    "  bun scripts/release/artifact.ts --event-binary  ash-event, dans ce bundle",
    "  bun scripts/release/artifact.ts --target        le triplet Rust construit",
    "  bun scripts/release/artifact.ts --identifier    l'identifiant de paquet attendu",
].join("\n");

/**
 * Un champ que la configuration Tauri **doit** porter. Absent, la release s'arrête ici en le
 * nommant, plutôt que de laisser une chaîne vide descendre dans une comparaison de CI : c'est
 * l'`Identifier` attendu d'un bundle signé qui deviendrait « n'importe lequel ».
 */
function required(value: string | null, field: string): string {
    if (value === null) {
        console.error(`${TAURI_CONF} : aucun ${field} lisible`);
        process.exit(1);
    }
    return value;
}

if (import.meta.main) {
    const [mode, asked] = process.argv.slice(2);
    const root = fileURLToPath(new URL("../../", import.meta.url));

    const printed = ((): string | null => {
        if (mode === "--target") return TARGET;

        const tauriConf = readFileSync(`${root}${TAURI_CONF}`, "utf8");
        if (mode === "--identifier") return required(identifierFrom(tauriConf), "identifier");

        const productName = required(productNameFrom(tauriConf), "productName");

        switch (mode) {
            case "--name":
                if (asked === undefined) break;
                return artifactName(productName, asked);
            case "--bundle-path":
                return bundlePath(productName, TARGET);
            case "--event-binary":
                return eventBinaryPath(productName, TARGET);
            default:
                break;
        }
        return null;
    })();

    if (printed === null) {
        console.error(
            mode === "--name" && asked !== undefined
                ? `« ${asked} » : format attendu X.Y.Z ou vX.Y.Z`
                : USAGE,
        );
        process.exit(1);
    }
    console.log(printed);
}
