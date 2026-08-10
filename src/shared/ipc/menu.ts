import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Le contrat du menu applicatif, côté webview.
 *
 * Les accélérateurs (`⌘N`, `⌘W`, `⌘1`…) sont déclarés en Rust dans
 * `src-tauri/src/menu.rs` : sur macOS, un menu natif est à la fois le chemin clavier et
 * le chemin souris, et il consomme la touche avant que la webview — donc le shell — ne
 * la voie. Ce module ne fait que traduire l'identifiant reçu en action typée.
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
    | { kind: "select-tab"; position: number };

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
        default:
            break;
    }

    const position = id.startsWith("tab:select:") ? Number(id.slice("tab:select:".length)) : NaN;
    return Number.isInteger(position) && position >= 1 && position <= 9
        ? { kind: "select-tab", position }
        : null;
}
