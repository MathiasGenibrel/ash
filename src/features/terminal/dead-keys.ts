/**
 * Le rattrapage d'une touche morte que xterm.js perd **sous WebKit**.
 *
 * Sur un clavier AZERTY, une touche morte suivie d'une lettre qui ne se combine pas avec
 * elle — `^` puis `d` — doit écrire `^d`. Sous WKWebView, xterm.js écrit `^^` : l'accent
 * part deux fois, et la lettre ne part jamais. `^` puis `e` reste correct, parce que `ê`
 * existe : seule la combinaison **impossible** est touchée.
 *
 * Ce n'est pas une régression d'Ash, et pas davantage un défaut de macOS. Mesuré sur une
 * page xterm.js **nue**, sans aucune option d'Ash ni gestionnaire de touches :
 *
 * | Cas                        | Moteur | Résultat |
 * |----------------------------|--------|----------|
 * | xterm.js 6.0.0 nu          | WebKit | `B^^Z` ❌ |
 * | xterm.js 5.5.0 nu          | WebKit | `B^^Z` ❌ |
 * | xterm.js 6.0.0 nu          | Blink  | `B^dZ` ✅ |
 * | `<textarea>` natif         | WebKit | `B^dZ` ✅ |
 *
 * Le `<textarea>` natif sous le **même** moteur, avec les **mêmes** frappes, rend le bon
 * texte : WebKit seul n'est pas fautif, et épingler une version d'xterm.js ne servirait à
 * rien — 5.5.0 échoue à l'identique. C'est la rencontre des deux, et elle échappe à
 * l'amont parce que xterm.js se teste sur Chromium.
 *
 * ## Ce que WebKit envoie, et pourquoi xterm.js s'y perd
 *
 * Les deux traces, relevées sur la page nue :
 *
 * ```
 * `^` puis `e`  →  compositionend(data="ê")   keydown(key="ê",  keyCode=229)  → émet `ê`
 * `^` puis `d`  →  compositionend(data="^")   keydown(key="^d", keyCode=68)   → émet `^`
 * ```
 *
 * Quand la composition **aboutit**, WebKit clôt sur le caractère composé et le `keydown`
 * qui suit porte ce seul caractère : xterm.js l'émet une fois, tout va bien.
 *
 * Quand elle **échoue**, WebKit clôt la composition sur l'accent seul — xterm.js l'émet,
 * c'est la première sortie, et elle est juste — puis envoie un `keydown` dont le `key`
 * porte la chaîne composée **entière**, `"^d"`. xterm.js la traite comme une touche
 * imprimable et n'en émet que le premier caractère : l'accent repart. La lettre, elle,
 * n'arrive que par l'`input` qui suit, et xterm.js l'écarte.
 *
 * ## La règle appliquée ici
 *
 * Ce module ne corrige pas xterm.js : il **complète** ce qu'il a perdu. Après un
 * `compositionend` de données `D`, si le `keydown` immédiatement suivant porte un `key` qui
 * **commence par `D` et le dépasse**, alors ce dépassement est la lettre jamais émise —
 * `"^d"` moins `"^"` donne `"d"`. La vue l'écrit dans le PTY et consomme la frappe, ce qui
 * supprime du même geste l'accent en double.
 *
 * Le cas qui marche n'est pas touché : `key` y vaut exactement `D`, il n'y a pas de
 * dépassement, et la règle rend `null`. Une frappe ordinaire non plus : sans
 * `compositionend` juste avant, il n'y a rien à comparer.
 *
 * **C'est un contournement, et il s'appuie sur un comportement qu'aucun contrat ne
 * promet** — ni celui de xterm.js, ni celui de WebKit. Il est écrit ici, isolé et testé,
 * pour qu'il se retire d'un bloc le jour où l'amont corrigera. Sa vérification est
 * manuelle et tient en une frappe : dans un onglet, `^` puis `d` doit écrire `^d`.
 * `bun test` couvre la règle, jamais le clavier — il n'a ni WKWebView ni touche morte.
 */

import type { KeyChord } from "./key-bindings";

/**
 * L'état d'une composition qui vient de se clore, le temps d'une frappe.
 *
 * Un objet plutôt qu'une fonction pure : la règle a besoin de savoir ce que le
 * `compositionend` **précédent** a produit, et cette mémoire dure exactement un `keydown`.
 * Une instance par onglet — la vue la possède —, parce que deux onglets composent
 * indépendamment.
 */
export class DeadKeyRepair {
    /**
     * Le texte du dernier `compositionend`, ou `null` si la frappe en cours ne suit pas
     * une composition.
     *
     * Consommé par le premier `keydown` qui vient, quel qu'il soit : c'est ce qui borne la
     * règle à la frappe immédiatement suivante, et l'empêche de rapprocher une composition
     * d'une lettre tapée dix secondes plus tard.
     */
    private pending: string | null = null;

    /** Une composition vient de se clore sur `data`. */
    compositionEnded(data: string): void {
        this.pending = data;
    }

    /**
     * Ce que xterm.js va perdre sur ce `keydown`, ou `null` s'il n'y a rien à rattraper.
     *
     * Rendre `null` est le défaut, et c'est le bon défaut : la frappe repart alors intacte
     * vers xterm.js. Se tromper dans ce sens ne coûte que le défaut d'origine ; se tromper
     * dans l'autre avalerait une frappe légitime.
     */
    resolveKeyDown(chord: KeyChord): string | null {
        // Un `keyup` ne consomme rien : la vue branche ce rattrapage sur le gestionnaire de
        // xterm.js, qui voit les deux sens de chaque touche. Fermer la fenêtre sur un
        // relâchement ferait dépendre le correctif de l'ordre où WebKit les envoie.
        if (chord.type !== "keydown") return null;

        const composed = this.pending;
        // Consommé ensuite, et quoi qu'il arrive : un `keydown` qui ne correspond pas ferme
        // quand même la fenêtre, sans quoi la composition resterait à attendre.
        this.pending = null;

        if (composed === null || composed === "") return null;
        // Une frappe avec ⌘ ou ⌃ n'est pas la lettre qui clôt une composition, et son
        // accord appartient aux tables voisines.
        if (chord.metaKey || chord.ctrlKey) return null;
        if (!chord.key.startsWith(composed)) return null;
        // Pas de dépassement : la composition a abouti, xterm.js émettra juste. C'est la
        // branche du `^e → ê`, et ne rien faire est ici tout le travail.
        if (chord.key.length === composed.length) return null;

        return chord.key.slice(composed.length);
    }
}
