import { agentGlyph as glyph, presentAgentState } from "@/shared/agent-state";
import { abbreviate } from "./labels";
import type { SidebarGroup, SidebarTabNode, SidebarTree, WorktreeNode } from "./tree";

/**
 * Le rendu de la sidebar. Il ne décide rien : il reçoit l'arbre que [`buildSidebar`] a
 * produit et le pose dans le DOM.
 *
 * Le DOM est reconstruit à chaque rendu, comme la barre d'onglets : quelques dizaines de
 * nœuds, contre le risque bien réel d'une colonne qui diverge de l'ordre que le backend
 * détient.
 */
export interface SidebarViewActions {
    selectTab(tabId: string): void;
    /** Replier ou déplier un worktree — propriété du worktree, pas du dépôt (ADR-0012). */
    toggleWorktree(key: string): void;
    /** Le `+` du pied : un onglet de plus dans le worktree courant. */
    newTab(): void;
}

export class SidebarView {
    readonly element: HTMLElement;

    private readonly body = document.createElement("div");
    private readonly count = document.createElement("span");

    constructor(private readonly actions: SidebarViewActions) {
        this.element = document.createElement("aside");
        this.element.className = "ash-sidebar";

        const header = document.createElement("div");
        header.className = "ash-sidebar-head";
        const title = document.createElement("span");
        title.textContent = "workspaces";
        this.count.className = "ash-sidebar-count";
        header.append(title, this.count);

        this.body.className = "ash-sidebar-body";

        this.element.append(header, this.body, this.foot());
    }

    render(tree: SidebarTree, collapsedColumn: boolean): void {
        this.element.classList.toggle("is-collapsed", collapsedColumn);
        this.count.replaceChildren(...this.counters(tree));

        if (tree.tabCount === 0) {
            this.body.replaceChildren(emptyState());
            return;
        }

        this.body.replaceChildren(
            ...(collapsedColumn
                ? tree.groups.map((group) => this.railEntry(group))
                : tree.groups.flatMap((group) => this.groupRows(group))),
        );
    }

    /** `1 waiting / 7 agents` — ou simplement `0` quand il n'y a rien. */
    private counters(tree: SidebarTree): Node[] {
        if (tree.tabCount === 0) return [text("span", "0")];

        const agents = text("span", `${tree.tabCount} agents`);
        if (tree.waitingCount === 0) return [agents];

        return [
            text("span", `${tree.waitingCount} waiting`, "ash-accent"),
            text("span", "/", "ash-rule"),
            agents,
        ];
    }

    private groupRows(group: SidebarGroup): HTMLElement[] {
        if (group.kind === "flat") {
            // Deux niveaux visibles, et pas trois : un dépôt sans worktree lié ne gagne
            // jamais de ligne intermédiaire (ADR-0012).
            return [
                this.worktreeRow(group.worktree, "flat"),
                ...this.tabRows(group.worktree, "flat"),
            ];
        }

        const header = document.createElement("div");
        header.className = "ash-repo";
        const name = text("span", group.label, "ash-repo-name");
        name.title = group.title;
        const worktrees = group.worktrees.length;
        header.append(
            name,
            spacer(),
            text("span", `${worktrees} worktree${worktrees > 1 ? "s" : ""}`, "ash-repo-count"),
        );

        return [
            header,
            ...group.worktrees.flatMap((worktree) => [
                this.worktreeRow(worktree, "grouped"),
                ...this.tabRows(worktree, "grouped"),
            ]),
        ];
    }

    private worktreeRow(worktree: WorktreeNode, shape: "flat" | "grouped"): HTMLElement {
        const row = document.createElement("button");
        row.type = "button";
        row.className = `ash-worktree is-${shape}`;
        row.setAttribute("aria-expanded", String(!worktree.collapsed));

        const chevron = text("span", worktree.collapsed ? "▸" : "▾", "ash-chevron");
        const name = text("span", worktree.label, "ash-worktree-name");
        name.title = worktree.title;

        row.append(chevron, name, spacer());
        if (worktree.suffix !== null) {
            row.append(text("span", worktree.suffix, "ash-worktree-suffix"));
        }
        // Repliée, la ligne doit encore dire ce qui se passe en dessous d'elle.
        if (worktree.collapsed) row.append(glyph(worktree.state));

        row.addEventListener("click", () => {
            this.actions.toggleWorktree(worktree.key);
        });
        return row;
    }

    private tabRows(worktree: WorktreeNode, shape: "flat" | "grouped"): HTMLElement[] {
        if (worktree.collapsed) return [];
        return worktree.tabs.map((tab) => this.tabRow(tab, shape));
    }

    private tabRow(tab: SidebarTabNode, shape: "flat" | "grouped"): HTMLElement {
        const shown = presentAgentState(tab.state);
        const row = document.createElement("button");
        row.type = "button";
        row.className = `ash-agent is-${shape} ${shown.className}`;
        if (tab.active) row.classList.add("is-selected");
        if (shown.tinted) row.classList.add("is-tinted");
        if (shown.rail !== "none") row.classList.add(`has-${shown.rail}-rail`);

        const name = text("span", tab.label, "ash-agent-name");
        name.title = tab.title;
        if (shown.struck) name.classList.add("is-struck");

        row.append(glyph(tab.state), name, text("span", shown.label, "ash-agent-state"));
        row.addEventListener("click", () => {
            this.actions.selectTab(tab.tabId);
        });
        return row;
    }

    /**
     * Une entrée du rail replié : deux lettres, puis les glyphes de ses agents.
     *
     * C'est ce qui fait qu'à 46 px la colonne garde encore un sens — sans quoi `⌘B` ne
     * replierait pas la sidebar, il l'effacerait.
     */
    private railEntry(group: SidebarGroup): HTMLElement {
        const label = group.kind === "repo" ? group.title : group.worktree.title;
        const shown = presentAgentState(group.state);

        const entry = document.createElement("div");
        entry.className = "ash-rail-entry";
        if (shown.tinted) entry.classList.add("is-tinted");
        entry.title = label;

        const initials = text("span", abbreviate(label), "ash-rail-initials");
        if (shown.tinted) initials.classList.add("ash-accent");

        const tabs =
            group.kind === "repo"
                ? group.worktrees.flatMap((worktree) => worktree.tabs)
                : group.worktree.tabs;

        entry.append(initials, ...tabs.map((tab) => glyph(tab.state)));
        return entry;
    }

    private foot(): HTMLElement {
        const foot = document.createElement("div");
        foot.className = "ash-sidebar-foot";

        const add = document.createElement("button");
        add.type = "button";
        add.className = "ash-sidebar-add";
        add.textContent = "+ tab";
        add.title = "Nouvel onglet dans le worktree courant (⌘N)";
        add.addEventListener("click", () => {
            this.actions.newTab();
        });

        foot.append(add, text("span", "⌘N", "ash-sidebar-hint"));
        return foot;
    }
}

function emptyState(): HTMLElement {
    const empty = document.createElement("div");
    empty.className = "ash-sidebar-empty";
    empty.append(
        text("p", "no workspaces."),
        text("p", "a workspace is one git root.", "ash-sidebar-hint"),
    );
    return empty;
}

function spacer(): HTMLElement {
    return text("span", "", "ash-spacer");
}

function text(tag: string, content: string, className?: string): HTMLElement {
    const element = document.createElement(tag);
    element.textContent = content;
    if (className !== undefined) element.className = className;
    return element;
}
