import { agentGlyph as glyph, presentAgentState } from "@/shared/agent-state";
import { composeSidebarHeader, type SidebarHeaderModel } from "./header";
import { abbreviate } from "./labels";
import { composeSubagentRow, type SubagentNode } from "./subagents";
import type { SidebarGroup, SidebarTabNode, SidebarTree, WorktreeNode } from "./tree";
import { planGroup, planRailEntry, planTab, planWorktree } from "./visible";

/**
 * Le rendu de la sidebar. Il ne décide rien : il reçoit l'arbre que [`buildSidebar`] a
 * produit et le pose dans le DOM.
 *
 * Deux décisions lui sont retirées, parce qu'elles ne se vérifieraient pas ici — `bun test`
 * n'a pas de DOM : la forme de l'en-tête ([`./header`]) et ce qu'une ligne repliée montre
 * à la place de ses enfants ([`./visible`]). La vue peint leur résultat, elle ne le
 * recalcule pas ; c'est ce qui rend la garantie « une ligne repliée ne cache jamais un
 * agent qui attend » testable.
 *
 * Le DOM est reconstruit à chaque rendu, comme la barre d'onglets : quelques dizaines de
 * nœuds, contre le risque bien réel d'une colonne qui diverge de l'ordre que le backend
 * détient.
 */
export interface SidebarViewActions {
    selectTab(tabId: string): void;
    /** Replier ou déplier un worktree — propriété du worktree, pas du dépôt (ADR-0012). */
    toggleWorktree(key: string): void;
    /** Replier ou déplier un groupe de dépôt, par sa clé de groupe (spec §4.1). */
    toggleGroup(key: string): void;
    /** Le `+` du pied : un onglet de plus dans le worktree courant. */
    newTab(): void;
}

export class SidebarView {
    readonly element: HTMLElement;

    private readonly body = document.createElement("div");
    private readonly head = document.createElement("div");
    /** L'instant du rendu en cours — voir [`SidebarView.render`]. */
    private now = 0;

    constructor(private readonly actions: SidebarViewActions) {
        this.element = document.createElement("aside");
        this.element.className = "ash-sidebar";

        this.head.className = "ash-sidebar-head";
        this.body.className = "ash-sidebar-body";

        this.element.append(this.head, this.body, this.foot());
    }

