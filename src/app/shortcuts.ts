import { readStroke, type KeyStroke } from "@/features/settings";

import type { MenuAction } from "./menu";

/**
 * Les raccourcis que le menu natif ne peut pas capter, et eux seuls.
 *
 * Il n'y en a que deux — `⌃⇥` et `⌃⇧⇥` — et ce n'est pas un choix de commodité : `muda`
 * donne à `Key::Tab` l'équivalent clavier `⇥` (U+21E5) là où `NSEvent` rend `\t`
 * (U+0009), donc `-[NSMenu performKeyEquivalent:]` ne fait jamais correspondre l'entrée.
 * Le raisonnement complet, et la mesure qui l'établit, sont dans l'en-tête de
 * `src-tauri/src/menu.rs`. Les entrées de menu restent déclarées là-bas : elles sont
 * visibles, cliquables, et produisent les mêmes actions.
 *
 * Ce module est le seul du frontend à lire une touche brute, et il vit dans `app/` pour
 * la même raison que `menu.ts` : un raccourci de fenêtre n'appartient à aucune feature.
 *
 * **Ce que l'écoute voit, et ce qu'elle ne voit pas.** Elle n'est posée que par
 * `app/main.ts`, sur le document de la fenêtre principale. La fenêtre de réglages a le
 * sien (`settings.html` → `app/settings.ts`, voir `vite.config.ts`) : ses champs de
 * saisie ne passent jamais par ici, et `⌃⇥` y reste ce que la webview en fait. La porte
 * est étroite pour la même raison qu'elle est en capture : elle ne retient que ce que le
 * menu natif ne peut pas capter, et tout ce qui ne correspond pas ressort intact.
 *
 * **`Tab` seul n'est pas un raccourci.** C'est la contrainte qui a dicté la forme de
 * [`matchShortcut`] : elle exige `Control`, et refuse tout ce qui porte `Cmd` ou `Option`
 * — sans quoi la complétion de `zsh` s'arrêterait de fonctionner, ce qui coûterait
 * infiniment plus cher que le raccourci ne rapporte.
 *
 * **Ce module ne sait plus quelle action une touche joue, et c'est le point (#22).** Les
 * liaisons sont réglables et détenues en Rust (`features::shortcuts`) ; ici, on ne décide que
 * d'une chose — **cette frappe est-elle de celles que le menu natif ne peut pas consommer** —
 * puis on demande au backend à qui elle appartient. Une combinaison recopiée ici serait la
 * seconde liste que tout ce travail évite, et `⌃⇥` continuerait de changer d'onglet après
 * qu'on l'a déplacé ailleurs.
 *
 * La porte reste donc étroite pour la même raison qu'avant, et elle laisse passer exactement
 * ce qu'elle laissait passer : `Ctrl+Tab`, avec ou sans `Shift`. Ce qui a changé est ce qui
 * s'ensuit — plus rien n'est joué sans que le backend l'ait nommé. Le jour où `muda`
 * corrigera son équivalent clavier (voir l'en-tête de `src-tauri/src/menu.rs`), ce fichier
 * s'efface en entier : la porte se ferme, et le menu natif reprend les deux entrées.
 */

/** Ce qu'un `keydown` a d'utile ici. Un objet nu, pour que la règle se teste sans DOM. */
export interface KeyPress {
    /** `KeyboardEvent.key` : « Tab », quel que soit l'état de `Shift`. */
    readonly key: string;
    /** `KeyboardEvent.code` : ce que le backend lit, indépendamment de la disposition. */
    readonly code: string;
    readonly ctrlKey: boolean;
    readonly shiftKey: boolean;
    readonly metaKey: boolean;
    readonly altKey: boolean;
}

/**
 * Cette frappe est-elle de celles que le menu natif ne peut pas consommer ?
 *
 * C'est la **seule** règle de ce module, et elle ne parle pas de raccourcis : elle décrit une
 * limite d'AppKit. Tout ce qui n'est pas `Ctrl+Tab` — avec ou sans `Shift` — part au terminal
 * inchangé : `Tab` seul (la complétion de `zsh`), `Cmd+Tab` (le commutateur d'applications de
 * macOS), `Ctrl+Alt+Tab`, et le reste du clavier.
 *
 * Ce que la frappe **joue**, en revanche, ne se décide pas ici : voir [`installShortcuts`].
 */
export function withheldFromTheMenu(press: KeyPress): boolean {
    if (press.key !== "Tab") return false;
    return press.ctrlKey && !press.metaKey && !press.altKey;
}

/**
 * Pose l'écoute sur le document, et rend de quoi la retirer.
 *
 * En phase de **capture** : xterm.js écoute le `keydown` de son `textarea`, qui est un
 * descendant, donc l'arrêter ici est la seule façon de garantir que `⌃⇥` ne parte pas
 * dans le PTY. `preventDefault` en plus de `stopPropagation`, parce que WKWebView ferait
 * sinon avancer son propre focus au `Tab`.
 *
 * Rien n'est arrêté quand la frappe ne correspond pas : c'est ce qui laisse la
 * complétion, les raccourcis d'édition de ligne et toute la saisie intacts.
 */
export function installShortcuts(
    target: Document,
    owner: (stroke: KeyStroke) => Promise<MenuAction | null>,
    handle: (action: MenuAction) => void,
): () => void {
    const onKeyDown = (event: KeyboardEvent): void => {
        if (!withheldFromTheMenu(event)) return;
        // La frappe est arrêtée **avant** de savoir ce qu'elle joue, et il n'y a pas d'autre
        // choix : un `keydown` se décide sur-le-champ, la réponse du backend arrive après. Ce
        // n'est pas une perte — `⌃⇥` était déjà retenu sans condition, c'est ce que la porte a
        // toujours fait, et `Tab` seul reste intact.
        event.preventDefault();
        event.stopPropagation();
        void owner(readStroke(event)).then((action) => {
            // Personne ne tient cette touche : le raccourci a été déplacé ailleurs, et il n'y
            // a rien à jouer. C'est ici que le rebinding devient vrai.
            if (action !== null) handle(action);
        });
    };

    target.addEventListener("keydown", onKeyDown, { capture: true });
    return () => {
        target.removeEventListener("keydown", onKeyDown, { capture: true });
    };
}
