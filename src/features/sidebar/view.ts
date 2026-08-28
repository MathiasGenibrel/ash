import { agentGlyph as glyph, agentRowClasses, presentAgentState } from "@/shared/agent-state";
import { composeSidebarHeader, type SidebarHeaderModel } from "./header";
import type { InstrumentationMark } from "./instrumentation";
import { abbreviate, newTabHint } from "./labels";
import { pinMark, worktreeGesture } from "./pinning";
import { composeSubagentRow, type SubagentNode } from "./subagents";
import type { RowLabel, SidebarGroup, SidebarTabNode, SidebarTree, WorktreeNode } from "./tree";
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
 * Le DOM est reconstruit à chaque rendu, comme la ligne de statut : quelques dizaines de
 * nœuds, contre le risque bien réel d'une colonne qui diverge de l'ordre que le backend
 * détient.
 */
export interface SidebarViewActions {
    selectTab(tabId: string): void;
    /**
     * Replier ou déplier **une ligne**, par sa clé — jamais la colonne, qui est `⌘B` et ne
     * passe pas par ici.
     *
     * Un worktree et un groupe de dépôt sont deux lignes (ADR-0012, spec §4.1) mais un seul
     * fait : leurs clés ne peuvent pas se confondre — un chemin absolu d'un côté, une clé
     * préfixée de l'autre — et `~/.ash/state.json` n'en garde qu'une liste.
     */
    toggleRowCollapsed(key: string): void;
    /** Le `+` du pied : un onglet de plus dans le worktree courant. */
    newTab(): void;
    /**
     * Le clic sur une ligne de worktree **sans onglet** : en ouvrir un là (spec §5.2).
     *
     * Une telle ligne n'existe que parce qu'elle est épinglée, et elle n'a rien à replier :
     * son clic est donc le seul de la colonne qui ouvre quelque chose. Voir [`./pinning`].
     */
    openTabIn(worktreeRoot: string): void;
    /**
     * Épingler ou désépingler un worktree.
     *
     * Rien n'est posé ici : l'épingle vit en Rust, survit à la fermeture, et revient par
     * l'annonce du backend ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    setPinned(worktreeRoot: string, pinned: boolean): void;
    /**
     * Le geste du marqueur : ouvrir la fenêtre de réglages sur cet outil.
     *
     * Il **n'écrit rien** — l'écriture chez l'utilisateur reste un geste explicite fait dans
     * l'écran, par le flux qui existe déjà (ADR-0007, ADR-0010).
     */
    instrument(command: string, adapter: string): void;
}

export class SidebarView {
    readonly element: HTMLElement;

    private readonly body = document.createElement("div");
    private readonly head = document.createElement("div");
    /**
     * Les deux endroits du pied qui **annoncent** le raccourci de « nouvel onglet ».
     *
     * Gardés parce que le pied est bâti une fois : la colonne se redessine à chaque onglet,
     * lui n'a aucune raison de le faire, et une liaison ne change pas au rythme des onglets.
     */
    private readonly add = document.createElement("button");
    private readonly hint = text("span", "", "ash-sidebar-hint");
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

