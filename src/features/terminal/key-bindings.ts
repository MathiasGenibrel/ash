/**
 * Les accords qu'Ash traduit lui-même en octets pour le PTY.
 *
 * Deux familles, pour deux raisons différentes : les six raccourcis d'**édition de ligne**
 * (#75), que xterm.js ne connaît pas, et `⇧⏎` (#91), que xterm.js connaît mais confond
 * avec `⏎` — voir l'entrée de la table, qui porte le détail.
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

/** Les touches d'édition de ligne : elles se combinent avec ⌥ et avec ⌘, jamais avec ⇧. */
type EditingKey = "ArrowLeft" | "ArrowRight" | "Backspace" | "Delete";

/**
 * L'accord, écrit comme on le lit : ses modificateurs puis sa touche, dans l'ordre `⇧⌥⌘`.
 *
 * C'est un type, et pas une convention de commentaire, parce que la table est indexée par
 * lui : une entrée mal orthographiée (`"Alt+ArrowLeft"`, `"⌘⌥ArrowLeft"` dans le mauvais
 * ordre) ne compile pas, et **deux entrées pour le même accord non plus** — un objet
 * littéral n'a pas deux fois la même clé. C'est ce qui rend l'ajout d'une ligne sûr : le
 * recouvrement silencieux, où la seconde entrée serait morte sans qu'aucun test ne le
 * dise, n'est pas représentable.
 *
 * L'union a deux branches plutôt qu'une famille unique, et c'est délibéré : ⏎ n'accepte
 * **que** ⇧, les touches d'édition n'acceptent **que** ⌥ et ⌘. Un
 * `${"" | "⇧"}${"" | "⌥"}${"" | "⌘"}${EditingKey | "Enter"}` aurait couvert les deux d'une
 * ligne, mais il aurait aussi rendu représentables `"Enter"` nu, `"⌘Enter"` et
 * `"⇧ArrowLeft"` — c'est-à-dire qu'il aurait laissé compiler l'entrée qui casse l'envoi
 * d'une commande. Un type qui accepte la faute qu'on redoute ne sert plus à rien.
 */
type Chord =
    | `${"" | "⌥"}${"" | "⌘"}${EditingKey}`
    // ⏎ n'entre dans la table qu'avec ⇧ : `"Enter"` seul n'est pas représentable, et c'est
    // la garantie la plus importante de ce type. Une entrée pour ⏎ nu détournerait la
    // touche qui valide les commandes.
    | "⇧Enter";

/**
 * Les raccourcis de la spec, et rien d'autre : les six d'édition de ligne, et `⇧⏎`.
 *
 * `⌘←`/`⌘→` envoient `Ctrl-A`/`Ctrl-E`, `⌘⌫`/`⌘⌦` envoient `Ctrl-U`/`Ctrl-K` : ce sont des
 * **chemins ajoutés**, pas des remplacements. Les mêmes contrôles tapés directement ne
 * passent pas par ici — ils n'ont ni `altKey` ni `metaKey` — et continuent d'être traités
 * par xterm.js.
 *
 * Ajouter une ligne ici suffit à ajouter un raccourci qui **envoie des octets** ; une
 * touche nouvelle s'ajoute d'abord à `Chord`. Les issues voisines (#77 onglets, #78
 * défilement, #79 recherche, #80 taille de police) ne relèvent pas de cette table : elles
 * déclenchent des actions de l'application, pas une écriture dans le PTY, et ce n'est pas
 * la même chose qu'on rend.
 *
 * Aucune valeur d'édition de ligne ne contient `\r` ni `\n` : ADR-0015, et un test le
 * vérifie. `⇧⏎` en contient un, et l'entrée ci-dessous dit pourquoi ce n'en est pas une
 * entorse.
 */
