import type {
    AccountUsage,
    AgentState,
    ConflictFile,
    GitHead,
    GitOperation,
    GitOperationKind,
    MergeView,
    SideLabel,
    GitStatus,
    Instrumented,
    MergeTab,
    RecognizedAgent,
    PinnedWorktree,
    Quota,
    RepoRef,
    SessionUsage,
    ShellTab,
    Subagent,

    WorktreeMetadata,
} from "./index";

/**
 * Test Data Builder : un onglet tel que le backend le décrirait.
 *
 * Les défauts sont valides et déterministes — un `zsh` à son invite, dans un dépôt sans
 * worktree lié, donc **à plat**. Un scénario ne surcharge que ce qu'il regarde.
 *
 * Il vit à côté du contrat, et non dans une feature, pour la même raison que le contrat
 * lui-même : la sidebar et la feature terminal décrivent toutes les deux des onglets dans
 * leurs tests. Chacune avec sa propre fabrique, un champ ajouté à `TabInfo` se rattrape à
 * quatre endroits — et les quatre finissent par ne plus décrire le même onglet.
 *
 * Ce fichier n'est pas réexporté par `index.ts` : seuls les tests l'importent, et rien du
 * bundle applicatif n'y touche.
 */
export class TabBuilder {
    private tabId = "T1";
    private process = "zsh";
    private state: AgentState = "idle";
    private cwd = "/dev/solo";
    private worktreeRoot = "/dev/solo";
    private worktreeName = "solo";
    private repo: { id: string; name: string } | null = null;
    private located = true;
    private stopped = false;
    /**
     * L'onglet est entré dans son état à l'époque Unix.
     *
     * Un défaut **déterministe**, comme les autres : un `Date.now()` ici ferait dépendre du
     * moment où le test tourne, et un scénario qui parle d'une durée doit dire lui-même
     * quand l'état a commencé.
     */
    private stateSince = 0;
    /** Aucun sous-agent : le cas de l'écrasante majorité des onglets. */
    private subagents: Subagent[] = [];
    /** Aucun outil reconnu : un shell à son invite, ou un programme quelconque (ADR-0006). */
    private agent: RecognizedAgent | null = null;
    /**
     * Aucune mesure de contexte — le défaut de **tout** onglet dont l'outil ne tient pas de
     * transcript, et de tout onglet dont l'agent n'a pas encore parlé.
     */
    private usage: SessionUsage | null = null;

    static create(): TabBuilder {
        return new TabBuilder();
    }

    named(tabId: string): this {
        this.tabId = tabId;
        return this;
    }

    running(process: string, state: AgentState = "working"): this {
        this.process = process;
        this.state = state;
        return this;
    }

    /**
     * Le backend a reconnu un outil dans l'avant-plan de cet onglet.
     *
     * Le nom de l'outil **est** ce que la ligne affiche : c'est le backend qui le pose dans
     * `process`, et un scénario qui parle d'un agent reconnu décrit les deux ensemble.
     */
    runningAgent(command: string, instrumented: Instrumented = "installed", adapter = "claude-code"): this {
        this.process = command;
        this.agent = { command, adapter, instrumented };
        return this;
    }

    inState(state: AgentState): this {
        this.state = state;
        return this;
    }

    /** L'onglet est entré dans son état à cette date — un `Date.now()`, en millisecondes. */
    since(stateSince: number): this {
        this.stateSince = stateSince;
        return this;
    }

    /**
     * Un sous-agent tourne sous cet onglet (spec §6.5).
     *
     * `since` est une **date** et non une durée, comme ce qui traverse réellement : un
     * scénario qui parle d'un compteur doit dire lui-même quand l'enfant a commencé.
     */
    withSubagent(
        agentType: string | null,
        state: AgentState = "working",
        since = 0,
        agentId = `agent-${String(this.subagents.length + 1)}`,
    ): this {
        this.subagents.push({ agentId, agentType, state, since });
        return this;
    }

    /**
     * L'outil de cet onglet dit occuper cette place dans sa fenêtre de contexte.
     *
     * Deux nombres, comme ce qui traverse : le pourcentage se calcule à l'affichage. Un
     * onglet dont l'outil est muet n'appelle simplement pas cette méthode — il n'y a pas de
     * « jauge vide » à décrire.
     */
    consuming(usedTokens: number, windowTokens = 200_000): this {
        this.usage = { usedTokens, windowTokens };
        return this;
    }

