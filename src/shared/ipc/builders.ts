import type {
    AgentState,
    GitHead,
    GitOperation,
    GitStatus,
    Instrumented,
    RecognizedAgent,
    PinnedWorktree,
    Subagent,
    TabInfo,
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

    /** Le backend n'a pas su situer ce répertoire — `.git` cassé, dépôt disparu. */
    unlocated(cwd: string): this {
        this.cwd = cwd;
        this.located = false;
        return this;
    }

    build(): TabInfo {
        return {
            tabId: this.tabId,
            cwd: this.cwd,
            process: this.process,
            agent: this.agent,
            state: this.state,
            stateSince: this.stateSince,
            subagents: this.subagents,
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