        // **`groups`, et non `tabCount`** : une colonne sans onglet n'est plus une colonne
        // vide depuis qu'une épingle y fait exister une ligne (spec §5.2). Compter les
        // onglets ici effacerait justement ce que l'épingle sert à garder.
        if (tree.groups.length === 0) {
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
                ? // Deux niveaux visibles, et pas trois : un dépôt qui n'héberge qu'un
                  // worktree ne gagne jamais de ligne intermédiaire (ADR-0012, amendement
                  // du 2026-08-26).
                  []
                : [this.repoRow(group)];

        const shape = group.kind === "flat" ? "flat" : "grouped";
        return [
            ...rows,
            ...plan.children.flatMap((worktree) => [
                // À plat, la ligne écrit ce que le groupe dit — le nom du dépôt —, et non
                // celui de son worktree, qui le répéterait. Elle désigne toujours le
                // worktree : c'est lui qu'elle replie, épingle et ouvre.
                this.worktreeRow(worktree, shape, group.kind === "flat" ? group.row : worktree),
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

        // Toujours au pluriel, et sans condition : cette ligne n'existe qu'à partir de deux
        // worktrees (ADR-0012, amendement du 2026-08-26). Le `1 worktree` d'avant comptait
        // ce que la ligne du dessous disait déjà, et n'informait de rien.
        row.append(
            chevron,
            name,
            spacer(),
            text("span", `${group.worktrees.length} worktrees`, "ash-repo-count"),
        );
        // Repliée, la ligne doit encore dire ce qui se passe en dessous d'elle.
        const badge = planGroup(group).badge;
        if (badge !== null) row.append(glyph(badge));

        row.addEventListener("click", () => {
            this.actions.toggleRowCollapsed(group.key);
        });
        return row;
    }

    /**
     * La ligne d'un worktree — et, dans la forme à plat, **la** ligne du groupe.
     *
     * `shown` est ce qu'elle écrit, `worktree` ce qu'elle désigne. Les deux coïncident sous
     * un dépôt à plusieurs worktrees ; ils divergent quand un dépôt n'en héberge qu'un, où
     * la ligne porte le nom du dépôt sans cesser de replier, d'épingler et d'ouvrir son
     * worktree (ADR-0012, amendement du 2026-08-26).
     */
    private worktreeRow(
        worktree: WorktreeNode,
        shape: "flat" | "grouped",
        shown: RowLabel,
    ): HTMLElement {
        const gesture = worktreeGesture(worktree);
        const row = document.createElement("button");
        row.type = "button";
        row.className = `ash-worktree is-${shape}`;
        if (worktree.pinned) row.classList.add("is-pinned");

        const name = text("span", shown.label, "ash-worktree-name");
        name.title = shown.title;

        if (gesture === "open-tab") {
            // Aucun onglet dessous : pas de chevron — il ne replierait rien —, et une ligne
            // qui dit ce que son clic fait. C'est la ligne qu'une épingle fait exister.
            row.title = `open a tab in ${worktree.title}`;
            row.append(text("span", "+", "ash-chevron"), name, spacer());
        } else {
            row.setAttribute("aria-expanded", String(!worktree.collapsed));
            row.append(text("span", worktree.collapsed ? "▸" : "▾", "ash-chevron"), name, spacer());
        }

        if (shown.suffix !== null) {
            row.append(text("span", shown.suffix, "ash-worktree-suffix"));
        }
        // Repliée, la ligne doit encore dire ce qui se passe en dessous d'elle.
        const badge = planWorktree(worktree).badge;
        if (badge !== null) row.append(glyph(badge));
        row.append(this.pin(worktree));

        row.addEventListener("click", () => {
            if (gesture === "open-tab") {
                this.actions.openTabIn(worktree.key);
                return;
            }
            this.actions.toggleRowCollapsed(worktree.key);
        });
        return row;
    }

    /**
     * L'épingle d'une ligne de worktree (spec §5.2).
     *
     * **Ce n'est pas un `<button>`**, et pour la raison exacte qui vaut au marqueur
     * d'instrumentation : la ligne en est déjà un, et un bouton dans un bouton est un DOM
     * invalide que les lecteurs d'écran rendent au hasard. C'est un `span` à qui l'on donne
     * le rôle, le nom et la place dans le parcours clavier — et dont le clic **n'atteint
     * pas** la ligne : épingler n'est ni replier ni ouvrir.
     */
    private pin(worktree: WorktreeNode): HTMLElement {
        const mark = pinMark(worktree);
        const element = text("span", mark.glyph, "ash-worktree-pin");
        if (worktree.pinned) element.classList.add("is-pinned");
        element.title = mark.title;
        element.setAttribute("aria-label", mark.title);
        element.setAttribute("role", "button");
        element.tabIndex = 0;

        const toggle = (event: Event): void => {
            event.stopPropagation();
            this.actions.setPinned(worktree.key, mark.pin);
        };
        element.addEventListener("click", toggle);
        element.addEventListener("keydown", (event) => {
            if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                toggle(event);
            }
        });
        return element;
    }

    private tabRow(tab: SidebarTabNode, shape: "flat" | "grouped"): HTMLElement {
        if (tab.state === null) return this.toolRow(tab, shape);

        const shown = presentAgentState(tab.state);
        const row = document.createElement("button");
        row.type = "button";
        // Ce que la ligne décide elle-même, c'est sa forme dans l'arbre — le reste vient de
        // `shared/agent-state`, canal par canal, et c'est ce qui garde le filet gauche à la
        // sélection seule (#181). La miniature des réglages compose la sienne avec les mêmes
        // classes, sorties du même appel : une divergence n'aurait pas d'endroit où naître.
        row.className = `ash-agent is-${shape}`;
        row.classList.add(...agentRowClasses(tab.state, tab.active));

        const name = text("span", tab.label, "ash-agent-name");
        name.title = tab.title;
        if (shown.struck) name.classList.add("is-struck");

        row.append(glyph(tab.state), name, text("span", shown.label, "ash-agent-state"));
        if (tab.mark !== null) row.append(this.instrumentationMark(tab.mark));
        row.addEventListener("click", () => {
            this.actions.selectTab(tab.tabId);
        });
        return row;
    }

    /**
     * La ligne d'un onglet qui n'est **pas** un agent — la surface de merge (#30).
     *
     * Ni glyphe d'état, ni teinte, ni lame : elle n'en a aucun, et lui prêter le `idle` d'un
     * shell à son invite ferait remonter un état inventé jusqu'à la ligne du dépôt. Ce
     * qu'elle porte à droite, c'est ce qu'elle **est** — `merge` —, et son clic sélectionne
     * l'onglet comme n'importe quelle autre ligne.
     */
    private toolRow(tab: SidebarTabNode, shape: "flat" | "grouped"): HTMLElement {
        const row = document.createElement("button");
        row.type = "button";
        row.className = `ash-agent is-${shape} is-tool`;
        if (tab.active) row.classList.add("is-selected");

        const name = text("span", tab.label, "ash-agent-name");
        name.title = tab.title;

        row.append(
            text("span", "⑂", "ash-agent-glyph"),
            name,
            text("span", "merge", "ash-agent-state"),
        );
        row.addEventListener("click", () => {
            this.actions.selectTab(tab.tabId);
        });
        return row;
    }

    /**
     * Le marqueur « non instrumenté » d'une ligne d'agent (ADR-0006).
     *
     * **Ce n'est pas un `<button>`** : la ligne d'onglet en est déjà un, et un bouton dans un
     * bouton est un DOM invalide que les lecteurs d'écran rendent au hasard. C'est un `span`
     * à qui l'on donne le rôle, le nom et la place dans le parcours clavier — et dont le clic
     * **n'atteint pas** la ligne : instrumenter n'est pas sélectionner.
     *
     * Quand rien ne peut être instrumenté, il n'y a pas de geste : le marqueur reste, sans
     * rôle ni tabulation, et sa phrase dit pourquoi.
     */
    private instrumentationMark(mark: InstrumentationMark): HTMLElement {
        const element = text("span", mark.glyph, "ash-agent-mark");
        element.title = mark.title;
        element.setAttribute("aria-label", mark.title);

        const target = mark.instrument;
        if (target === null) return element;

        element.setAttribute("role", "button");
        element.tabIndex = 0;
        element.classList.add("is-actionable");
        const open = (event: Event): void => {
            // La sidebar informe, l'écran agit (ADR-0010) : le geste ouvre les réglages, et
            // n'écrit rien de lui-même. Sans cet arrêt, il sélectionnerait aussi l'onglet.
            event.stopPropagation();
            this.actions.instrument(target.command, target.adapter);
        };
        element.addEventListener("click", open);
        element.addEventListener("keydown", (event) => {
            if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                open(event);
            }
        });
        return element;
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

        row.append(
            glyph(shown.state),
            name,
            spacer(),
            text("span", shown.status, "ash-subagent-state"),
        );
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
        // Le même nom que la colonne dépliée : un dépôt à plat s'y lit par son nom, pas par
        // celui de son worktree — sans quoi `⌘B` renommerait la ligne sous les yeux.
        const label = group.kind === "repo" ? group.title : group.row.title;
        const plan = planRailEntry(group);
        const shown = presentAgentState(plan.badge);

        const entry = document.createElement("div");
        entry.className = "ash-rail-entry";
        if (shown.tinted) entry.classList.add("is-tinted");
        entry.title = label;

        const initials = text("span", abbreviate(label), "ash-rail-initials");
        if (shown.tinted) initials.classList.add("ash-accent");

        // Une surface d'outil n'a pas d'état, donc pas de pastille : le rail replié ne
        // montre que ce qui en a un.
        entry.append(
            initials,
            ...plan.children.flatMap((tab) => (tab.state === null ? [] : [glyph(tab.state)])),
        );
        return entry;
    }

    private foot(): HTMLElement {
        const foot = document.createElement("div");
        foot.className = "ash-sidebar-foot";

        this.add.type = "button";
        this.add.className = "ash-sidebar-add";
        this.add.textContent = "+ tab";
        this.add.title = newTabHint("").title;
        this.add.addEventListener("click", () => {
            this.actions.newTab();
        });

        foot.append(this.add, this.hint);
        return foot;
    }

    /**
     * Annonce le raccourci **en vigueur** de « nouvel onglet », ou rien du tout.
     *
     * La colonne ne le sait pas et n'a pas à le savoir : les liaisons sont détenues en Rust
     * et réglables (spec §4.4), donc `⌘T` écrit ici deviendrait faux au premier rebinding
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Une action sans
     * raccourci n'affiche **rien** — pas un tiret, pas un « aucun » : le bouton reste, et
     * c'est lui qui fait l'action.
     */
    showNewTabShortcut(keys: string): void {
        const shown = newTabHint(keys);
        this.hint.textContent = shown.hint;
        this.add.title = shown.title;
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