    /**
     * `now` est l'horloge du **rendu**, et non un état de la colonne : les durées des lignes
     * filles s'en déduisent, exactement comme celle de la ligne de statut. Le passer en
     * paramètre plutôt que de lire `Date.now()` ici est ce qui rend la composition d'une
     * ligne fille vérifiable sans DOM ([`./subagents`]).
     */
    render(tree: SidebarTree, collapsedColumn: boolean, now: number): void {
        this.now = now;
        this.element.classList.toggle("is-collapsed", collapsedColumn);
        this.header(composeSidebarHeader(tree, collapsedColumn));

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

    /**
     * `1 waiting / 7 agents`, ou `❯1` à 46 px.
     *
     * La phrase entière reste portée par le `title` et l'`aria-label` dans les deux formes :
     * replier la colonne abrège l'affichage, jamais l'information.
     */
    private header(model: SidebarHeaderModel): void {
        this.head.classList.toggle("is-compact", model.shape === "compact");
        this.head.title = model.summary;
        this.head.setAttribute("aria-label", model.summary);

        if (model.shape === "compact") {
            // `❯3` ne se lit pas à voix haute. Un `aria-label` seul n'y suffit pas : posé
            // sur un `div`, il tombe sur le rôle `generic`, que les lecteurs d'écran
            // n'exposent pas — la phrase serait écrite dans le DOM sans que personne ne
            // l'entende. `role="img"` donne à l'en-tête un rôle qui accepte un nom, et
            // masque le glyphe qu'il remplace. La forme longue n'en a pas besoin : son
            // texte est déjà lisible, et un rôle l'empêcherait de l'être.
            this.head.setAttribute("role", "img");

            const badge = document.createElement("span");
            badge.className = "ash-sidebar-badge";
            if (model.badge.urgent) badge.classList.add("is-urgent");
            if (model.badge.state !== null) badge.append(glyph(model.badge.state));
            badge.append(text("span", String(model.badge.count), "ash-sidebar-badge-count"));

            this.head.replaceChildren(badge);
            return;
        }

        this.head.removeAttribute("role");

        const count = document.createElement("span");
        count.className = "ash-sidebar-count";
        count.append(
            ...model.chips.map((chip) =>
                text("span", chip.text, chip.tone === "plain" ? undefined : `ash-${chip.tone}`),
            ),
        );

        this.head.replaceChildren(text("span", model.title), count);
    }

    private groupRows(group: SidebarGroup): HTMLElement[] {
        const plan = planGroup(group);
        const rows =
            group.kind === "flat"
                ? // Deux niveaux visibles, et pas trois : un dépôt sans worktree lié ne
                  // gagne jamais de ligne intermédiaire (ADR-0012).
                  []
                : [this.repoRow(group)];

        const shape = group.kind === "flat" ? "flat" : "grouped";
        return [
            ...rows,
            ...plan.children.flatMap((worktree) => [
                this.worktreeRow(worktree, shape),
                ...planWorktree(worktree).children.flatMap((tab) => [
                    this.tabRow(tab, shape),
                    // Les lignes filles suivent immédiatement la ligne de leur onglet, et
                    // n'ont pas de repli à elles : elles se montrent, ou n'existent pas.
                    ...planTab(tab).children.map((child) => this.subagentRow(child, tab)),
                ]),
            ]),
        ];
    }

    private repoRow(group: Extract<SidebarGroup, { kind: "repo" }>): HTMLElement {
        const row = document.createElement("button");
        row.type = "button";
        row.className = "ash-repo";
        row.setAttribute("aria-expanded", String(!group.collapsed));

        const chevron = text("span", group.collapsed ? "▸" : "▾", "ash-chevron");
        const name = text("span", group.label, "ash-repo-name");
        name.title = group.title;

        const worktrees = group.worktrees.length;
        row.append(
            chevron,
            name,
            spacer(),
            text("span", `${worktrees} worktree${worktrees > 1 ? "s" : ""}`, "ash-repo-count"),
        );
        // Repliée, la ligne doit encore dire ce qui se passe en dessous d'elle.
        const badge = planGroup(group).badge;
        if (badge !== null) row.append(glyph(badge));

        row.addEventListener("click", () => {
            this.actions.toggleGroup(group.key);
        });
        return row;
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
        const badge = planWorktree(worktree).badge;
        if (badge !== null) row.append(glyph(badge));

        row.addEventListener("click", () => {
            this.actions.toggleWorktree(worktree.key);
        });
        return row;
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
     * Une ligne de sous-agent : son libellé, son état, sa durée — et aucun geste à elle.
     *
     * **Ce n'est pas un bouton, et c'est la décision qui compte ici** : un sous-agent n'a pas
     * de terminal ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)), donc rien à
     * sélectionner. Un clic sur la ligne sélectionne le **parent**, qui est le seul onglet
     * qu'il y ait à montrer ; et comme elle n'est pas un bouton, `tab` ne s'y arrête pas —
     * le parcours clavier ne gagne pas une étape qui ne mène nulle part.
     */
    private subagentRow(child: SubagentNode, parent: SidebarTabNode): HTMLElement {
        const shown = composeSubagentRow(child, this.now);
        const presented = presentAgentState(shown.state);

        const row = document.createElement("div");
        row.className = `ash-subagent ${presented.className}`;
        if (presented.struck) row.classList.add("is-struck");

        const name = text("span", shown.label, "ash-subagent-name");
        name.title = shown.title;

        row.append(glyph(shown.state), name, spacer(), text("span", shown.status, "ash-subagent-state"));
        row.addEventListener("click", () => {
            this.actions.selectTab(parent.tabId);
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
        const plan = planRailEntry(group);
        const shown = presentAgentState(plan.badge);

        const entry = document.createElement("div");
        entry.className = "ash-rail-entry";
        if (shown.tinted) entry.classList.add("is-tinted");
        entry.title = label;

        const initials = text("span", abbreviate(label), "ash-rail-initials");
        if (shown.tinted) initials.classList.add("ash-accent");

        entry.append(initials, ...plan.children.map((tab) => glyph(tab.state)));
        return entry;
    }

    private foot(): HTMLElement {
        const foot = document.createElement("div");
        foot.className = "ash-sidebar-foot";

        const add = document.createElement("button");
        add.type = "button";
        add.className = "ash-sidebar-add";
        add.textContent = "+ tab";
        add.title = "Nouvel onglet dans le worktree courant (⌘T)";
        add.addEventListener("click", () => {
            this.actions.newTab();
        });

        foot.append(add, text("span", "⌘T", "ash-sidebar-hint"));
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
