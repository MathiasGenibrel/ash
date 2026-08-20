import type { IBufferRange, ILink, ILinkProvider } from "@xterm/xterm";

import { scanLine, type Candidate } from "./link-scan";

/**
 * Les liens d'un terminal : ce qui se souligne sous `Cmd`, et ce que le clic ouvre
 * (spec §4.2).
 *
 * ## Trois règles, et la raison de chacune
 *
 * - **Le frontend ne décide rien.** Il découpe (`link-scan.ts`), il demande, il souligne ce
 *   que le backend a reconnu, et il redemande au clic. La liste blanche des schémas et la
 *   vérification d'existence sont dans `src-tauri/src/features/links/`, parce que la
 *   sortie d'un PTY est du texte hostile et qu'une décision prise dans la webview serait
 *   une décision prise par ce texte.
 * - **La vérification ne bloque jamais le rendu.** Elle est asynchrone, et **un candidat
 *   pas encore vérifié reste du texte** : pas de soulignement, pas de curseur en main, pas
 *   de clic. C'est un défaut sûr, pas un état d'attente à afficher.
 * - **Rien de ce que le survol a retenu n'autorise le clic.** Ce qui repart au clic est le
 *   mot, jamais un chemin résolu — voir `commands.rs` côté Rust.
 *
 * ## Pourquoi un lien est rendu même quand il n'est pas encore vérifié
 *
 * xterm.js **met en cache la réponse d'un fournisseur pour la ligne survolée** : tant que
 * la souris reste sur la même rangée, il ne redemande pas (`Linkifier._askForLink`,
 * vérifié sur 6.0.0). Rendre `undefined` en attendant la réponse du backend laisserait donc
 * la ligne inerte jusqu'à ce que la souris en sorte et y revienne. Un `ILink` est donc rendu
 * pour chaque candidat, avec ses **décorations éteintes** — c'est le mécanisme que xterm.js
 * documente (« changes made after the link is provided will trigger changes ») —, et elles
 * s'allument quand la vérification revient, sous la souris, sans rien redessiner d'autre.
 * Un lien éteint n'est pas cliquable : le refus est dans `activate`.
 */

/** Ce que la feature attend du backend `links` : une question, une ouverture. */
export interface LinkBridge {
    /** Ceux de ces mots qu'Ash accepterait d'ouvrir, résolus depuis `cwd`. */
    openable(cwd: string, candidates: string[]): Promise<string[]>;
    /** Ouvre — ou ne fait rien, si le backend ne le reconnaît plus. */
    open(cwd: string, candidate: string): Promise<void>;
}

/** Une ligne **logique** du tampon, ses replis déjà défaits. */
export interface LinkLine {
    readonly text: string;
    /** La rangée du tampon où la ligne logique commence, dans les coordonnées d'xterm.js. */
    readonly startRow: number;
    /** La largeur du terminal, qui transforme un index de la ligne en colonne. */
    readonly cols: number;
}

/** Comment lire la ligne logique qui passe par une rangée. `null` s'il n'y en a pas. */
export type LineReader = (bufferLineNumber: number) => LinkLine | null;

/**
 * Ce qu'il faut savoir d'ailleurs pour donner un sens à un mot : à qui demander, et depuis
 * où résoudre.
 *
 * C'est ce que le composition root de la feature passe à une vue ; la lecture des lignes,
 * elle, appartient à la vue et n'est connue de personne d'autre.
 */
export interface LinkContext {
    readonly bridge: LinkBridge;
    /**
     * Le `cwd` de l'onglet affiché, **au moment du survol**.
     *
     * Une fonction et non une valeur : le `cwd` change à chaque `cd`, la sonde d'ADR-0005
     * le suit, et un chemin relatif doit se résoudre depuis le répertoire courant — pas
     * depuis celui qu'avait l'onglet en s'ouvrant.
     */
    cwd(): string | null;
}

export interface LinkDeps extends LinkContext {
    readonly lines: LineReader;
}

/**
 * Ce qui sépare un `cwd` d'un mot dans une clé de cache.
 *
 * `\0` et non un espace : **un répertoire peut en contenir un** (`/Users/moi/Mes
 * projets`), et une clé qu'on ne saurait plus relire ferait chercher les liens d'un autre
 * dossier. Aucun des deux morceaux ne peut porter un octet nul — le backend refuse
 * d'ailleurs tout candidat qui en porterait un (`target.rs`).
 */
const KEY_SEPARATOR = "\u0000";

