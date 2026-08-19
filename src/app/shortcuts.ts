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
 * **Ce que le rebinding (#22) laisse ouvert ici, et qu'il faut savoir.** Les liaisons sont
 * désormais réglables et détenues en Rust (`features::shortcuts`) ; cette porte-ci, elle,
 * reste écrite en dur sur `⌃⇥`. Donner une autre combinaison à `Select Next Tab` dans les
 * réglages change donc le menu — et la nouvelle touche fonctionne —, mais `⌃⇥` continue
 * d'atteindre la même action par ce chemin. Ces deux entrées sont les seules dans ce cas,
 * précisément parce qu'elles sont les seules qu'AppKit n'allume pas. La sortie n'est pas de
 * recopier une liaison ici — ce serait la seconde liste que #110 interdit — mais de faire
 * lire à `main.ts` la combinaison en vigueur, ou de voir disparaître ce module le jour où
 * `muda` corrigera son équivalent clavier (voir l'en-tête de `src-tauri/src/menu.rs`).
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
