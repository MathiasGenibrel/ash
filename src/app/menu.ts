import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
    CapturePreview,
    ConflictChoice,
    KeyStroke,
    ShortcutsReport,
} from "@/features/settings";

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

/**
 * L'annonce d'une liaison qui a changé. Elle ne porte **rien** : c'est un signal.
 *
 * Chaque surface redemande ce dont elle a besoin — le pied de la colonne les glyphes d'une
 * action, la fenêtre de réglages son instantané. Faire voyager la liste ferait de chaque
 * abonné le détenteur d'une copie
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
const SHORTCUTS_CHANGED_EVENT = "ash://shortcuts-changed";

/**
 * L'identifiant de « nouvel onglet », pour les surfaces qui **annoncent** son raccourci.
 *
 * Un identifiant d'action, pas une combinaison : c'est celui que le menu émet déjà, et il
 * ne bouge pas quand la touche bouge. Le nommer ici évite qu'une chaîne le désigne au fond
 * du composition root.
 */
export const NEW_TAB_ACTION = "tab:new";

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

/**
 * Les sept verbes de la section `shortcuts` des réglages (spec §4.4, issue #22).
 *
 * Les liaisons sont **détenues en Rust** (`features::shortcuts`), et le menu natif en dérive :
 * ce module ne connaît que les noms de commandes, jamais les combinaisons. Une table recopiée
 * ici aurait fini par annoncer un raccourci que le menu ne joue plus.
 *
 * Six d'entre eux rendent l'instantané **entier**, et c'est ce qui rend les deux surfaces
 * incapables de diverger : quand la réponse arrive, le backend a déjà refait le menu.
 */
export function menuShortcuts(): Promise<ShortcutsReport> {
    return invoke<ShortcutsReport>("menu_shortcuts");
}

/**
 * Éteint les entrées d'Ash le temps d'une capture, et les rallume après.
 *
 * Sur macOS, un accélérateur de menu est consommé **avant** la webview : sans ce geste, `⌘W`
 * frappé pendant une capture fermerait la fenêtre au lieu d'être lu. Voir
 * `shortcut_listening` dans `src-tauri/src/menu.rs`, où la mesure est expliquée.
 */
export function listenForShortcut(active: boolean): Promise<void> {
    return invoke<void>("shortcut_listening", { active });
}

/**
 * L'action à qui appartient une frappe **que le menu natif n'a pas consommée**.
 *
 * Deux chords sont dans ce cas, et deux seulement — `⌃⇥` et `⌃⇧⇥` (voir `shortcuts.ts`). La
 * webview les capte, puis vient demander à qui elles appartiennent : elle ne connaît ni
 * combinaison, ni table de touches, ni règle de comparaison. Une liaison déplacée cesse donc
 * de répondre à son ancienne touche sans qu'une ligne de TypeScript ne l'apprenne.
 *
 * La réponse est un identifiant d'action — le même que porte `ash://menu-action` —, donc
 * elle se relit par le traducteur qui existe déjà.
 */
export async function shortcutOwner(stroke: KeyStroke): Promise<MenuAction | null> {
    const held = await invoke<string | null>("shortcut_owner", { stroke });
    return held === null ? null : parseMenuAction(held);
}

/**
 * La combinaison en vigueur d'une action, telle que macOS l'écrit — vide s'il n'y en a
 * aucune.
 *
 * L'autre sens de la même question : ce qu'une surface **affiche**. Le pied de la colonne
 * annonce `⌘T` parce qu'il le demande, et non parce qu'il le sait.
 */
export function shortcutKeys(action: string): Promise<string> {
    return invoke<string>("shortcut_keys", { action });
}

/** S'abonne aux changements de liaison. Rend de quoi se désabonner. */
export function onShortcutsChanged(handle: () => void): Promise<UnlistenFn> {
    return listen<null>(SHORTCUTS_CHANGED_EVENT, () => {
        handle();
    });
}

export function previewShortcut(stroke: KeyStroke): Promise<CapturePreview> {
    return invoke<CapturePreview>("shortcut_preview", { stroke });
}

export function bindShortcut(action: string, stroke: KeyStroke): Promise<ShortcutsReport> {
    return invoke<ShortcutsReport>("shortcut_bind", { action, stroke });
}

export function clearShortcut(action: string): Promise<ShortcutsReport> {
    return invoke<ShortcutsReport>("shortcut_clear", { action });
}

export function resetShortcut(action: string): Promise<ShortcutsReport> {
    return invoke<ShortcutsReport>("shortcut_reset", { action });
}

export function resetAllShortcuts(): Promise<ShortcutsReport> {
    return invoke<ShortcutsReport>("shortcut_reset_all");
}

export function resolveShortcutConflict(choice: ConflictChoice): Promise<ShortcutsReport> {
    return invoke<ShortcutsReport>("shortcut_resolve", { choice });
}

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
