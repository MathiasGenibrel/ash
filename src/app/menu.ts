import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Le contrat du menu applicatif, côté webview.
 *
 * Les accélérateurs (`⌘T`, `⌘W`, `⌘1`…) sont déclarés en Rust dans
 * `src-tauri/src/menu.rs` : sur macOS, un menu natif est à la fois le chemin clavier et
 * le chemin souris, et il consomme la touche avant que la webview — donc le shell — ne
 * la voie. Ce module ne fait que traduire l'identifiant reçu en action typée.
 *
 * **L'event n'arrive qu'à la fenêtre que le backend a désignée**, et c'est lui qui décide
 * laquelle : une action de menu naît sans surface, et `route` dans `src-tauri/src/menu.rs`
 * lui en donne une à partir de la fenêtre au premier plan. C'est pourquoi il n'y a rien à
 * filtrer ici — la fenêtre de réglages ne reçoit tout simplement pas `tab:close` (#107).
 *
 * `⌃⇥` et `⌃⇧⇥` sont les deux seules à ne pas arriver par ici quand elles viennent du
 * clavier : leur entrée de menu existe, mais AppKit ne l'allume pas — voir l'en-tête de
 * `src-tauri/src/menu.rs`. C'est `shortcuts.ts` qui les capte, et il produit les mêmes
 * actions que celles-ci, de sorte que le composition root n'ait qu'une table à jouer.
 *
 * Il vit dans `app/`, et non dans `shared/ipc/` : un menu est un objet de fenêtre, dont
 * le seul lecteur est le composition root — c'est lui qui relie une action à la feature
 * qui sait la jouer. `shared/` est réservé à ce qui sert au moins deux features sans
 * porter la règle d'aucune, et cette table ne nomme aujourd'hui que des actions
 * d'onglet. Le pendant Rust, `src-tauri/src/menu.rs`, est posé de la même façon à côté
 * de son composition root.
 *
 * Les deux côtés partagent des chaînes que rien ne vérifie à la compilation : un
 * identifiant inconnu est donc ignoré ici, et un test Rust garde la table symétrique.
 */

const MENU_ACTION_EVENT = "ash://menu-action";

export type MenuAction =
    | { kind: "new-tab" }
    | { kind: "new-home-tab" }
    | { kind: "close-tab" }
    | { kind: "clear-scrollback" }
    /** `Cmd+1` … `Cmd+9`, à partir de 1. */
    | { kind: "select-tab"; position: number }
    /** `Ctrl+Tab` : l'onglet suivant, en bouclant. Voir `shortcuts.ts`. */
    | { kind: "next-tab" }
    /** `Ctrl+Shift+Tab` : l'onglet précédent, en bouclant. */
    | { kind: "previous-tab" }
    /** `Cmd+B` : replie ou déplie la colonne. */
    | { kind: "toggle-sidebar" };

/** S'abonne aux actions de menu. Rend de quoi se désabonner. */
export function onMenuAction(handle: (action: MenuAction) => void): Promise<UnlistenFn> {
    return listen<string>(MENU_ACTION_EVENT, (event) => {
        const action = parseMenuAction(event.payload);
        if (action !== null) handle(action);
    });
}

export function parseMenuAction(id: string): MenuAction | null {
    switch (id) {
        case "tab:new":
            return { kind: "new-tab" };
        case "tab:new-home":
            return { kind: "new-home-tab" };
        case "tab:close":
            return { kind: "close-tab" };
        case "tab:clear":
            return { kind: "clear-scrollback" };
        case "tab:next":
            return { kind: "next-tab" };
        case "tab:previous":
            return { kind: "previous-tab" };
        case "view:toggle-sidebar":
            return { kind: "toggle-sidebar" };
        default:
            break;
    }

    const position = id.startsWith("tab:select:") ? Number(id.slice("tab:select:".length)) : NaN;
    return Number.isInteger(position) && position >= 1 && position <= 9
        ? { kind: "select-tab", position }
        : null;
}
