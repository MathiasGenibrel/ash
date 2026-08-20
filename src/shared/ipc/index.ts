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

import type { Assert, Mirrors, Refuses } from "./mirroring";

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
 * La place qu'une conversation occupe dans sa fenêtre de contexte.
 *
 * **Deux nombres, et pas un pourcentage** : le calcul est un fait d'affichage, et le garder
 * ici laisse l'écran libre de dire `128k / 200k` plutôt qu'un `73 %`.
 *
 * `windowTokens` est une **supposition**, et c'est une limite connue : le transcript nomme le
 * modèle sans dire si la session est de 200 k ou de 1 M, et aucun hook ne le dit non plus.
 * `usedTokens`, lui, est exact.
 */
export interface SessionUsage {
    usedTokens: number;
    windowTokens: number;
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
    /**
     * La place que la conversation de cet onglet occupe dans sa fenêtre de contexte, ou
     * `null`.
     *
     * `null` couvre trois cas que rien ne doit distinguer à l'écran : l'outil ne tient pas de
     * transcript, aucun hook n'en a encore nommé un, ou la mesure n'a rien donné. Dans les
     * trois, **on n'affiche rien** — pas de jauge à zéro, pas de `ctx —`. Un outil muet ne
     * doit rien coûter à l'affichage, et rien ne doit suggérer qu'il manque une mesure.
     *
     * Elle voyage comme `stateSince` : une donnée absolue, envoyée au changement. Le backend
     * ne relit le transcript qu'à l'arrivée d'un hook — jamais à une passe de sonde — donc
     * elle ne fait pas repartir `ash://tab-changed`.
     *
     * Ce n'est **pas** un état : un contexte plein ne rend pas un onglet `error`.
     */
    usage: SessionUsage | null;
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
    /**
     * Les chemins qui attendent une décision, tels que git les écrit — un chemin exotique
     * y arrive entre guillemets et échappé, comme git l'affiche partout ailleurs.
     *
     * La liste est **bornée** par le backend ; `tree.conflicted` ne l'est pas. Afficher
     * `conflicts.length` là où l'utilisateur attend un nombre de fichiers dirait « 100 »
     * pour un dépôt qui en a trois mille.
     */
    conflicts: string[];
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
 * Le commit sur lequel un rebase ou un `am` s'est arrêté (spec §7.4).
 *
 * `subject` est `null` pour le moteur `apply` et pour `git am` : ils n'écrivent nulle part
 * le message du commit en cours, et Ash n'en invente pas.
 */
export interface StoppedCommit {
    /** L'identifiant abrégé — celui qu'un `git show` accepte tel quel. */
    commit: string;
    subject: string | null;
}

/**
 * Un rebase ou un merge **arrêté**, tel que le backend le lit (spec §7.4).
 *
 * Ash **ne touche à rien** : tout ici est de la lecture. `escapes` en est la preuve la plus
 * visible — ce sont les commandes de secours (`git rebase --abort`, `--skip`) rendues comme
 * du **texte à montrer**, jamais comme des actions qu'Ash exécuterait
 * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
 *
 * `conflicts` peut être **vide** sans que ce soit une anomalie : un rebase interactif
 * s'arrête aussi sur un `edit` ou un `break`, sans le moindre conflit.
 *
 * `testCommand` est `null` quand rien dans le worktree ne la nomme. C'est une réponse, pas
 * un manque : un prompt qui nomme la mauvaise commande coûte plus cher qu'un prompt muet.
 */
export interface StoppedOperation {
    operation: GitOperation;
    conflicts: string[];
    /** Combien il y en a en tout, ou `null` si `git` n'a pas répondu. */
    conflictedTotal: number | null;
    stoppedAt: StoppedCommit | null;
    /** `ORIG_HEAD` abrégé — le filet de secours, affiché et jamais utilisé. */
    origHead: string | null;
    testCommand: string | null;
    escapes: string[];
}

/**
 * Ce qu'il est advenu d'une demande de composition (`pty_compose`).
 *
 * Ce n'est pas un détail d'implémentation : c'est ce que l'écran doit dire à l'utilisateur.
 * `written` s'accompagne de « ash typed this for you — not sent yet », `queued` de
 * « queued behind the current turn » — et dans les deux cas **rien n'a été envoyé**
 * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
 *
 * `prompt-not-empty` : l'utilisateur a déjà commencé à taper, et Ash n'insère pas au milieu
 * d'une frappe. `no-agent` : aucun outil reconnu ne tient l'avant-plan de l'onglet, et le
 * texte y serait une ligne de commande plutôt qu'un prompt.
 */
export type ComposeOutcome = "written" | "queued" | "prompt-not-empty" | "no-agent";

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
 * Où vit la fiche de branche
 * ([ADR-0013](../../../docs/adr/0013-fiche-de-branche-dans-le-depot.md)).
 *
 * `repo` est le cas nominal — `.ash/worktree.md`, versionné, qui voyage avec la branche et
 * qu'un agent qui la reprend peut lire. `local` est le repli quand l'équipe ne veut pas du
 * fichier dans le dépôt : la fiche vit alors dans `~/.ash/worktrees/`, et **perd son unique
 * avantage**. Ash ne force ni l'un ni l'autre, et n'écrit jamais dans un `.gitignore`.
 */
export type CardMode = "repo" | "local";

/**
 * Ce que le bloc `<!-- ash:log -->` porte, et ce qu'Ash a le droit d'en faire.
 *
 * Cinq des huit valeurs sont des **refus**, et c'est le sujet de la fiche : Ash n'écrit que
 * ce qui lui appartient, et sait le reconnaître. `edited-by-hand` et `conflicted` sont les
 * deux que la spec §10 et ADR-0013 nomment ; `unterminated` et `duplicated` sont ce qu'une
 * fusion laisse derrière elle. Dans les cinq cas, le fichier n'est pas touché.
 */
export type CardLogState =
    | "current"
    | "stale"
    | "no-card"
    | "no-block"
    | "edited-by-hand"
    | "conflicted"
    | "unterminated"
    | "duplicated";

/**
 * L'état de la zone d'Ash dans la fiche, **calculé une seule fois** côté Rust.
 *
 * `writable` n'est pas déduit de `state` par l'écran, et c'est délibéré : agir et afficher
 * doivent lire la même décision, sans quoi un bouton allumé pourrait proposer une écriture
 * que le backend refusera — c'est la leçon de `hooks::presence`.
 */
export interface CardLog {
    state: CardLogState;
    /** La table telle qu'elle irait dans le bloc. */
    table: string;
    /** Le fichier tel qu'il est face au fichier tel qu'Ash le laisserait (spec §10). */
    diff: string;
    /** Ce qui se passe, ou ce qui ne se passera pas, en une phrase. */
    note: string;
    writable: boolean;
}

/**
 * La fiche de branche d'un worktree (spec §7.5).
 *
 * `source` est du markdown **brut**, et c'est le seul endroit du contrat où du texte non
 * interprété traverse : ADR-0013 exige que le rendu n'invente aucune syntaxe, donc l'écran
 * met en forme ce que n'importe quel éditeur afficherait déjà. Rien ici n'est du HTML, et
 * rien n'est posé par `innerHTML`.
 */
export interface BranchCard {
    /** La racine du worktree — la même clé que celle des onglets (`TabLocation`). */
    worktreeRoot: string;
    path: string;
    /** Où la fiche irait dans l'autre mode — ce que l'interrupteur promet. */
    otherPath: string;
    mode: CardMode;
    /** `.ash` est gitignoré : la fiche ne partira pas avec la branche. */
    ignoredByTheRepo: boolean;
    exists: boolean;
    source: string;
    log: CardLog;
}

/**
 * Un agent qui tourne **en ce moment** dans un worktree — la colonne `agents now` du
 * tableau (spec §7.3).
 *
 * C'est l'une des deux colonnes que `git worktree list` ne donne pas : elle vient des
 * onglets, dont le backend connaît le `cwd` résolu et l'outil en avant-plan (ADR-0005,
 * ADR-0006). Un onglet où tourne un shell ou un `vim` n'y est pas.
 */
export interface WorktreeAgent {
    /** De quoi y aller d'un clic — et rien de plus : rien ne sélectionne sans un geste (ADR-0010). */
    tabId: TabId;
    /** Le nom de l'outil — `claude`, `codex`. */
    command: string;
    state: AgentState;
    /** Quand il est **entré** dans cet état. Une date absolue, comme `TabInfo.stateSince`. */
    since: number;
}

/**
 * D'où vient ce que `last worked by` affirme.
 *
 * Les deux ne promettent pas la même chose, et l'écran le dit : `tab` est une observation
 * d'à l'instant — l'agent est là, ou vient d'y être —, `commit` une observation qui a
 * survécu à la fermeture de son onglet, parce que le journal d'attribution l'a gardée
 * ([ADR-0014](../../../docs/adr/0014-attribution-locale-des-commits.md)).
 */
export type WorkSource = "tab" | "commit";

/**
 * Qui a travaillé dans ce worktree en dernier — la seconde colonne que `git worktree list`
 * ne donne pas.
 *
 * `null` veut dire **« Ash ne sait pas »**, jamais « personne » : un agent qui a travaillé
 * une nuit sans rien valider, et dont l'onglet est fermé, n'a laissé aucune trace qu'Ash ait
 * le droit d'invoquer. La colonne se tait alors, et c'est la lettre d'ADR-0014.
 */
export interface LastWork {
    agent: string;
    at: number;
    source: WorkSource;
}

/** Le dépôt sous lequel une ligne du tableau se range — la même clé que [`RepoRef`]. */
export interface WorktreeRepo {
    id: string;
    name: string;
}

/**
 * Une ligne du tableau des worktrees (spec §7.3).
 *
 * Rien ici n'est calculé par la fenêtre : les deux colonnes du milieu croisent les onglets,
 * le journal et l'état git, et c'est le backend qui les compose
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface WorktreeRow {
    /** La même clé que `TabLocation.worktreeRoot` et que l'event `ash://git-metadata`. */
    worktreeRoot: string;
    worktreeName: string;
    repo: WorktreeRepo | null;
    /** `null` quand rien ne s'est laissé lire — un `.git` cassé, un dépôt disparu. */
    metadata: WorktreeMetadata | null;
    agentsNow: WorktreeAgent[];
    /**
     * `done · waiting for your review` — l'état que la spec §7.3 nomme le plus utile du
     * tableau.
     *
     * Il n'y a **pas** de seconde notion de « vu » : un `done` ne survit à sa lecture que
     * trente secondes, et elles ne partent qu'au premier focus de la fenêtre (spec §6.4).
     * Un onglet encore `done` est donc un onglet que personne n'a regardé.
     */
    awaitingReview: boolean;
    lastWorkedBy: LastWork | null;
    /**
     * Sans agent depuis plus de trois jours **et** des fichiers modifiés (spec §5.4).
     *
     * **Ash le signale, il ne le supprime jamais.** Le mot ne sort que sur une observation
     * datée : un worktree qu'Ash n'a jamais vu habité n'est pas signalé pour autant.
     */
    stale: boolean;
    /** Le worktree principal du dépôt : celui que `git worktree remove` refuse. */
    main: boolean;
}

