/**
 * Passer un rebase arrêté à l'agent qui tourne déjà dans l'onglet (spec §7.4).
 *
 * Le geste tient en trois temps, et **leur ordre est la règle** :
 *
 * 1. demander au backend le prompt — s'il n'y a rien d'arrêté, il n'y a rien à faire ;
 * 2. **sélectionner l'onglet de destination**, avant que quoi que ce soit ne soit écrit.
 *    [ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md) le demande
 *    explicitement : écrire dans un terminal qu'on ne regarde pas viole la première de ses
 *    trois conditions, celle qui veut que le texte soit **visible** ;
 * 3. demander la composition. Le backend arbitre — prompt non vide, onglet sans agent
 *    reconnu, tour en cours — et rend ce qui s'est passé.
 *
 * Rien ici n'envoie quoi que ce soit, et rien ici n'en a le moyen : la seule commande
 * appelée est `pty_compose`, dont le texte ne porte jamais de saut de ligne. Le `⏎` reste à
 * l'utilisateur, et c'est ce que le libellé rendu dit mot pour mot.
 *
 * Ce module ne dessine rien. C'est la vue `conflicts` du panneau bas (#24) qui l'appellera,
 * et l'onglet de merge (#30) réutilisera la **même** composition côté Rust pour ce qu'il
 * n'aura pas résolu.
 */

import { isShell } from "@/shared/ipc";
import type { ComposeOutcome, Tab, TabId } from "@/shared/ipc";

import type { GitBridge, PtyBridge } from "./ports";

/**
 * Ce que l'écran doit dire après le geste.
 *
 * Le `tone` sépare ce qui a été écrit de ce qui ne l'a pas été ; le `message` est le texte
 * que l'utilisateur lit. Celui de `typed` est **mot pour mot** celui d'ADR-0015 : la
 * franchise de ce moment-là est la décision elle-même, pas une formulation qu'on ajuste.
 */
export interface ComposeNotice {
    readonly tone: "typed" | "queued" | "refused";
    readonly message: string;
}

/** De quoi passer la main : d'où vient le conflit, et à quel onglet on le passe. */
export interface HandOver {
    readonly worktreeRoot: string;
    readonly tabId: TabId;
}

/**
 * Ce dont le geste a besoin, injecté.
 *
 * `selectTab` vient du composition root : la sélection d'onglet vit côté fenêtre, et cette
 * feature ne la détient pas plus qu'elle ne détient l'état git.
 */
export interface HandOverDeps {
    readonly git: GitBridge;
    readonly pty: PtyBridge;
    readonly selectTab: (tabId: TabId) => void;
}

const NOTICES: Record<ComposeOutcome, ComposeNotice> = {
    written: { tone: "typed", message: "ash typed this for you — not sent yet" },
    queued: { tone: "queued", message: "queued behind the current turn — not sent yet" },
    "prompt-not-empty": {
        tone: "refused",
        message: "there is already something in this prompt — ash wrote nothing",
    },
    "no-agent": {
        tone: "refused",
        message: "no agent is running in this tab — ash wrote nothing",
    },
};

/**
 * Rédige le prompt de conflit dans l'onglet visé, et rend ce qu'il faut en dire.
 *
 * `null` quand rien n'est arrêté dans ce worktree : il n'y a alors ni onglet à sélectionner
 * ni message à afficher.
 */
export async function handOverConflictsToAgent(
    handOver: HandOver,
    deps: HandOverDeps,
): Promise<ComposeNotice | null> {
    const prompt = await deps.git.conflictPrompt(handOver.worktreeRoot);
    return writePromptInTab(prompt, handOver.tabId, deps);
}

/**
 * Les deux temps qui comptent, **écrits une seule fois** : sélectionner, puis composer.
 *
 * Extraits pour l'onglet de merge (#30), qui passe « le reste » à l'agent avec un prompt
 * venu d'ailleurs — `merge_rest_prompt`, composé par le même `compose_conflict_prompt` côté
 * Rust, sur les seuls chemins qu'il n'a pas résolus. Le **chemin d'écriture** reste unique :
 * un second appel à `pty_compose` posé ailleurs pourrait oublier la sélection préalable, et
 * c'est précisément la condition « visible » d'ADR-0015 qui tomberait.
 *
 * `null` quand il n'y a rien à écrire : ni onglet à sélectionner, ni message à afficher.
 */
export async function writePromptInTab(
    prompt: string | null,
    tabId: TabId,
    deps: Pick<HandOverDeps, "pty" | "selectTab">,
): Promise<ComposeNotice | null> {
    if (prompt === null || prompt.length === 0) return null;

    // Avant l'écriture, et sans condition : l'utilisateur doit **voir** le terminal où le
    // texte va se poser, y compris quand la composition finit par être refusée — sinon le
    // refus parlerait d'un prompt qu'il ne regarde pas.
    deps.selectTab(tabId);

    const outcome = await deps.pty.compose(tabId, prompt);
    return NOTICES[outcome];
}

/**
 * L'onglet où poser un prompt de conflit : un **shell du même worktree** qui porte un agent
 * reconnu ([ADR-0006](../../../docs/adr/0006-decouverte-automatique-des-agents.md)).
 *
 * Les trois conditions se lisent d'un bloc, et chacune a un coût si on l'oublie :
 *
 * - **shell** — une surface d'outil n'a pas de PTY (ADR-0003) ; y composer n'a aucun sens,
 *   et rien côté backend n'existe pour le faire ;
 * - **agent reconnu** — le backend refuse de composer dans un onglet qui n'en porte pas
 *   (`no-agent`). Viser un `zsh` à son invite ferait donc sélectionner un terminal sous les
 *   yeux de l'utilisateur pour lui annoncer un refus ;
 * - **même worktree** — le prompt parle des conflits de *ce* worktree. L'écrire ailleurs
 *   donnerait à un agent des chemins qui n'existent pas chez lui.
 *
 * Le premier dans **l'ordre du backend**, qui est celui que `⌘1..9` numérote : à deux agents
 * dans le même worktree, celui qu'on désigne est celui qu'on voit en premier.
 *
 * `null` quand il n'y en a aucun, et Ash n'en ouvre **pas** un pour l'occasion : ouvrir un
 * onglet est un geste de l'utilisateur, et l'écran le lui dit
 * ([ADR-0010](../../../docs/adr/0010-la-sidebar-informe-l-ecran-agit.md)).
 */
export function agentTabIn(tabs: readonly Tab[], worktreeRoot: string): TabId | null {
    const found = tabs.find(
        (tab) => isShell(tab) && tab.agent !== null && tab.location?.worktreeRoot === worktreeRoot,
    );
    return found?.tabId ?? null;
}
