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
 * **`Tab` seul n'est pas un raccourci.** C'est la contrainte qui a dicté la forme de
 * [`matchShortcut`] : elle exige `Control`, et refuse tout ce qui porte `Cmd` ou `Option`
 * — sans quoi la complétion de `zsh` s'arrêterait de fonctionner, ce qui coûterait
 * infiniment plus cher que le raccourci ne rapporte.
 */

/** Ce qu'un `keydown` a d'utile ici. Un objet nu, pour que la règle se teste sans DOM. */
export interface KeyStroke {
    /** `KeyboardEvent.key` : « Tab », quel que soit l'état de `Shift`. */
    readonly key: string;
    readonly ctrlKey: boolean;
    readonly shiftKey: boolean;
    readonly metaKey: boolean;
    readonly altKey: boolean;
}

/**
 * Traduit une frappe en action, ou rend `null` — et `null` veut dire « laisse passer ».
 *
 * Tout ce qui n'est pas exactement `Ctrl+Tab` ou `Ctrl+Shift+Tab` part au terminal
 * inchangé : `Tab`, `Cmd+Tab` (le commutateur d'applications de macOS), `Ctrl+Alt+Tab`,
 * et le reste du clavier.
 */
export function matchShortcut(stroke: KeyStroke): MenuAction | null {
    if (stroke.key !== "Tab") return null;
    if (!stroke.ctrlKey || stroke.metaKey || stroke.altKey) return null;
    return stroke.shiftKey ? { kind: "previous-tab" } : { kind: "next-tab" };
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
    handle: (action: MenuAction) => void,
): () => void {
    const onKeyDown = (event: KeyboardEvent): void => {
        const action = matchShortcut(event);
        if (action === null) return;
        event.preventDefault();
        event.stopPropagation();
        handle(action);
    };

    target.addEventListener("keydown", onKeyDown, { capture: true });
    return () => {
        target.removeEventListener("keydown", onKeyDown, { capture: true });
    };
}