/**
 * Combien de réponses une vue garde en tête.
 *
 * Tout le reste de la fonctionnalité est **borné** face à une sortie hostile — 64 candidats
 * par ligne (`link-scan.ts`), 128 par question et 4096 octets par candidat (côté Rust) — et
 * la mémoire du survol était le seul endroit qui ne l'était pas : une session de plusieurs
 * jours, `Cmd` tenu au-dessus de sorties verbeuses, y accumulait une clé par mot vu sans
 * jamais rien rendre. Ce n'est pas une fuite qui casse quelque chose ; c'est un tas qui ne
 * redescend pas, dans un processus dont l'auteur ne ferme jamais la fenêtre.
 *
 * 2048 clés couvrent très largement ce qu'une main survole entre deux `cd`. Ce que l'oubli
 * coûte est un aller-retour de plus, c'est-à-dire rien ; ce qu'il rend en prime, c'est qu'un
 * fichier créé **après** avoir été survolé finit par redevenir cliquable, là où une réponse
 * négative gardée pour toujours le laissait inerte jusqu'à la fermeture de l'onglet.
 */
const MOST_REMEMBERED = 2048;

/**
 * Un ensemble de clés qui **oublie les plus anciennes** au lieu de grandir sans fin.
 *
 * L'ordre d'insertion est celui d'un `Set` de JavaScript, et c'est tout ce dont l'éviction a
 * besoin : la plus ancienne clé est la première que l'itérateur rend. Rien ici ne connaît
 * les liens — c'est une mémoire, pas une décision, et la décision est côté Rust.
 */
class Remembered {
    private readonly keys = new Set<string>();

    has(key: string): boolean {
        return this.keys.has(key);
    }

    add(key: string): void {
        // Réinsérer ne rajeunit pas une clé : une réponse ne change pas de fraîcheur parce
        // qu'on l'a relue, et le `delete`/`add` que ça demanderait coûterait à chaque survol
        // plus que l'oubli qu'il éviterait.
        if (this.keys.has(key)) return;
        this.keys.add(key);
        if (this.keys.size <= MOST_REMEMBERED) return;
        const oldest = this.keys.values().next();
        if (oldest.done !== true) this.keys.delete(oldest.value);
    }

    delete(key: string): void {
        this.keys.delete(key);
    }

    clear(): void {
        this.keys.clear();
    }
}

/** Ce qu'il faut savoir du lien survolé pour l'allumer, l'éteindre et l'ouvrir. */
interface Hovered {
    readonly link: ILink;
    readonly key: string;
    readonly cwd: string;
    readonly candidate: string;
}

export class TerminalLinks {
    private readonly deps: LinkDeps;
    /** Les mots que le backend a reconnus, sous la forme `cwd` + mot. */
    private readonly recognised = new Remembered();
    /** Ceux dont la réponse est en route, pour ne pas les redemander à chaque rangée. */
    private readonly asked = new Remembered();
    private held = false;
    /** Le lien sous la souris — il n'y en a jamais plus d'un, xterm.js s'en assure. */
    private hovered: Hovered | null = null;
    private disposed = false;

    constructor(deps: LinkDeps) {
        this.deps = deps;
    }

    /** Le fournisseur à poser sur le terminal. */
    get provider(): ILinkProvider {
        return {
            provideLinks: (bufferLineNumber, callback) => {
                callback(this.linksOf(bufferLineNumber));
            },
        };
    }

    /**
     * `Cmd` vient d'être enfoncé ou relâché — ou la fenêtre a perdu le focus.
     *
     * Repeindre **sans attendre un mouvement de souris** est un critère à part entière :
     * relâcher `Cmd` la main immobile doit rendre le curseur au texte. xterm.js, lui, ne
     * redemande rien tant que la souris ne change pas de cellule.
     */
    setCmdHeld(held: boolean): void {
        if (this.held === held) return;
        this.held = held;
        if (held && this.hovered !== null) {
            this.verify(this.hovered.cwd, [this.hovered.candidate]);
        }
        this.paint();
    }

    /**
     * Y a-t-il, sous la souris, un lien que `Cmd`+clic ouvrirait ?
     *
     * C'est ce qui décide si le clic est **retenu** avant d'atteindre une TUI en suivi de
     * souris. Voir `xterm-view.ts` : sans `Cmd`, ou sans lien ouvrable, `vim` et `htop`
     * reçoivent leurs événements comme aujourd'hui.
     */
    get claimsTheClick(): boolean {
        return this.held && this.hovered !== null && this.recognised.has(this.hovered.key);
    }

    dispose(): void {
        this.disposed = true;
        this.hovered = null;
        this.recognised.clear();
        this.asked.clear();
    }

