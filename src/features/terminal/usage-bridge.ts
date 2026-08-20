import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { AccountUsage } from "@/shared/ipc";
import type { UsageBridge } from "./ports";

/**
 * Nom de l'event qui porte les deux quotas. Contrat avec `ACCOUNT_USAGE_EVENT` dans
 * `src-tauri/src/features/usage/commands.rs` : une chaîne que rien ne vérifie à la
 * compilation, comme celle des onglets et celle de la surveillance git.
 */
const ACCOUNT_USAGE_EVENT = "ash://account-usage";

/**
 * Le pont vers `features::usage` : **une lecture, un event, et rien d'autre**.
 *
 * Posé dans la feature terminal parce que c'est elle qui consomme ces deux valeurs — la
 * ligne de statut —, comme `git-bridge.ts` l'est pour la surveillance de `.git`. La fenêtre
 * de réglages, elle, ne passe pas par ici : elle parle de l'**interrupteur** de sondage,
 * qu'elle lit et écrit par `settings_usage`, et les deux questions n'ont ni le même
 * propriétaire ni la même forme.
 *
 * Rien ici ne déclenche d'appel réseau : `usage_snapshot` rend ce que le fil de fond a déjà
 * trouvé ([ADR-0016](../../../docs/adr/0016-ash-sort-sur-le-reseau.md)).
 */
export const tauriUsage: UsageBridge = {
    snapshot: () => invoke<AccountUsage>("usage_snapshot"),
    onAccountUsage: (handler) =>
        listen<AccountUsage>(ACCOUNT_USAGE_EVENT, (event) => {
            handler(event.payload);
        }),
};
