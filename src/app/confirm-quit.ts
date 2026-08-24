import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { askForConfirmation, composeConfirmBox, type CloseAnswer } from "@/features/terminal";
import { presentAgentState } from "@/shared/agent-state";
import type { TabInfo } from "@/shared/ipc";
import { row, text, type UiComponent } from "@/shared/ui";

/**
 * La question posée quand `⌘Q` arrive et qu'un agent tourne (issue #177, spec §4.4).
 *
 * **Le critère n'est pas ici.** C'est le backend qui décide s'il faut demander, parce que
 * c'est lui qui détient les onglets et leurs agents
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)) : cet event n'arrive **que**
 * quand la réponse est oui, et il porte les onglets concernés. Le frontend rend la modale,
 * il ne compte rien et ne filtre rien — s'il le faisait, il faudrait qu'il détienne une
 * copie des onglets, et elle aurait un âge.
 *
 * Ce module vit dans `app/` et non dans `features/terminal/`, pour la raison exacte qui y
 * met `select-tab.ts` : quitter est un objet d'application, pas de terminal. Ce qu'il
 * emprunte à la feature est la **boîte** — le voile, `Échap`, le focus sur `Annuler` —, qui
 * y vit parce que sa feuille de style y vit.
 *
 * Les deux côtés partagent une chaîne que rien ne vérifie à la compilation, comme celles du
 * menu et de la sélection d'onglet ; le contrat est `CONFIRM_QUIT_EVENT` dans
 * `src-tauri/src/features/quit/commands.rs`.
 */
const CONFIRM_QUIT_EVENT = "ash://confirm-quit";

/** La classe d'une ligne d'agent, peinte par `features/terminal/terminal.css`. */
const ITEM_CLASS = "ash-confirm-item";

/**
 * Ce que la boîte dit : combien d'agents, puis lesquels — un par ligne.
 *
 * Le chemin nommé est le `cwd` de l'onglet, et pas la racine de son worktree : deux agents
 * lancés dans deux sous-dossiers du même worktree y seraient devenus deux lignes
 * identiques, et la liste sert précisément à reconnaître ce qu'on va perdre.
 *
 * L'état vient de `shared/agent-state`, la même source que la sidebar et la ligne de
 * statut — un `waiting` nommé autrement ici serait un sixième mot pour cinq états.
 */
export function composeQuitBox(
    appName: string,
    running: readonly TabInfo[],
    answer: CloseAnswer,
): UiComponent {
    const count = running.length;
    const headline =
        count === 1
            ? `1 agent tourne. Quitter ${appName} ?`
            : `${String(count)} agents tournent. Quitter ${appName} ?`;

    return composeConfirmBox(
        [
            text(headline),
            ...running.map((tab) =>
                row(text(`${tab.cwd} — ${presentAgentState(tab.state).label}`)).class(ITEM_CLASS),
            ),
        ],
        "Quitter",
        answer,
    );
}

/**
 * Pose la question dans `host` à chaque fois que le backend la fait remonter.
 *
 * `Annuler` ne rappelle **rien** : annuler, c'est ne rien faire. Aucun PTY n'est fermé,
 * aucun état d'agent n'est perdu, et le laissez-passer du backend n'a pas été ouvert — le
 * `⌘Q` suivant repose donc la question. Seul « Quitter » traverse la frontière, et l'arrêt
 * qu'il déclenche est celui d'avant cette tranche.
 *
 * Une boîte déjà ouverte fait ignorer la demande suivante : deux `⌘Q` de suite ne doivent
 * pas empiler deux voiles dont l'un cacherait l'autre.
 */
export function onConfirmQuit(host: HTMLElement, appName: string): Promise<UnlistenFn> {
    let asking = false;

    return listen<TabInfo[]>(CONFIRM_QUIT_EVENT, (event) => {
        if (asking) return;
        asking = true;

        const running = event.payload;
        void askForConfirmation(host, (answer) => composeQuitBox(appName, running, answer)).then(
            async (quit) => {
                asking = false;
                if (quit) await invoke("quit_now");
            },
        );
    });
}