    private linksOf(bufferLineNumber: number): ILink[] | undefined {
        const line = this.deps.lines(bufferLineNumber);
        if (line === null) return undefined;

        const cwd = this.deps.cwd() ?? "";
        const candidates = scanLine(line.text);
        if (candidates.length === 0) return undefined;

        // Une seule question pour la ligne entière : un aller-retour par mot ferait une
        // dizaine d'appels pour un seul mouvement de souris.
        if (this.held) {
            this.verify(
                cwd,
                candidates.map((candidate) => candidate.text),
            );
        }

        return candidates.map((candidate) => this.link(candidate, line, cwd));
    }

    private link(candidate: Candidate, line: LinkLine, cwd: string): ILink {
        const at = keyOf(cwd, candidate.text);
        const lit = this.held && this.recognised.has(at);
        const link: ILink = {
            range: rangeOf(candidate, line),
            text: candidate.text,
            decorations: { pointerCursor: lit, underline: lit },
            activate: (event) => {
                // Trois refus, dans cet ordre : sans `Cmd`, un clic est un clic ; un mot que
                // le backend n'a pas reconnu n'est pas un lien ; et ce qui repart est le
                // **mot**, que le backend décidera de nouveau.
                if (!event.metaKey) return;
                if (!this.recognised.has(at)) return;
                event.preventDefault();
                void this.deps.bridge.open(cwd, candidate.text).catch(() => {
                    // Le backend a refusé, ou LaunchServices n'a pas voulu. Il n'y a rien à
                    // dire que l'utilisateur ne voie déjà : le Finder ne s'ouvre pas.
                });
            },
            hover: () => {
                this.hovered = { link, key: at, cwd, candidate: candidate.text };
                if (this.held) this.verify(cwd, [candidate.text]);
            },
            leave: () => {
                if (this.hovered?.link === link) this.hovered = null;
            },
        };
        return link;
    }

    /** Demande au backend ce qu'il reconnaît — une fois par mot, jamais deux. */
    private verify(cwd: string, candidates: string[]): void {
        const unknown = candidates.filter((candidate) => {
            const at = keyOf(cwd, candidate);
            return !this.recognised.has(at) && !this.asked.has(at);
        });
        if (unknown.length === 0) return;
        for (const candidate of unknown) this.asked.add(keyOf(cwd, candidate));

        void this.deps.bridge
            .openable(cwd, unknown)
            .then((recognised) => {
                if (this.disposed) return;
                for (const candidate of recognised) this.recognised.add(keyOf(cwd, candidate));
                // Le survol a commencé avant la réponse — c'est le cas nominal, et c'est ici
                // que le soulignement apparaît.
                this.paint();
            })
            .catch(() => {
                // Rien ne devient cliquable, et les mots restent du texte. On oublie qu'ils
                // ont été demandés pour que le survol suivant repose la question.
                for (const candidate of unknown) this.asked.delete(keyOf(cwd, candidate));
            });
    }

    /** Allume ou éteint le lien sous la souris. Il n'y en a jamais plus d'un. */
    private paint(): void {
        const hovered = this.hovered;
        if (hovered === null) return;
        const lit = this.held && this.recognised.has(hovered.key);
        const decorations = hovered.link.decorations;
        if (decorations === undefined) return;
        // Les affectations passent par les accesseurs qu'xterm.js pose sur le lien courant :
        // ce sont **elles** qui repeignent le soulignement et le curseur.
        decorations.pointerCursor = lit;
        decorations.underline = lit;
    }
}

/** Un mot n'est reconnu que **pour un `cwd`** : `src/main.rs` change de sens à chaque `cd`. */
function keyOf(cwd: string, candidate: string): string {
    return `${cwd}${KEY_SEPARATOR}${candidate}`;
}

/**
 * La place du candidat dans le tampon, bornes **incluses** — la convention d'xterm.js
 * (`Linkifier._linkAtPosition`, vérifié sur 6.0.0).
 *
 * Une ligne logique repliée occupe plusieurs rangées : l'index dans la ligne donne la
 * rangée par division entière, et la colonne par le reste. Les colonnes sont en base 1.
 */
function rangeOf(candidate: Candidate, line: LinkLine): IBufferRange {
    const last = Math.max(candidate.start, candidate.end - 1);
    return {
        start: {
            x: (candidate.start % line.cols) + 1,
            y: line.startRow + Math.floor(candidate.start / line.cols),
        },
        end: {
            x: (last % line.cols) + 1,
            y: line.startRow + Math.floor(last / line.cols),
        },
    };
}