/**
 * Ce qu'une suppression de worktree emporterait — énoncé **avant** qu'elle n'ait lieu
 * (spec §5.4).
 *
 * Ash ne supprime rien : `command` est du **texte à montrer**, comme les `escapes` d'un
 * rebase arrêté ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
 * `carries` vide veut dire qu'il n'y a rien à perdre — et il porte au contraire une ligne
 * qui l'avoue quand `git status` n'a pas répondu.
 */
export interface WorktreeRemoval {
    worktreeRoot: string;
    worktreeName: string;
    carries: string[];
    refused: string | null;
    command: string;
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

/* ------------------------------------------------------------------------------------- *
 * L'onglet de merge (spec §7.4, issue #30) — miroir de `src-tauri/src/features/merge/`
 * et de `src-tauri/src/tabs.rs`.
 * ------------------------------------------------------------------------------------- */

/**
 * Un côté d'un conflit — **nommé par sa branche**, jamais `ours` ni `theirs`.
 *
 * C'est le critère du ticket, et ce n'est pas du vocabulaire : le `ours` de git désigne la
 * branche courante pendant un merge, et la branche **cible** pendant un rebase. Un écran qui
 * garderait les deux mots mettrait le travail de l'utilisateur du mauvais côté une fois sur
 * deux, au moment précis où il tranche.
 *
 * `role` est ce que ce côté *est* dans cette opération — « the branch you are rebasing
 * onto », « the branch you are on ». Le backend le compose, la webview l'affiche : c'est le
 * backend qui sait laquelle des deux lectures s'applique.
 */
export interface SideLabel {
    name: string;
    role: string;
}

/** Les deux colonnes extérieures des trois panneaux. */
export interface MergeSides {
    /** Ce que git appelle `ours`. Le mot s'arrête au backend. */
    left: SideLabel;
    /** Ce que git appelle `theirs`. */
    right: SideLabel;
}

/**
 * Un conflit, tel que git l'a écrit dans le fichier du worktree.
 *
 * `base` n'est là que si le dépôt configure `merge.conflictStyle = diff3`. Son absence
 * n'est pas un manque : les trois panneaux de la spec sont `gauche` / résultat / `droite`.
 */
export interface MergeHunk {
    /** Le rang dans le fichier, à partir de zéro : c'est par lui qu'une résolution le désigne. */
    index: number;
    ours: string;
    base: string | null;
    theirs: string;
}

/**
 * Un fichier en conflit.
 *
 * `resolved` se déduit du **fichier**, pas de `git status` : l'état de l'index est
 * rafraîchi par une surveillance limitée à une lecture toutes les cinq secondes, et un
 * compte en retard ferait clignoter `continue` à contretemps.
 *
 * `unreadable` est un refus, pas une panne : un chemin que git a dû échapper
 * (`"src/\303\251.rs"`) n'est jamais ouvert ni réécrit par Ash. Il reste listé et compté.
 */
export interface ConflictFile {
    path: string;
    hunks: MergeHunk[];
    resolved: boolean;
    unreadable: boolean;
}

/** Ce que l'onglet montre quand l'opération est toujours arrêtée. */
export interface MergeStopped {
    operation: GitOperation;
    sides: MergeSides;
    files: ConflictFile[];
    /** Les conflits que git compte au-delà de la liste, bornée à cent. Zéro d'ordinaire. */
    hidden: number;
    /** Le compte à droite de `continue` (spec §7.4). */
    unresolved: number;
    origHead: string | null;
    /** `abort` et `skip` — du **texte**, qu'Ash n'exécute pas (ADR-0015). */
    escapes: string[];
    /** Le libellé du bouton : `git rebase --continue`, `git merge --continue`. */
    continueCommand: string;
    /** Faux tant qu'il reste un conflit : le bouton reste **visible**, éteint. */
    canContinue: boolean;
}

/**
 * L'onglet de merge, relu de bout en bout.
 *
 * `stopped` à `null` veut dire que l'opération s'est terminée ou a été abandonnée
 * **ailleurs** — dans un terminal, par un agent. L'onglet reste ouvert et le dit ; rien ne
 * se ferme sans un geste de l'utilisateur
 * ([ADR-0010](../../../docs/adr/0010-la-sidebar-informe-l-ecran-agit.md)).
 */
export interface MergeView {
    tabId: TabId;
    worktreeRoot: string;
    title: string;
    stopped: MergeStopped | null;
}

/** Ce qu'une invocation git de l'onglet a fait, ou n'a pas fait. */
export interface MergeOutcome {
    /** La phrase qui nomme ce qui a été tenté. Présente même quand `success` est faux. */
    label: string;
    success: boolean;
    /** Ce que git a dit, tel quel. */
    output: string;
}

/**
 * Un onglet de shell — ce qu'un onglet a toujours été jusqu'à #30.
 *
 * C'est `TabInfo` **plus son étiquette**. Le champ `kind` n'existe que parce qu'un second
 * genre existe désormais : sans lui, la seule façon de distinguer les deux serait de tester
 * la présence d'un champ, c'est-à-dire de deviner.
 */
export type ShellTab = TabInfo & { kind: "shell" };

/**
 * Un onglet de merge : **pas de PTY du tout**
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md), reformulation du 2026-08-10).
 *
 * Il n'a ni `cwd`, ni processus en avant-plan, ni état d'agent, ni `stateSince`, ni pause —
 * et ces champs ne sont **pas** dans sa forme. Les y mettre à des valeurs neutres ferait
 * apparaître un `idle · 12m` sous un onglet où aucun processus ne tourne, et la ligne de
 * statut afficherait la durée d'un état qui n'existe pas.
 */
export interface MergeTab {
    kind: "merge";
    tabId: TabId;
    /** La racine du worktree dont on résout le conflit — la clé de rangement de la sidebar. */
    worktreeRoot: string;
    /** Ce que la ligne affiche — `rebase feat onto main`, composé par le backend. */
    title: string;
    /** L'opération est toujours arrêtée dans ce worktree. */
    live: boolean;
    location: TabLocation | null;
}

/**
 * Un onglet, quel que soit son genre — la somme du modèle §3.
 *
 * L'ordre de la liste est celui du backend, et lui seul : les shells d'abord, dans leur
 * ordre, puis les surfaces de merge. C'est celui que `⌘1..9` numérote et que `⌃⇥` parcourt
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export type Tab = ShellTab | MergeTab;

/** L'onglet est-il un terminal ? La seule question à poser avant de lire un champ de shell. */
export function isShell(tab: Tab): tab is ShellTab {
    return tab.kind === "shell";
}

/* ------------------------------------------------------------------------------------- *
 * La preuve que « un onglet = un PTY » ne peut plus se supposer.
 * ------------------------------------------------------------------------------------- */

/**
 * Les seuls champs qu'un `Tab` offre **sans** qu'on ait demandé son genre.
 *
 * `keyof` d'une somme est l'intersection des clés de ses variantes : cette liste est donc
 * littéralement ce que `tsc` laisse lire sur un onglet non discriminé. Tout le reste —
 * `cwd`, `process`, `state`, `stateSince`, `paused`, `agent`, `subagents`, `title` —
 * exige de passer par [`isShell`], et une lecture directe ne compile pas.
 *
 * #30 a corrigé **dix** endroits qui supposaient « un onglet est un PTY ». Cette assertion
 * est ce qui rend le onzième impossible plutôt qu'improbable : elle rougit aussi bien le
 * jour où un troisième genre d'onglet apparaît que le jour où quelqu'un ajoute un `cwd`
 * neutre à [`MergeTab`] pour faire taire une erreur de compilation — c'est-à-dire au moment
 * précis où le trou se rouvrirait.
 */
export type ReadableOnAnyTab = Assert<Mirrors<keyof Tab, "kind" | "tabId" | "location">>;

/**
 * Un `Tab` n'est **pas** un `TabInfo`.
 *
 * C'est la moitié structurelle du même filet : les dizaines de fonctions déjà écrites qui
 * prennent un `TabInfo` refusent la somme, donc aucune d'elles ne peut se voir passer une
 * surface d'outil par distraction. Ce qui se compile encore, ce sont celles qui prennent
 * explicitement un [`ShellTab`] — et celles-là ont un appelant qui a discriminé.
 */
export type ATabIsNotATabInfo = Assert<Refuses<Tab, TabInfo>>;

/* ------------------------------------------------------------------------------------- *
 * Les quotas du compte (spec §4.2) — ADR-0016 et ADR-0017.
 * ------------------------------------------------------------------------------------- */

/**
 * Un quota : où il en est, et **quand il repart de zéro**.
 *
 * `resetsAt` est une date absolue, en millisecondes depuis l'époque Unix — la même forme que
 * [`TabInfo.stateSince`], et pour la même raison : le `resets in 2h14` de la maquette est un
 * fait d'affichage, calculé ici. Un décompte transporté ferait repartir
 * `ash://account-usage` chaque seconde pour animer un compteur, alors que la valeur ne bouge
 * qu'au rythme du quota.
 *
 * `null` veut dire que l'hôte n'a pas donné de date — un compte sans fenêtre de limitation,
 * une fenêtre qui n'a pas commencé. Le pourcentage passe quand même : n'avoir qu'une des
 * deux moitiés vaut mieux que n'en avoir aucune.
 *
 * **La durée de la fenêtre n'existe nulle part, ni ici ni côté backend.** Les cinq heures de
 * la maquette ne sont écrites dans aucun code : tout ce dont l'écran a besoin pour dire
 * `2h14` est `resetsAt`, et une fenêtre qui passerait à quatre heures n'aurait rien à
 * corriger.
 */
export interface Quota {
    /** Entre `0` et `100`. L'hôte le rend parfois fractionnaire. */
    percent: number;
    resetsAt: number | null;
}

/**
 * Ce qu'Ash sait de l'usage du **compte** — pas d'un onglet, pas d'un worktree.
 *
 * Les deux quotas sont transverses : ils ne dépendent d'aucune sélection, et changer
 * d'onglet ne les touche pas. C'est pourquoi ils ne voyagent pas dans un [`TabInfo`], mais
 * dans leur propre event.
 *
 * **Les deux champs sont indépendants**, et `null` des deux côtés est ce que « la valeur
 * disparaît » veut dire ([ADR-0016](../../../docs/adr/0016-ash-sort-sur-le-reseau.md),
 * condition 3) : jeton absent, jeton refusé, hôte injoignable, ou appels coupés par
 * l'utilisateur donnent tous la même chose — rien. Ni un zéro, ni un tiret, ni la dernière
 * valeur connue laissée en place. L'écran n'affiche donc **rien** dans ce cas, et ne signale
 * pas d'erreur : il n'a aucun moyen de savoir laquelle des quatre raisons s'applique, et
 * c'est délibéré.
 */
export interface AccountUsage {
    session: Quota | null;
    weekly: Quota | null;
}
