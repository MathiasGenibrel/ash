/**
 * Les raccourcis d'édition de ligne, traduits en octets pour le PTY.
 *
 * xterm.js ne connaît **aucun** de ces raccourcis : `⌥←`, `⌘←`, `⌥⌫`… sont des
 * conventions de Terminal.app et d'iTerm, pas du terminal lui-même. Tant qu'Ash laissait
 * `macOptionIsMeta: true`, ⌥ produisait bien un `ESC`+touche — mais pour **toutes** les
 * touches, ce qui interdisait à macOS de composer `|` (⌥⇧L sur AZERTY), `~`, `\`, `{`,
 * `}`, `[`, `]` ou `€`. On payait la moitié qui gêne sans avoir celle qui sert.
 *
 * La table est donc explicite, et elle est ici plutôt que dans la vue parce que c'est une
 * **fonction pure de l'événement vers des octets** : elle ne touche pas au DOM, ne connaît
 * ni xterm.js ni le pont Tauri, et se vérifie sans clavier. `xterm-view.ts` ne fait plus
 * que la brancher et envoyer ce qu'elle rend.
 *
 * Les séquences sont celles que `readline`/`zle` attendent, et celles que Terminal.app et
 * iTerm envoient : l'objectif n'est pas d'inventer une convention, c'est de n'en manquer
 * aucune.
 *
 * Ce que la table ne fait **pas**, et ne doit pas faire : capter un accélérateur du menu
 * natif. `Cmd+W`, `Cmd+K`, `Cmd+B`, `Cmd+,` et `Cmd+1…9` sont déclarés dans
 * `src-tauri/src/menu.rs`, et macOS les consomme dans `performKeyEquivalent:` avant que la
 * webview ne voie un `keydown`. Une entrée qui les nommerait serait morte ici — et vivante
 * le jour où le menu perdrait l'entrée. `MENU_ACCELERATORS`, dans le test voisin, tient ce
 * garde-fou.
 */

/** Ce que la table lit d'un événement clavier : un `KeyboardEvent` en est un. */
export interface KeyChord {
    readonly type: string;
    readonly key: string;
    readonly altKey: boolean;
    readonly ctrlKey: boolean;
    readonly metaKey: boolean;
    readonly shiftKey: boolean;
}

const ESC = "\x1b";

/** Les seules valeurs de `KeyboardEvent.key` que la table nomme. */
type BoundKey = "ArrowLeft" | "ArrowRight" | "Backspace" | "Delete";

/**
 * L'accord, écrit comme on le lit : ses modificateurs puis sa touche.
 *
 * C'est un type, et pas une convention de commentaire, parce que la table est indexée par
 * lui : une entrée mal orthographiée (`"Alt+ArrowLeft"`, `"⌘⌥ArrowLeft"` dans le mauvais
 * ordre) ne compile pas, et **deux entrées pour le même accord non plus** — un objet
 * littéral n'a pas deux fois la même clé. C'est ce qui rend l'ajout d'une ligne sûr : le
 * recouvrement silencieux, où la seconde entrée serait morte sans qu'aucun test ne le
 * dise, n'est pas représentable.
 */
type Chord = `${"" | "⌥"}${"" | "⌘"}${BoundKey}`;

/**
 * Les six raccourcis de la spec, et rien d'autre.
 *
 * `⌘←`/`⌘→` envoient `Ctrl-A`/`Ctrl-E`, `⌘⌫`/`⌘⌦` envoient `Ctrl-U`/`Ctrl-K` : ce sont des
 * **chemins ajoutés**, pas des remplacements. Les mêmes contrôles tapés directement ne
 * passent pas par ici — ils n'ont ni `altKey` ni `metaKey` — et continuent d'être traités
 * par xterm.js.
 *
 * Ajouter une ligne ici suffit à ajouter un raccourci qui **envoie des octets** ; une
 * touche nouvelle s'ajoute d'abord à `BoundKey`. Les issues voisines (#77 onglets, #78
 * défilement, #79 recherche, #80 taille de police) ne relèvent pas de cette table : elles
 * déclenchent des actions de l'application, pas une écriture dans le PTY, et ce n'est pas
 * la même chose qu'on rend.
 *
 * Les valeurs ne contiennent jamais `\r` ni `\n` : ADR-0015, et un test le vérifie.
 */
const LINE_EDITING = {
    // Mot précédent / suivant — `backward-word` et `forward-word`.
    "⌥ArrowLeft": `${ESC}b`,
    "⌥ArrowRight": `${ESC}f`,
    // Début / fin de ligne — `Ctrl-A` et `Ctrl-E`.
    "⌘ArrowLeft": "\x01",
    "⌘ArrowRight": "\x05",
    // Efface le mot précédent — `ESC ⌫`, et non `ESC w` : c'est ce qu'envoie Terminal.app,
    // et le seul que `zle` relie à `backward-kill-word` sans configuration.
    "⌥Backspace": `${ESC}\x7f`,
    // Efface le mot suivant — `ESC d`, `kill-word`.
    "⌥Delete": `${ESC}d`,
    // Efface jusqu'au début / la fin de la ligne — `Ctrl-U` et `Ctrl-K`.
    "⌘Backspace": "\x15",
    "⌘Delete": "\x0b",
} satisfies Partial<Record<Chord, string>>;

/**
 * La même table, vue comme un dictionnaire ouvert.
 *
 * L'affectation élargit le type sans `as` : la table garde ses clés vérifiées à
 * l'écriture, et la résolution peut y chercher n'importe quelle touche du clavier.
 */
const BY_CHORD: Readonly<Record<string, string | undefined>> = LINE_EDITING;

/**
 * Les octets qu'un accord doit envoyer au PTY, ou `null` s'il ne nous regarde pas.
 *
 * `null` est la réponse par défaut, et c'est ce qui rend la correction sûre : tout ce qui
 * n'est pas nommé ci-dessus — une touche composée avec ⌥, un `Ctrl-A` tapé directement, un
 * accélérateur de menu qui aurait échappé au système — repart intact vers xterm.js.
 *
 * Les modificateurs sont comparés **exactement** : `ctrlKey` et `shiftKey` doivent être
 * relâchés. `⌃⌥←` ou `⇧⌥←` ne sont pas des raccourcis d'Ash, et les avaler priverait le
 * shell de ce qu'il en aurait fait.
 */
export function resolveKeyBinding(chord: KeyChord): string | null {
    if (chord.type !== "keydown") return null;
    if (chord.ctrlKey || chord.shiftKey) return null;

    const pressed = `${chord.altKey ? "⌥" : ""}${chord.metaKey ? "⌘" : ""}${chord.key}`;
    return BY_CHORD[pressed] ?? null;
}
