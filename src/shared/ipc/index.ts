/**
 * Le contrat Rust ↔ TypeScript des onglets et de l'état git des worktrees.
 *
 * Il vit dans `shared/` et non dans une feature parce qu'il en sert **deux** — la feature
 * terminal, qui ouvre et affiche les onglets, et la sidebar, qui les range par worktree —
 * et qu'il ne porte la règle d'aucune des deux : ce ne sont que les formes que le backend
 * sérialise.
 *
 * Miroir de `src-tauri/src/features/pty/registry.rs`, de
 * `src-tauri/src/features/pty/locate.rs` et de
 * `src-tauri/src/features/git/metadata.rs`. Rien ici n'est calculé : le frontend **rend**
 * ce que le backend détient ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */

/** Identifiant d'onglet : l'ulid que le backend a posé dans `ASH_TAB_ID`. */
export type TabId = string;

/**
 * Les cinq états d'une ligne d'agent.
 *
 * Seuls `idle` et `working` ont un producteur à ce jalon — la sonde d'ADR-0005 sait si le
 * shell est à son invite. `waiting`, `done` et `error` viendront des hooks
 * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)), qui interdit de les déduire de
 * la sortie du PTY. Le frontend sait déjà les **présenter** ; il n'en invente aucun.
 */
export type AgentState = "idle" | "working" | "waiting" | "done" | "error";

/**
 * Le dépôt commun sous lequel un worktree se range.
 *
 * L'`id` est le dossier git commun : c'est par lui que deux worktrees du même projet se
 * reconnaissent, jamais par le nom — deux dépôts homonymes existent.
 */
export interface RepoRef {
    id: string;
    name: string;
}

/**
 * Où un onglet se situe dans la hiérarchie d'
 * [ADR-0012](../../../docs/adr/0012-worktree-unite-de-travail.md).
 *
 * `repo` à `null` **est** la forme à plat : le backend l'a déjà tranché, la sidebar ne le
 * re-dérive pas.
 */
export interface TabLocation {
    worktreeRoot: string;
    /** Le nom **brut** du dossier — la matière du suffixe `·sidebar`, pas le suffixe. */
    worktreeName: string;
    repo: RepoRef | null;
}

/**
 * Un onglet, tel que le backend le décrit.
 *
 * `cwd` est le répertoire **courant** : la sonde d'ADR-0005 le suit à travers les `cd`, et
 * même pendant qu'un programme tourne. C'est lui que « nouvel onglet dans le worktree
 * courant » (spec §4.4) reprend, et c'est son changement qui fait migrer l'onglet d'un
 * dépôt à l'autre dans la sidebar.
 *
 * `location` à `null` veut dire « le backend n'a pas su situer ce répertoire » — un `.git`
 * cassé, un dépôt disparu. Ce n'est pas la même chose qu'un répertoire hors dépôt, qui a
 * bien une localisation, sans `repo`.
 */
export interface TabInfo {
    tabId: TabId;
    cwd: string;
    /** Le programme qui tient l'avant-plan de l'onglet — `zsh`, `claude`, `bun`. */
    process: string;
    state: AgentState;
    location: TabLocation | null;
}

/**
 * Où pointe le `HEAD` d'un worktree.
 *
 * Deux formes, et pas de troisième : une branche, ou un commit détaché — pendant un
 * rebase, notamment. Le nom est déjà court (`feat/watch`), le commit déjà abrégé : la
 * mise en forme est faite côté backend, qui est le seul à savoir ce qu'il a lu.
 */
export type GitHead = { kind: "branch"; name: string } | { kind: "detached"; commit: string };

/** L'opération git en cours dans un worktree. */
export type GitOperationKind = "rebase" | "am" | "merge";

/** `2/5` : l'étape en cours et le total. */
export interface GitProgress {
    step: number;
    total: number;
}

/**
 * Ce qu'un worktree traverse en ce moment — ce que la ligne `rebasing onto main · 2/5`
 * met en mots.
 *
 * `branch` est la branche que l'opération déplace, `onto` son point d'arrivée : un nom de
 * branche quand un ref le désigne, un identifiant abrégé sinon. `progress` est absente
 * pour un merge, qui n'a pas d'étapes.
 */
export interface GitOperation {
    kind: GitOperationKind;
    branch: string | null;
    onto: string | null;
    progress: GitProgress | null;
}

/**
 * `+3 ~1` : des **nombres de fichiers**, jamais de lignes.
 *
 * `added` compte les fichiers ajoutés à l'index **et** les fichiers non suivis — un agent
 * crée des fichiers avant de les ajouter. Un dossier entièrement nouveau y compte pour
 * une entrée, comme git le rend. Un renommage compte comme une modification.
 *
 * `deleted` et `conflicted` ne sont pas dans la maquette ; ils viennent du même appel, et
 * afficher `+3 ~1` sans savoir qu'un fichier est en conflit serait perdre l'information
 * au moment où elle compte.
 */
export interface GitTreeStatus {
    added: number;
    modified: number;
    deleted: number;
    conflicted: number;
}

/** `↑2 ↓1` : l'avance et le retard sur la branche amont. */
export interface GitUpstream {
    ahead: number;
    behind: number;
}

/**
 * Ce que seul `git status` sait dire d'un worktree.
 *
 * `upstream` à `null` veut dire « cette branche ne suit rien » — une branche locale toute
 * neuve, un `HEAD` détaché. Ce n'est pas `↑0 ↓0`, qui est une synchronisation constatée.
 */
export interface GitStatus {
    tree: GitTreeStatus;
    upstream: GitUpstream | null;
}

/**
 * L'état git d'un worktree.
 *
 * Il est **propre au worktree**, jamais au dépôt : deux worktrees du même projet peuvent
 * avoir un rebase en cours dans l'un et rien dans l'autre
 * ([ADR-0012](../../../docs/adr/0012-worktree-unite-de-travail.md)).
 *
 * `head` et `operation` se lisent dans les fichiers de contrôle du dépôt et sont toujours
 * là. `status` vient d'un appel à `git` : `null` veut dire qu'il n'a pas répondu — absent
 * de la machine, ou dépôt trop gros pour le délai. C'est un cas **nominal**, qui se rend
 * en n'affichant ni `+3 ~1` ni `↑2 ↓1` ; la branche, elle, reste affichée.
 */
export interface WorktreeMetadata {
    head: GitHead;
    operation: GitOperation | null;
    status: GitStatus | null;
}

/**
 * Ce que porte l'event `ash://git-metadata`.
 *
 * `worktreeRoot` est la même clé que celle des onglets (`TabLocation.worktreeRoot`) :
 * c'est par elle que la sidebar rapproche un état git de la ligne qui l'affiche.
 */
export interface WorktreeMetadataChanged {
    worktreeRoot: string;
    metadata: WorktreeMetadata;
}
