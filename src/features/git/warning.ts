/**
 * L'avertissement qui **nomme** l'agent — ce que ce popup a et qu'aucun client git n'a.
 *
 * Spec §7.1 : « un avertissement nommant **l'agent qui travaille** dans ce worktree, parce
 * qu'un checkout déplacerait des fichiers sous ses pieds ». Le mot qui porte tout est
 * *nommant* : « un agent tourne » ne dit pas s'il faut s'arrêter, `claude` le dit.
 *
 * **Ce module ne décide pas qui est en danger.** La règle vit en Rust
 * (`features::git::at_risk` : `working` et `waiting`, jamais `idle`, `done` ni `error`), et
 * la liste arrive déjà filtrée dans `BranchOverview.agentsAtRisk`
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ce qui se décide ici est
 * la **phrase** : combien d'agents, comment les énumérer, et lesquels sont déjà arrêtés.
 *
 * La pause dont parle l'avertissement est celle d'
 * [ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md) — `SIGSTOP` sur le
 * groupe en avant-plan, et rien d'autre. Aucune touche n'est écrite dans le PTY, et rien de
 * ce que l'outil affiche n'est interprété.
 */

import { AGENT_STATES, presentAgentState } from "@/shared/agent-state";
import type { BusyAgent } from "@/shared/ipc";

/** Ce que la confirmation propose de faire de chaque agent. */
export interface PauseOffer {
    readonly agent: BusyAgent;
    /** `Pause claude` — ou `Resume claude` quand il est déjà arrêté. */
    readonly label: string;
    /** Un agent arrêté n'a plus rien à arrêter : le geste offert est l'inverse. */
    readonly resumes: boolean;
}

/**
 * La phrase de l'avertissement, ou `null` quand il n'y a personne à déranger.
 *
 * Trois choses y sont, et chacune parce que son absence rendrait la phrase inutile :
 *
 * - **les noms**, énumérés, jamais comptés ;
 * - **le worktree**, parce que la popup peut parler d'une branche qui vit ailleurs et que
 *   l'utilisateur doit savoir de quel arbre on parle ;
 * - **la conséquence**, parce qu'un avertissement qui n'explique pas ce qu'il craint se
 *   lit comme un obstacle et se clique sans être lu.
 *
 * Un agent déjà en pause reste nommé, et son état est dit : le faire disparaître laisserait
 * croire qu'il n'y a plus personne dans ce worktree, alors qu'il reprendra dès qu'on le
 * relancera.
 */
export function warnAbout(
    agents: readonly BusyAgent[],
    worktreeName: string,
): string | null {
    if (agents.length === 0) return null;

    // Un groupe par état, dans l'ordre de la planche `1e` — donc `working` avant `waiting`,
    // le même ordre que la sidebar. Le mot vient de `shared/agent-state`, et pas d'ici : il
    // n'y a qu'une présentation des cinq états dans tout le dépôt, et une phrase qui dirait
    // « claude is working » pendant que la sidebar montre `waiting` serait deux vues du même
    // fait qui se contredisent à l'écran, au moment précis où l'utilisateur décide.
    const clauses: string[] = [];
    for (const state of AGENT_STATES) {
        const named = agents.filter((agent) => !agent.paused && agent.state === state);
        if (named.length > 0) {
            clauses.push(clause(named, presentAgentState(state).label));
        }
    }

    // `paused` n'est pas un sixième état, et n'en deviendra pas un : c'est un fait du groupe
    // de processus (`TabInfo.paused`), qui se superpose à l'état sans le remplacer. Il est
    // dit en dernier parce que c'est celui qui ne réclame rien de l'utilisateur.
    const stopped = agents.filter((agent) => agent.paused);
    if (stopped.length > 0) {
        clauses.push(clause(stopped, "paused"));
    }

    // La conséquence n'est écrite que s'il reste quelqu'un pour la subir : tout le monde
    // étant déjà arrêté, la phrase serait un avertissement contre un danger écarté.
    const consequence =
        stopped.length === agents.length ? " — nothing is writing" : " — this would move files under it";
    return `${clauses.join(", and ")} in ${worktreeName}${consequence}`;
}

/**
 * Le geste offert pour chaque agent, dans l'ordre de la liste.
 *
 * Un agent arrêté se voit proposer **de reprendre**, et pas une pause éteinte : sans ce
 * chemin de retour, un `SIGSTOP` serait un piège — l'agent n'émet plus aucun hook, donc plus
 * aucun état, et rien d'autre qu'Ash ne sait qu'il attend un signal.
 */
export function pauseOffers(agents: readonly BusyAgent[]): readonly PauseOffer[] {
    return agents.map((agent) => ({
        agent,
        label: `${agent.paused ? "Resume" : "Pause"} ${agent.name}`,
        resumes: agent.paused,
    }));
}

/** `claude`, `claude and codex`, `claude, codex and aider`. */
function enumerate(names: readonly string[]): string {
    if (names.length <= 1) return names[0] ?? "";
    return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1] ?? ""}`;
}

/** `claude is working`, `claude and codex are waiting`. */
function clause(agents: readonly BusyAgent[], said: string): string {
    const names = enumerate(agents.map((agent) => agent.name));
    return `${names} ${agents.length === 1 ? "is" : "are"} ${said}`;
}
