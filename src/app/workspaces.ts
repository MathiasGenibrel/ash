import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { Workspaces } from "@/shared/ipc";

/**
 * Les worktrees épinglés et les lignes repliées — ce que la colonne garde d'une session à
 * l'autre (spec §3.1, §5.2).
 *
 * **L'état vit en Rust** (`src-tauri/src/features/workspaces/`), et ce module ne fait que le
 * relayer : c'est la règle du thème, de la taille de police et des onglets — le frontend
 * affiche un état, il ne le détient pas
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)). Pour une épingle, la question
 * ne se pose même pas : elle doit survivre à la fenêtre qui l'affiche.
 *
 * Un geste — épingler, replier — ne pose donc rien ici : il part au backend, qui retient,
 * écrit, et **annonce** l'état entier par `ash://workspaces`. C'est ce retour qui redessine
 * la colonne. Poser le résultat au passage donnerait deux routes vers l'écran, et il faudrait
 * qu'elles restent d'accord.
 *
 * Il vit dans `app/` et non dans la sidebar pour la raison qui y met `theme.ts` et
 * `select-tab.ts` : la sidebar n'appelle aucune commande Tauri, c'est le composition root qui
 * relie ce que le backend détient à ce qui sait le peindre.
 */

/** Nom de l'event du backend. Contrat avec `features::workspaces::commands`. */
const WORKSPACES_EVENT = "ash://workspaces";

/** Ce sur quoi la colonne s'ouvre tant que le backend n'a pas répondu : aucune épingle. */
export const NO_WORKSPACES: Workspaces = { pinned: [], collapsed: [] };

/** Ce qu'on rend à qui suit l'état de la colonne. */
export interface WorkspacesChanges {
    /** L'état en cours — une ligne qui se dessine maintenant n'attend pas le prochain geste. */
    readonly current: Workspaces;
    /** Prévient après chaque changement. Rend de quoi se désabonner. */
    subscribe(listener: (workspaces: Workspaces) => void): () => void;
}

/** Ce que `followWorkspaces` rend : de quoi suivre, de quoi agir, et de quoi attendre. */
export interface WorkspacesBinding {
    changes: WorkspacesChanges;
    /** Épingle ou désépingle un worktree. Rejette si l'appel n'aboutit pas. */
    pin(worktreeRoot: string, pinned: boolean): Promise<void>;
    /** Replie ou déplie une ligne — un worktree, ou un groupe de dépôt. */
    collapse(key: string, collapsed: boolean): Promise<void>;
    /** Le raccordement à l'état que le backend détient. Rejette s'il n'a pas lieu. */
    ready: Promise<void>;
}

/**
 * Relie la fenêtre à l'état de colonne que le backend détient.
 *
 * La fonction n'est pas `async`, pour la même raison que `followThemeMode` : ses abonnés
 * doivent pouvoir s'inscrire avant que le premier aller-retour ne soit revenu. Une colonne
 * sans épingle est un premier démarrage parfaitement honnête, donc un échec de raccordement
 * ne casse rien — il n'y a que des épingles à perdre, et le message est dans `ready`.
 */
export function followWorkspaces(): WorkspacesBinding {
    let current: Workspaces = NO_WORKSPACES;
    const listeners = new Set<(workspaces: Workspaces) => void>();

    const apply = (workspaces: Workspaces): void => {
        current = workspaces;
        for (const listener of listeners) listener(workspaces);
    };

    const ready = (async (): Promise<void> => {
        // L'abonnement **avant** la lecture : un geste joué entre les deux — il n'y en a pas
        // au démarrage, mais rien ne l'interdit — se perdrait dans l'autre ordre.
        await listen<Workspaces>(WORKSPACES_EVENT, (event) => {
            apply(event.payload);
        });
        apply(await invoke<Workspaces>("workspaces"));
    })();

    return {
        changes: {
            get current() {
                return current;
            },
            subscribe(listener) {
                listeners.add(listener);
                return () => listeners.delete(listener);
            },
        },
        pin: (worktreeRoot, pinned) => invoke("workspaces_pin", { worktreeRoot, pinned }),
        collapse: (key, collapsed) => invoke("workspaces_collapse", { key, collapsed }),
        ready,
    };
}
