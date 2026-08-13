import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { TabId } from "@/features/terminal";

/**
 * L'onglet que le backend demande de rendre actif, quand la demande vient de l'extérieur
 * de la webview.
 *
 * Aujourd'hui il n'y a qu'un émetteur : le **clic sur une bannière macOS** (spec §8). La
 * bannière a voyagé avec l'identifiant de son onglet, macOS le rend à Ash par un délégué
 * quand l'utilisateur clique, et le backend décide que cet onglet-là doit être devant.
 *
 * Ce module vit dans `app/` et non dans `shared/ipc/`, pour la raison exacte qui y met
 * `menu.ts` : la sélection d'onglet est un objet de fenêtre, dont le seul lecteur est le
 * composition root — c'est lui qui relie une demande à la feature qui sait la jouer. Le
 * pendant Rust est posé de la même façon, dans `src-tauri/src/lib.rs` : ni `agents` ni
 * `notifications` ne peut porter cet event, la première ignorant ce qu'est une fenêtre et la
 * seconde ce qu'est un onglet.
 *
 * **La sélection reste détenue ici, et ce n'est pas une entorse à
 * [ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md).** L'ADR donne au backend
 * l'*état* d'un agent — jamais reconstruit côté TypeScript — pas la *vue* qu'on en a : le
 * jour du démon `ashd`, plusieurs vues partageront les mêmes PTY et chacune tiendra sa
 * propre sélection. Ce qui traverse la frontière est donc la décision du backend — « cet
 * onglet » —, et le frontend la rend.
 *
 * Les deux côtés partagent une chaîne que rien ne vérifie à la compilation, comme celle du
 * menu applicatif ; le contrat est `SELECT_TAB_EVENT` dans `src-tauri/src/lib.rs`.
 */
const SELECT_TAB_EVENT = "ash://select-tab";

/** S'abonne aux sélections venues du backend. Rend de quoi se désabonner. */
export function onSelectTab(handle: (tabId: TabId) => void): Promise<UnlistenFn> {
    return listen<TabId>(SELECT_TAB_EVENT, (event) => {
        handle(event.payload);
    });
}
