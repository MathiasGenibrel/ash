import type {
    AgentState,
    GitHead,
    GitOperation,
    GitStatus,
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

    inState(state: AgentState): this {
        this.state = state;
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
            state: this.state,
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

    /** `git` absent, trop lent, ou en échec. **Pas** un arbre propre. */
    withoutStatus(): this {
        this.known = false;
        return this;
    }

    build(): WorktreeMetadata {
        return {
            head: this.head,
            operation: this.operation,
            status: this.known ? { tree: this.tree, upstream: this.upstream } : null,
        };
    }
}

function basename(path: string): string {
    const segments = path.split("/").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? "/";
}
