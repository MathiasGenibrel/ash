import { invoke } from "@tauri-apps/api/core";

import type { BranchCard } from "@/shared/ipc";

/**
 * Le raccordement de la fiche de branche au backend qui la détient.
 *
 * Il vit dans `app/` et non dans la feature, comme `bottom-panel.ts` et `sidebar-column.ts`,
 * et pour la même raison : la feature ne connaît aucune commande Tauri, c'est le composition
 * root qui relie ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * **Il n'y a pas d'event.** La fiche est un fichier, pas un état vivant : elle est relue au
 * moment où on la regarde — panneau ouvert sur l'onglet `branch`, changement d'onglet actif,
 * geste sur le bouton — et rien ne la pousse. Une surveillance de `.ash/worktree.md` serait
 * un second abonnement FSEvents pour un document qu'on lit quelques secondes par jour ; le
 * jour où elle se justifiera, c'est `features/git` qui l'apportera, comme pour `.git`.
 */

/** Ce que la fenêtre sait demander au sujet d'une fiche. */
export interface BranchCardBinding {
    /** La fiche de ce worktree. `null` si elle ne se lit pas. */
    read(worktreeRoot: string): Promise<BranchCard | null>;
    /** Pose le journal dans le bloc `ash:log` — ou refuse, et la fiche rendue le dira. */
    writeLog(worktreeRoot: string): Promise<BranchCard | null>;
    /** Choisit où la fiche vit. `null` rend la main à la détection. */
    place(worktreeRoot: string, local: boolean | null): Promise<BranchCard | null>;
}

export function followBranchCard(): BranchCardBinding {
    // Un aller-retour qui n'aboutit pas ne laisse **aucune moitié d'état** : le backend n'a
    // rien écrit, et l'écran garde ce qu'il montrait. C'est la conduite déjà retenue pour la
    // colonne, les épingles et le panneau.
    const ask = async (
        command: string,
        args: Record<string, unknown>,
    ): Promise<BranchCard | null> => invoke<BranchCard | null>(command, args).catch(() => null);

    return {
        read: (worktreeRoot) => ask("branch_card", { worktreeRoot }),
        writeLog: (worktreeRoot) => ask("branch_card_write_log", { worktreeRoot }),
        place: (worktreeRoot, local) => ask("branch_card_place", { worktreeRoot, local }),
    };
}
