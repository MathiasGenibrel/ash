import type { AgentState, TabInfo } from "@/shared/ipc";

/**
 * Test Data Builder : un onglet tel que le backend le décrirait.
 *
 * Les défauts sont valides et déterministes — un `zsh` à son invite, dans un dépôt sans
 * worktree lié, donc **à plat**. Un scénario ne surcharge que ce qu'il regarde.
 *
 * Ce fichier n'est importé que par les tests ; il n'est pas dans l'API publique de la
 * feature (`index.ts`), et rien du bundle applicatif n'y touche.
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

function basename(path: string): string {
    const segments = path.split("/").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? "/";
}