    /** Un worktree seul sous son dépôt : la forme **à plat** d'ADR-0012. */
    inFlatWorktree(root: string): this {
        this.cwd = root;
        this.worktreeRoot = root;
        this.worktreeName = basename(root);
        this.repo = null;
        return this;
    }

    /** Un worktree **groupé** sous son dépôt commun. */
    inWorktree(root: string, repoName: string, repoId = `/dev/${repoName}/.git`): this {
        this.cwd = root;
        this.worktreeRoot = root;
        this.worktreeName = basename(root);
        this.repo = { id: repoId, name: repoName };
        return this;
    }

    /** Le shell est descendu dans un sous-dossier : le worktree, lui, ne bouge pas. */
    workingIn(cwd: string): this {
        this.cwd = cwd;
        return this;
    }

    /** L'agent de cet onglet est arrêté — `SIGSTOP` (ADR-0015). */
    paused(): this {
        this.stopped = true;
        return this;
    }

    /** Le backend n'a pas su situer ce répertoire — `.git` cassé, dépôt disparu. */
    unlocated(cwd: string): this {
        this.cwd = cwd;
        this.located = false;
        return this;
    }

    /**
     * L'onglet, **étiqueté shell** (`kind: "shell"`).
     *
     * L'étiquette est dans la fabrique et non ajoutée par chaque test : depuis #30 un
     * onglet est une somme (`Shell | Merge`), et un test qui décrirait un onglet sans son
     * genre décrirait une forme que le backend n'envoie plus.
     */
    build(): ShellTab {
        return {
            kind: "shell",
            tabId: this.tabId,
            cwd: this.cwd,
            process: this.process,
            agent: this.agent,
            state: this.state,
            stateSince: this.stateSince,
            subagents: this.subagents,
            usage: this.usage,
            paused: this.stopped,
            location: this.located
                ? {
                      worktreeRoot: this.worktreeRoot,
                      worktreeName: this.worktreeName,
                      repo: this.repo,
                  }
                : null,
        };
    }
}

/**
 * Test Data Builder : un **onglet de merge** (spec §7.4) — le premier onglet sans PTY.
 *
 * Ses défauts décrivent le décor courant : un rebase de `feat` sur `main`, arrêté, dans un
 * worktree seul sous son dépôt. Il n'a **ni `cwd`, ni état, ni pause** — et c'est ce que
 * ce constructeur rend visible dans les tests qui rangent une liste d'onglets.
 */
export class MergeTabBuilder {
    static create(): MergeTabBuilder {
        return new MergeTabBuilder();
    }

    private tabId = "merge-1";
    private worktreeRoot = "/dev/ash";
    private title = "rebase feat onto main";
    private isLive = true;
    private located = true;
    private repo: RepoRef | null = null;

    id(tabId: string): this {
        this.tabId = tabId;
        return this;
    }

    named(title: string): this {
        this.title = title;
        return this;
    }

    inFlatWorktree(root: string): this {
        this.worktreeRoot = root;
        this.repo = null;
        return this;
    }

    inWorktree(root: string, repoName: string, repoId = `/dev/${repoName}/.git`): this {
        this.worktreeRoot = root;
        this.repo = { id: repoId, name: repoName };
        return this;
    }

    /** L'opération s'est terminée ailleurs : l'onglet reste, et le dit. */
    finished(): this {
        this.isLive = false;
        this.title = "nothing to merge";
        return this;
    }

    build(): MergeTab {
        return {
            kind: "merge",
            tabId: this.tabId,
            worktreeRoot: this.worktreeRoot,
            title: this.title,
            live: this.isLive,
            location: this.located
                ? {
                      worktreeRoot: this.worktreeRoot,
                      worktreeName: basename(this.worktreeRoot),
                      repo: this.repo,
                  }
                : null,
        };
    }
}

/**
 * Test Data Builder : un worktree **épinglé**, tel que le backend l'aurait relu (spec §5.2).
 *
 * Les défauts sont valides et déterministes — un worktree seul sous son dépôt, donc la forme
 * **à plat**, comme pour un onglet. Un scénario ne surcharge que ce qu'il regarde.
 *
 * Il vit à côté de [`TabBuilder`] parce qu'une ligne de la colonne se range de la même façon
 * qu'elle vienne d'un onglet ou d'une épingle : les deux fabriques doivent décrire le même
 * worktree quand elles nomment la même racine.
 */
