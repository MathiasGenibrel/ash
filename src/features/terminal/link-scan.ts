/**
 * Découper une ligne de terminal en **candidats** — et rien de plus.
 *
 * Ce fichier n'a **aucune autorité** : il ne dit pas ce qui est ouvrable, il dit ce qui
 * mérite d'être soumis. La décision est côté Rust, dans `features/links/target.rs`, qui
 * porte la liste blanche des schémas et la vérification d'existence. C'est voulu, et c'est
 * la seule répartition tenable : **la sortie d'un PTY est du texte hostile**, et une
 * décision de sécurité prise dans la webview serait une décision prise par ce que le texte
 * a réussi à faire croire à un rendu.
 *
 * D'où la conduite ici : on découpe **large** — un `javascript:alert(1)` est un candidat
 * comme un autre, et le backend le refuse. Un candidat de trop est un mot qui reste du
 * texte ; un candidat manqué serait un lien qu'on ne peut pas ouvrir.
 */

/** Un mot de la ligne, avec sa place dans la ligne **logique** (retours à la ligne défaits). */
export interface Candidate {
    /** Le mot, débarrassé de sa ponctuation d'entourage. */
    readonly text: string;
    /** Index du premier caractère, dans la ligne logique. */
    readonly start: number;
    /** Index du caractère qui suit le dernier, dans la ligne logique. */
    readonly end: number;
}

/**
 * Ce qu'on ne regarde pas au-delà — et ce que l'appelant ne recolle pas au-delà non plus.
 *
 * Une ligne logique peut être immense — une sortie qui n'a jamais rendu la main en fait
 * une seule de plusieurs milliers de colonnes. Le survol n'a de sens que sur ce qui est
 * lisible ; au-delà, il n'y a que du travail à donner à un `stat`.
 */
export const LONGEST_LINE = 8192;

/** Au-delà, la ligne n'est plus une ligne de sortie, c'est une attaque de patience. */
const MOST_CANDIDATES = 64;

/**
 * Ce qui ne peut pas faire partie d'un mot : les blancs, et ce qui entoure une citation.
 *
 * L'espace insécable en fait partie : les agents en impriment, et il ne se voit pas.
 */
const SEPARATORS = new Set([" ", "\t", "\n", "\r", '"', "'", "`", "<", ">", "|", "\u00a0"]);

/** Ce qu'on retire en tête : la ponctuation qui ouvre. */
const OPENING = new Set(["(", "[", "{", ",", ";", ":", "=", "*", "‘", "“"]);

/** Ce qu'on retire en queue : la ponctuation qui ferme une phrase, pas un chemin. */
const CLOSING = new Set([".", ",", ";", ":", "!", "?", "*", "]", "}", ")", "’", "”"]);

/** `<schéma>:` au sens de la RFC 3986 — la même règle large que côté Rust. */
const SCHEME = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;

/** `x.y` : un nom de fichier avec extension, seul, sans aucune barre oblique. */
const NAMED_WITH_EXTENSION = /^[\w@+-]+(\.[\w@+-]+)+$/;

/** `chemin:12` et `chemin:12:5` — ce que `rustc`, `tsc` et un `grep -n` impriment. */
const LINE_AND_COLUMN = /:\d+(:\d+)?$/;

/**
 * Les candidats d'une ligne logique, dans l'ordre où ils s'y trouvent.
 *
 * La ligne attendue est **logique** : les lignes que le terminal a repliées faute de
 * largeur ont déjà été recollées par l'appelant. Sans ça, une URL coupée en deux par le
 * bord de la fenêtre donnerait deux moitiés dont aucune n'est un lien.
 */
export function scanLine(line: string): Candidate[] {
    const found: Candidate[] = [];
    const readable = line.length > LONGEST_LINE ? line.slice(0, LONGEST_LINE) : line;

    let index = 0;
    while (index < readable.length && found.length < MOST_CANDIDATES) {
        const character = readable[index];
        if (character === undefined || SEPARATORS.has(character)) {
            index += 1;
            continue;
        }
        let end = index;
        while (end < readable.length) {
            const next = readable[end];
            if (next === undefined || SEPARATORS.has(next)) break;
            end += 1;
        }
        const candidate = trim(readable, index, end);
        if (candidate !== null) found.push(candidate);
        index = end + 1;
    }
    return found;
}

/**
 * Débarrasse un mot de ce qui l'entoure, et dit s'il reste quelque chose à soumettre.
 *
 * Le cas qui justifie le soin : `(voir https://example.com/a).` — la parenthèse fermante et
 * le point appartiennent à la phrase, pas à l'URL. Mais `https://fr.wikipedia.org/wiki/Ash_(logiciel)`
 * garde la sienne, parce qu'elle en ouvre une : c'est la règle de l'équilibre, la même que
 * celle des terminaux qui font ça correctement.
 */
function trim(line: string, from: number, to: number): Candidate | null {
    let start = from;
    let end = to;

    while (start < end) {
        const first = line[start];
        if (first === undefined || !OPENING.has(first)) break;
        start += 1;
    }
    while (end > start) {
        const last = line[end - 1];
        if (last === undefined || !CLOSING.has(last)) break;
        // Une fermante n'est retirée que si rien ne l'a ouverte dans le mot.
        if (last === ")" && line.slice(start, end).includes("(")) break;
        if (last === "]" && line.slice(start, end).includes("[")) break;
        if (last === "}" && line.slice(start, end).includes("{")) break;
        end -= 1;
    }

    let text = line.slice(start, end);
    if (text.length === 0) return null;

    // `src/main.rs:12:5` désigne un fichier, et c'est ce fichier qu'on révèle : le numéro
    // de ligne n'a nulle part où aller dans le Finder. Seulement pour ce qui n'a pas de
    // schéma — `https://example.com:8080` porte un port, pas une ligne.
    if (!SCHEME.test(text)) {
        const numbered = LINE_AND_COLUMN.exec(text);
        if (numbered !== null && numbered.index > 0) {
            end = start + numbered.index;
            text = line.slice(start, end);
        }
    }

    return looksLikeALink(text) ? { text, start, end } : null;
}

/**
 * Ce qui vaut la peine d'être soumis au backend.
 *
 * Trois formes, et la troisième est la seule qui demande à être justifiée : un mot sans
 * barre oblique mais avec une extension (`Cargo.toml`, `index.ts`) est un chemin relatif
 * parfaitement ordinaire dans une sortie de compilateur. Un mot nu (`README`) n'en est pas
 * un ici : il ferait un candidat de presque chaque mot anglais imprimé, pour un cas rare.
 */
function looksLikeALink(text: string): boolean {
    if (SCHEME.test(text)) return true;
    if (text.includes("/")) return true;
    return NAMED_WITH_EXTENSION.test(text);
}
