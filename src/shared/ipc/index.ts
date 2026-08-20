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
 * Un sous-agent qui tourne **dans** un onglet (spec §6.5).
 *
 * Il n'a pas de terminal à lui — c'est le même processus, dans le même onglet
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)) — donc sa ligne n'est pas
 * cliquable, et rien ne le sélectionne.
 *
 * `state` ne vaut jamais que `working` ou `done` : un sous-agent ne peut pas interroger
 * l'utilisateur, donc il n'est **jamais `waiting`** (ADR-0007, amendement du 2026-08-13). Il
 * n'a pas non plus de `error` : son échec n'a aucune source, faute de processus à surveiller,
 * et Ash n'en invente pas.
 *
 * `agentId` distingue deux frères **dans cet onglet**, et rien de plus : il n'est ni stable
 * entre deux sessions, ni une clé de persistance.
 */
export interface Subagent {
    agentId: string;
    /** Le type que l'outil donne à l'enfant — `code-reviewer`, `Explore`. `null` s'il se tait. */
    agentType: string | null;
    state: AgentState;
    /** Quand l'enfant est entré dans cet état — une date absolue, comme `stateSince`. */
    since: number;
}

/**
 * Ce que la configuration d'un outil reconnu porte, du point de vue d'Ash.
 *
 * Ce n'est **pas** un état d'agent : un outil non instrumenté montre `idle` et `working`
 * comme les autres. Ce que le mot dit, c'est *pourquoi* il ne montrera jamais `waiting`
 * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)) — sans quoi son absence se lirait
 * comme une panne.
 *
 * `unsupported` n'est pas `missing` : le premier n'a **aucun geste** — aucun adaptateur de
 * cette version ne sait instrumenter cet outil —, le second mène au flux d'installation des
 * hooks, qui existe déjà dans la fenêtre de réglages.
 */
export type Instrumented = "installed" | "missing" | "unsupported";

/**
 * L'outil reconnu dans l'avant-plan d'un onglet (ADR-0006).
 *
 * Reconnaître est de la **lecture** : rien n'a été écrit, aucune autorisation macOS n'a été
 * demandée. Le backend a comparé ce que la sonde a vu — le chemin de l'exécutable, son nom,
 * son `argv[0]` — à la table embarquée et aux entrées déclarées, et la sidebar rend le
 * résultat ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface RecognizedAgent {
    /** Le nom de l'outil — `claude`, et non le `2.1.234` de son binaire. */
    command: string;
    /** L'adaptateur qui le traduit — `claude-code`, `generic`. */
    adapter: string;
    instrumented: Instrumented;
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
    /**
     * Le programme qui tient l'avant-plan de l'onglet — `zsh`, `claude`, `bun`.
     *
     * C'est le nom de l'**outil** quand c'en est un : le binaire d'un Claude Code posé par
     * son installateur officiel s'appelle `2.1.234`, et l'onglet dit `claude` (ADR-0006).
     */
    process: string;
    /**
     * L'outil reconnu dans l'avant-plan, ou `null` — un shell à son invite, un `vim`.
     *
     * Il ne change pas d'une passe de sonde à l'autre : la fiche d'onglet reste donc stable,
     * et `ash://tab-changed` ne repart pas pour lui.
     */
    agent: RecognizedAgent | null;
    state: AgentState;
    /**
     * Quand l'onglet est **entré** dans cet état — un `Date.now()`, en millisecondes.
     *
     * Une date, jamais une durée : le backend l'envoie une seule fois, en absolu, et la
     * fiche d'onglet reste donc identique tant que l'état ne change pas. Une durée
     * transportée ferait partir `ash://tab-changed` chaque seconde pour chaque onglet
     * actif, c'est-à-dire un rendu complet de la sidebar par seconde pour animer un
     * compteur.
     *
     * Le compteur, lui, se calcule ici, à l'affichage — voir `elapsedSince` dans
     * `features/terminal/status-line.ts`.
     */
    stateSince: number;
    /**
     * Les sous-agents en cours sous cet onglet, dans leur ordre d'apparition.
     *
     * Vide dans le cas courant — et vide pour toujours chez un outil qui n'expose pas ses
     * sous-tâches, sans que rien ne suggère qu'il en manque.
     */
    subagents: Subagent[];
    location: TabLocation | null;
    /**
     * Le groupe en avant-plan de cet onglet est **arrêté** — `SIGSTOP`
     * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
     *
     * Ce n'est pas un sixième état : un état vient d'un hook (ADR-0007), et un processus
     * arrêté n'en émet aucun. Sans ce champ, un agent mis en pause paraîtrait `working` pour
     * toujours, et personne ne saurait qu'il attend un `SIGCONT`.
     */
    paused: boolean;
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