const BOUND_CHORDS = {
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
    // Retour à la ligne **dans** le prompt, sans l'envoyer — `⇧⏎` (#91).
    //
    // xterm.js envoie le **même octet** pour `⏎` et pour `⇧⏎` : `evaluateKeyboardEvent`
    // écrit `result.key = ev.altKey ? C0.ESC + C0.CR : C0.CR` (`@xterm/xterm` 6.0.0,
    // `src/common/input/Keyboard.ts:102`, `case 13`), et ne lit jamais `shiftKey`. Un agent
    // qui distingue les deux — Claude Code au premier chef — ne peut donc pas les
    // distinguer dans un onglet d'Ash : `⇧⏎` y envoie le prompt au lieu d'y insérer une
    // ligne. Ce n'est pas une régression, ça n'a jamais marché, et c'est la raison d'être
    // du `/terminal-setup` de Claude Code, qui pose ce même réglage dans iTerm2 et VS Code.
    //
    // `ESC`+`CR` est la convention retenue par ces terminaux, et c'est **déjà** ce que
    // xterm.js envoie pour `⌥⏎` : la ligne ci-dessus donne à `⇧⏎` la séquence dont `⌥⏎`
    // prouve l'effet, au lieu d'en inventer une. `⌥⏎` continue par ailleurs de la produire
    // — la table ne nomme pas `"⌥Enter"`, la frappe repart donc intacte vers xterm.js.
    // `macOptionIsMeta: false` n'y change rien : le chemin « third level shift » de xterm
    // (`CoreBrowserTerminal._isThirdLevelShift`) exige `!ev.keyCode || ev.keyCode > 47`, et
    // ⏎ a le keyCode 13.
    //
    // **La séquence part pour tous les onglets et tous les programmes**, parce qu'Ash ne
    // sait pas encore reconnaître ce qui tient l'avant-plan (#61). Les deux autres voies
    // ont été pesées : n'envoyer la séquence que sous un agent instrumenté est juste, mais
    // suppose #61 ; un réglage déplace la décision sur l'utilisateur, et la fenêtre de
    // réglages n'a pas de section pour ça (#22).
    //
    // Ce que coûte l'envoi inconditionnel est réel et se dit — mais il est plus petit que
    // « `⇧⏎` devient une gêne au shell ». Dans `zsh`, `ESC`+`CR` **est** lié : `zshzle(1)`
    // donne `ESC-^M` à `self-insert-unmeta`, « insert a character into the buffer after
    // stripping the meta bit and converting ^M to ^J ». À une invite `zsh` ordinaire, `⇧⏎`
    // insère donc une **nouvelle ligne dans la ligne de commande** au lieu de la valider :
    // c'est exactement ce que le geste veut dire. Le coût qui reste est ailleurs, et il est
    // réel : dans `vim`, `ESC` sort du mode insertion ; et dans un `zsh` en `bindkey -v`,
    // `ESC` passe en mode commande, où `^M` est `accept-line` — la ligne part quand même.
    //
    // Le cas le plus plausible n'est pas volontaire, c'est ⇧ encore enfoncé après une
    // majuscule. Il est accepté parce que le mode de défaillance reste « ça ne valide pas »
    // et jamais « ça fait autre chose » : `ESC`+`CR` n'exécute aucune commande que `CR`
    // n'aurait pas exécutée, et une seconde frappe de `⏎` répare. À reprendre quand #61
    // donnera de quoi conditionner l'entrée.
    //
    // ADR-0015 n'est pas en cause : le `\r` n'est pas composé par Ash, c'est la touche que
    // l'utilisateur vient de presser qui est relayée — et précédée d'`ESC`, elle valide
    // justement **moins** qu'aujourd'hui.
    "⇧Enter": `${ESC}\r`,
} satisfies Partial<Record<Chord, string>>;

/**
 * La même table, vue comme un dictionnaire ouvert.
 *
 * L'affectation élargit le type sans `as` : la table garde ses clés vérifiées à
 * l'écriture, et la résolution peut y chercher n'importe quelle touche du clavier.
 */
const BY_CHORD: Readonly<Record<string, string | undefined>> = BOUND_CHORDS;

/**
 * Les octets qu'un accord doit envoyer au PTY, ou `null` s'il ne nous regarde pas.
 *
 * `null` est la réponse par défaut, et c'est ce qui rend la correction sûre : tout ce qui
 * n'est pas nommé ci-dessus — une touche composée avec ⌥, un `Ctrl-A` tapé directement, un
 * accélérateur de menu qui aurait échappé au système — repart intact vers xterm.js.
 *
 * Les modificateurs sont comparés **exactement** : `⌃` doit être relâché, et `⇧` fait
 * partie de l'accord depuis #91 — il n'est plus un motif de refus, il est **écrit** dans
 * la clé cherchée. Le résultat est le même pour tout ce qui existait avant : `⇧⌥←` compose
 * `"⇧⌥ArrowLeft"`, que la table ne nomme pas, donc la frappe repart au shell. Ce que ce
 * détour apporte en échange, c'est que la table peut nommer `⇧⏎` sans nommer `⏎`.
 *
 * L'ordre des signes est celui de `key-actions.ts` — `⇧` d'abord — pour que les deux
 * tables se lisent de la même façon.
 */
export function resolveKeyBinding(chord: KeyChord): string | null {
    if (chord.type !== "keydown") return null;
    if (chord.ctrlKey) return null;

    const shift = chord.shiftKey ? "⇧" : "";
    const pressed = `${shift}${chord.altKey ? "⌥" : ""}${chord.metaKey ? "⌘" : ""}${chord.key}`;
    return BY_CHORD[pressed] ?? null;
}