export class PinBuilder {
    private worktreeRoot = "/dev/solo";
    private worktreeName = "solo";
    private repo: { id: string; name: string } | null = null;

    static create(root: string): PinBuilder {
        const pin = new PinBuilder();
        pin.worktreeRoot = root;
        pin.worktreeName = basename(root);
        return pin;
    }

    /** Un worktree **groupé** sous son dépôt commun — les mêmes clés que `TabBuilder`. */
    ofRepo(repoName: string, repoId = `/dev/${repoName}/.git`): this {
        this.repo = { id: repoId, name: repoName };
        return this;
    }

    build(): PinnedWorktree {
        return {
            worktreeRoot: this.worktreeRoot,
            worktreeName: this.worktreeName,
            repo: this.repo,
        };
    }
}

/**
 * Test Data Builder : l'état git d'un worktree, tel que la surveillance le décrirait.
 *
 * Les défauts sont valides et déterministes — une branche, aucune opération en cours, un
 * arbre **propre** qui ne suit aucune amont. C'est le cas nominal ; un scénario ne
 * surcharge que ce qu'il regarde.
 *
 * L'arbre propre par défaut est un choix : il oblige chaque test qui parle de `+3 ~1` à
 * le dire, et distingue à l'écriture l'arbre propre du `status` absent — les deux se
 * rendent différemment, et les confondre est exactement le bug qu'on veut empêcher.
 */
export class MetadataBuilder {
    private head: GitHead = { kind: "branch", name: "feat/agent-sidebar" };
    private operation: GitOperation | null = null;
    private tree = { added: 0, modified: 0, deleted: 0, conflicted: 0 };
    private upstream: { ahead: number; behind: number } | null = null;
    private conflicts: string[] = [];
    private known = true;

    static create(): MetadataBuilder {
        return new MetadataBuilder();
    }

    onBranch(name: string): this {
        this.head = { kind: "branch", name };
        return this;
    }

    /** Un `git checkout <sha>`, ou un rebase en cours. */
    detachedAt(commit: string): this {
        this.head = { kind: "detached", commit };
        return this;
    }

    /** Un rebase arrêté : `HEAD` détaché, la branche déplacée nommée par `head-name`. */
    rebasing(branch: string | null, onto: string | null, step?: number, total?: number): this {
        this.head = { kind: "detached", commit: "a1b2c3d" };
        this.operation = {
            kind: "rebase",
            branch,
            onto,
            progress: step !== undefined && total !== undefined ? { step, total } : null,
        };
        return this;
    }

    /** Un merge arrêté sur conflit : la branche courante reste celle de `HEAD`. */
    merging(onto: string): this {
        this.operation = { kind: "merge", branch: null, onto, progress: null };
        return this;
    }

    withTree(tree: Partial<GitStatus["tree"]>): this {
        this.tree = { ...this.tree, ...tree };
        return this;
    }

    withUpstream(ahead: number, behind: number): this {
        this.upstream = { ahead, behind };
        return this;
    }

    /** Les chemins qui attendent une décision, et leur compte, tenus cohérents. */
    withConflicts(...paths: string[]): this {
        this.conflicts = paths;
        this.tree = { ...this.tree, conflicted: paths.length };
        return this;
    }

    /** `git` absent, trop lent, ou en échec. **Pas** un arbre propre. */
    withoutStatus(): this {
        this.known = false;
        return this;
    }

    build(): WorktreeMetadata {
        return {
            head: this.head,
            operation: this.operation,
            status: this.known
                ? { tree: this.tree, upstream: this.upstream, conflicts: this.conflicts }
                : null,
        };
    }
}

function basename(path: string): string {
    const segments = path.split("/").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? "/";
}

/**
 * Test Data Builder : ce que l'onglet de merge montre (spec §7.4).
 *
 * Les défauts décrivent le décor du ticket — un rebase de `feat` sur `main`, arrêté sur un
 * fichier à un hunk, `continue` éteint. Les deux côtés portent des **noms de branche**, et
 * c'est ce que les scénarios inversent quand ils parlent d'un merge.
 */
