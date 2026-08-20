import { invoke } from "@tauri-apps/api/core";

import type { ActionOffer, ActionOutcome, BranchOverview, TabId } from "@/shared/ipc";
import type { AgentPause, BranchesBridge } from "./ports";

/**
 * L'implémentation réelle des deux ports : cinq commandes, et rien d'autre.
 *
 * Le pendant de `terminal/git-bridge.ts`, et posé pour la même raison : la feature qui
 * consomme une surface backend est celle qui écrit le pont vers elle. Le TypeScript ne
 * connaît de `features::git` et de `features::pty` que ces cinq noms et les types du
 * contrat partagé — jamais leur structure interne.
 */
export const tauriBranches: BranchesBridge = {
    branches: (worktreeRoot) =>
        invoke<BranchOverview | null>("git_branches", { worktreeRoot }),
    offers: (worktreeRoot, branch) =>
        invoke<ActionOffer[] | null>("git_branch_offers", { worktreeRoot, branch }).then(
            (offered) => offered ?? [],
        ),
    run: (worktreeRoot, action, branch) =>
        invoke<ActionOutcome | null>("git_branch_action", { worktreeRoot, action, branch }),
};

/**
 * La pause, telle que le backend la comprend : `SIGSTOP` puis `SIGCONT` sur le groupe en
 * avant-plan de l'onglet ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
 *
 * Il n'y a rien à composer ici, et c'est le point : aucun octet n'est écrit dans le PTY.
 */
export const tauriPause: AgentPause = {
    pause: (tabId: TabId) => invoke<void>("pty_pause", { tabId }),
    resume: (tabId: TabId) => invoke<void>("pty_resume", { tabId }),
};
