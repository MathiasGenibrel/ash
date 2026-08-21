import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { StatusBarBridge } from "./ports";
import { parseStatusBarLayout } from "./status-bar";

/**
 * Nom de l'event qui porte la barre de statut — ce qu'elle montre, et dans quel ordre.
 * Contrat avec `STATUS_BAR_LAYOUT_EVENT` dans `src-tauri/src/features/theme/commands.rs` :
 * une chaîne que rien ne vérifie à la compilation, comme celle des quotas et celle des
 * onglets.
 */
const STATUS_BAR_LAYOUT_EVENT = "ash://status-bar-layout";

/**
 * Le pont vers la barre des vues 5c et 5e : **une lecture, trois demandes, un event**.
 *
 * Posé dans la feature terminal parce que c'est elle qui la consomme — la ligne de statut,
 * son menu contextuel et son mode édition —, comme `usage-bridge.ts` l'est pour les quotas.
 * Le couple est celui du thème : on lit une fois en s'affichant, puis c'est l'event qui tient
 * à jour, et la webview ne redemande jamais.
 *
 * Les trois demandes ne se ressemblent pas, et c'est délibéré : `toggle` envoie une
 * **intention** (le backend décide où un segment revient), `arrange` envoie une **valeur**
 * (la webview seule a mesuré ce qui est sous le pointeur), `reset` n'envoie rien du tout.
 * Aucune ne rend de résultat — la barre revient par l'event, pour toutes les trois.
 */
export const tauriStatusBar: StatusBarBridge = {
    layout: async () => parseStatusBarLayout(await invoke<unknown>("status_bar_layout")),
    toggle: (segment) => invoke<void>("toggle_status_bar_segment", { segment }),
    arrange: (items) => invoke<void>("set_status_bar_layout", { items }),
    reset: () => invoke<void>("reset_status_bar_layout"),
    onLayout: (handler) =>
        listen<unknown>(STATUS_BAR_LAYOUT_EVENT, (event) => {
            handler(parseStatusBarLayout(event.payload));
        }),
};