export class MergeViewBuilder {
    private title = "rebase feat onto main";
    private left: SideLabel = { name: "main", role: "the branch you are rebasing onto" };
    private right: SideLabel = { name: "feat", role: "your commits, being replayed" };
    private kind: GitOperationKind = "rebase";
    private files: ConflictFile[] = [conflicted("src/probe.rs")];
    private hidden = 0;
    private escapes = ["git rebase --abort", "git rebase --skip"];
    private stopped = true;

    static create(): MergeViewBuilder {
        return new MergeViewBuilder();
    }

    /** Le même conflit, pris dans l'autre sens : `main` reste à gauche, son rôle change. */
    merging(): this {
        this.kind = "merge";
        this.title = "merge feat into main";
        this.left = { name: "main", role: "the branch you are on" };
        this.right = { name: "feat", role: "the branch being merged in" };
        this.escapes = ["git merge --abort"];
        return this;
    }

    withFiles(...files: ConflictFile[]): this {
        this.files = files;
        return this;
    }

    /** git compte plus de conflits que la liste n'en porte — elle est bornée à cent. */
    withHidden(hidden: number): this {
        this.hidden = hidden;
        return this;
    }

    /** L'opération s'est terminée ailleurs : il n'y a plus rien à résoudre. */
    finished(): this {
        this.stopped = false;
        return this;
    }

    build(): MergeView {
        const unresolved = this.files.filter((file) => !file.resolved).length;
        return {
            tabId: "merge-1",
            worktreeRoot: "/dev/ash",
            title: this.stopped ? this.title : "nothing to merge",
            stopped: this.stopped
                ? {
                      operation: {
                          kind: this.kind,
                          branch: this.kind === "merge" ? null : "feat",
                          onto: this.kind === "merge" ? "feat" : "main",
                          progress: this.kind === "merge" ? null : { step: 2, total: 5 },
                      },
                      sides: { left: this.left, right: this.right },
                      files: this.files,
                      hidden: this.hidden,
                      unresolved,
                      origHead: "80eca44",
                      escapes: this.escapes,
                      continueCommand: `git ${this.kind} --continue`,
                      canContinue: unresolved === 0 && this.hidden === 0,
                  }
                : null,
        };
    }
}

/** Un fichier en conflit, avec autant de hunks qu'on lui en donne de côtés. */
export function conflicted(path: string, hunks = 1): ConflictFile {
    return {
        path,
        hunks: Array.from({ length: hunks }, (_unused, index) => ({
            index,
            ours: `main ${String(index)}\n`,
            base: null,
            theirs: `feat ${String(index)}\n`,
        })),
        resolved: hunks === 0,
        unreadable: false,
    };
}

/** Un chemin que git a dû échapper : Ash le liste, le compte, et ne l'ouvre pas. */
export function unreadableConflict(path: string): ConflictFile {
    return { path, hunks: [], resolved: false, unreadable: true };
}

/**
 * Les deux quotas du compte, dans la position de la maquette (vue 5b).
 *
 * Défauts **déterministes** : `63 %` et `28 %`, avec deux dates de remise à zéro fixes — un
 * `Date.now()` dans un défaut ferait dépendre un test de l'heure à laquelle il tourne, et le
 * ferait tomber une fois par an sur un changement d'heure.
 *
 * Chaque quota se retire seul (`.withoutSession()`), parce que c'est exactement le cas que
 * le backend laisse passer : un compte migré, ou une réponse partielle, n'en porte qu'un.
 */
export class AccountUsageBuilder {
    private session: Quota | null = { percent: 63, resetsAt: 1_787_249_640_000 };
    private weekly: Quota | null = { percent: 28, resetsAt: 1_787_475_600_000 };

    withSession(percent: number, resetsAt: number | null = 1_787_249_640_000): this {
        this.session = { percent, resetsAt };
        return this;
    }

    withWeekly(percent: number, resetsAt: number | null = 1_787_475_600_000): this {
        this.weekly = { percent, resetsAt };
        return this;
    }

    /** Le quota de session n'existe pas — et rien ne doit suggérer qu'il a échoué. */
    withoutSession(): this {
        this.session = null;
        return this;
    }

    withoutWeekly(): this {
        this.weekly = null;
        return this;
    }

    build(): AccountUsage {
        return { session: this.session, weekly: this.weekly };
    }
}

/** Ce qu'Ash rend quand il ne sait rien : rien. Voir [`AccountUsage`]. */
export function noAccountUsage(): AccountUsage {
    return new AccountUsageBuilder().withoutSession().withoutWeekly().build();
}