/**
 * Un worktree **épinglé** : une ligne de la colonne qui existe sans qu'aucun onglet ne
 * l'habite (spec §5.2).
 *
 * C'est **un [`TabLocation`]**, et pas une seconde forme qui lui ressemble : une ligne de
 * worktree se range de la même façon qu'elle vienne d'un onglet ou d'une épingle, donc
 * `tree.ts` n'a qu'une règle de rangement, pas deux qui pourraient diverger — le jour où
 * elles divergeraient, un worktree épinglé **et** ouvert aurait deux lignes. Côté Rust ce
 * sont bien deux `struct` dans deux features qui ne se connaissent pas ; c'est ici, une
 * seule fois, qu'on écrit la forme, et `mirror.ts` la confronte aux deux — exactement comme
 * [`RepoRef`].
 *
 * Ce qui traverse est **relu à chaque fois**, jamais recopié du fichier : une épingle dont le
 * dossier a disparu n'arrive pas ici du tout, et la ligne s'efface sans que l'épingle soit
 * perdue.
 */
export type PinnedWorktree = TabLocation;

/**
 * Ce que la colonne garde d'une session à l'autre — et **rien d'autre** (spec §3.1, §9.2).
 *
 * Deux faits, qui vivent dans `~/.ash/state.json` : les worktrees épinglés et les lignes
 * repliées. Aucune session, aucun onglet, aucun worktree courant, aucun état d'agent ne
 * survit à la fermeture ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * `collapsed` mêle deux familles de clés qui ne peuvent pas se confondre : la racine d'un
 * worktree — un chemin absolu — et la clé d'un groupe de dépôt, préfixée (`repo:`, `flat:`).
 * Le repli de la **colonne** (`⌘B`) n'y est pas : il ne se replie pas par ligne, et rien ne
 * le fait survivre.
 */
export interface SidebarRows {
    pinned: PinnedWorktree[];
    collapsed: string[];
}

/**
 * La popup de branches (spec §7.1) — miroir de `src-tauri/src/features/git/branches.rs` et
 * de `branch_actions.rs`.
 *
 * Rien ici n'est calculé côté TypeScript : le groupement, l'ordre, le worktree qui détient
 * chaque branche et les agents en danger viennent tous du backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ce que la webview fait de
 * ces formes — filtrer, sélectionner, écrire une phrase — est dans `features/git/`.
 */

/** De quel côté de la frontière vit une branche. */
export type BranchKind = "local" | "remote";

/** Le worktree qui détient une branche, quand ce n'est pas celui d'où l'on regarde. */
export interface BranchWorktree {
    root: string;
    /** Le dernier segment du chemin — `ash-sidebar`. */
    name: string;
}

/** Une branche, telle que la popup la montre. */
export interface Branch {
    /** `feat/popup` pour une locale, `origin/feat/popup` pour une distante. */
    name: string;
    kind: BranchKind;
    /** L'objet court de la pointe — `a1b2c3d`. */
    tip: string;
    /** La date du commit de pointe, en **secondes** Unix. Le backend ne met rien en forme. */
    committedAt: number;
    /**
     * `null` sur une branche libre.
     *
     * C'est la colonne de droite de la spec §7.1, et le seul endroit d'où elle vient : la
     * webview ne sait pas quels worktrees existent.
     */
    worktree: BranchWorktree | null;
}

/** Les quatre groupes de la spec §7.1, dans l'ordre où ils s'affichent. */
export type BranchGroup = "current" | "recent" | "local" | "remote";

export interface BranchSection {
    group: BranchGroup;
    branches: Branch[];
}

/** Un agent qui écrit dans un worktree — **nommé**, pas compté. */
export interface BusyAgent {
    /** L'onglet qui le porte : c'est par lui que la pause le retrouve. */
    tabId: string;
    /** Le nom de l'outil, tel que la sidebar l'affiche — `claude`. */
    name: string;
    state: AgentState;
    /** Son groupe en avant-plan est déjà arrêté. */
    paused: boolean;
}

/** Tout ce que la popup montre, en une seule réponse — donc vrai au même instant. */
export interface BranchOverview {
    worktreeRoot: string;
    /** `null` quand ce worktree ne détient aucune branche. */
    current: string | null;
    /** Les groupes **non vides**, dans l'ordre. */
    sections: BranchSection[];
    /** Les agents qu'un geste sur l'arbre dérangerait. Vide dans le cas courant. */
    agentsAtRisk: BusyAgent[];
}

/** Les trois verbes que `⌘⏎` propose. Une union fermée, comme l'énumération Rust. */
export type BranchAction = "checkout" | "rebase" | "merge";

/** Ce qu'une action propose — son libellé à deux côtés, et sa raison de refus. */
export interface ActionOffer {
    action: BranchAction;
    /**
     * Toujours présent, refus compris : un bouton éteint reste visible **avec sa raison**.
     *
     * Composé côté Rust, et pas ici : le message d'échec est fabriqué du côté qui reçoit la
     * sortie de git, et deux compositions séparées nommeraient deux choses pour un geste.
     */
    label: string;
    /** `null` quand l'action est permise. */
    refused: string | null;
    /** Elle touche l'arbre de travail, donc elle dérange un agent qui y écrit. */
    touchesTree: boolean;
}

/** Ce qu'une action a fait, ou n'a pas fait. */
export interface ActionOutcome {
    /** Les deux côtés, encore — y compris quand `success` est faux (spec §7.1). */
    label: string;
    success: boolean;
    /** Ce que git a dit, tel quel. */
    output: string;
}
