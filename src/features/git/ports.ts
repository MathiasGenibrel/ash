/**
 * Les deux frontières de la feature git côté webview.
 *
 * Elles sont derrière des interfaces pour la raison qui vaut partout dans ce dépôt : sans
 * ça, la règle qui compte ici — **rien ne s'exécute sans un geste, et une pause est un
 * `SIGSTOP`, jamais une touche** — ne serait vérifiable qu'en lançant l'application sur un
 * vrai dépôt avec un vrai agent dedans.
 *
 * Le second port ne s'appelle pas « pty » par hasard : la pause appartient à la feature qui
 * tient les processus, pas à celle qui parle de branches. `features/git` demande ; elle n'a
 * pas la main sur un agent.
 */

import type { ActionOffer, ActionOutcome, BranchAction, BranchOverview, TabId } from "@/shared/ipc";

/** Ce que la popup attend du backend git. */
export interface BranchesBridge {
    /**
     * Les branches d'un worktree, groupées, situées, et avec les agents qu'elles menacent.
     *
     * `null` quand `git` n'a pas répondu. L'écran dit alors qu'il n'a pas su lire ; il
     * n'affiche pas une liste vide, qui se lirait comme un dépôt sans branche.
     */
    branches(worktreeRoot: string): Promise<BranchOverview | null>;
    /**
     * Ce que `⌘⏎` propose pour une branche, refus compris.
     *
     * Relu au moment où on le montre, et non à l'ouverture de la popup : un autre worktree a
     * pu prendre la branche entre les deux.
     */
    offers(worktreeRoot: string, branch: string): Promise<readonly ActionOffer[]>;
    /**
     * Lance une action. **Jamais appelée sans un geste explicite de l'utilisateur.**
     *
     * Le backend relit la liste avant d'agir et refuse ce qu'il ne retrouve pas : ce qui
     * part d'ici est un nom, jamais une ligne de commande.
     */
    run(worktreeRoot: string, action: BranchAction, branch: string): Promise<ActionOutcome | null>;
}

/**
 * Ce que la confirmation attend de qui tient les processus.
 *
 * « Pause » veut dire `SIGSTOP` sur le groupe en avant-plan, et **rien d'autre**
 * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)) : aucune touche
 * écrite dans le PTY, aucune interprétation de ce que l'outil affiche. Le port n'a donc pas
 * de méthode « interrompre » ni « envoyer » — il n'y a rien d'autre à offrir.
 *
 * `resume` est là parce qu'un agent laissé arrêté sans moyen de le relancer est un piège :
 * il n'émet plus de hook, donc plus d'état, et seule la fiche de son onglet dit encore qu'il
 * attend un signal.
 */
export interface AgentPause {
    pause(tabId: TabId): Promise<void>;
    resume(tabId: TabId): Promise<void>;
}
