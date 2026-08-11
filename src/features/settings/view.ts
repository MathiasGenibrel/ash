import type { SettingsSnapshot, ToolDeclaration, ToolDraft } from "./contract";
import { addBlockedReason, degradedModeSubject, describeTool, describeToolCount } from "./model";
import { SETTINGS_SECTIONS, type SettingsSection } from "./sections";

/**
 * Le rendu de la fenêtre de réglages. Il ne décide rien : il reçoit ce que le backend
 * détient et ce que [`model`](./model.ts) en a conclu, et le pose dans le DOM.
 *
 * Le DOM est reconstruit à chaque rendu, comme la sidebar et la barre d'onglets : quelques
 * dizaines de nœuds, contre le risque d'une liste qui diverge de celle du backend.
 *
 * **Ce que cette vue laisse volontairement vide**, et où :
 *
 * - la ligne `test` d'une carte — le glyphe, la phrase et les quatre pastilles de la spec
 *   §9.1 — est une **troisième ligne de la grille** `44px 1fr` de [`toolCard`]. C'est
 *   l'issue #15, et c'est aussi elle qui rendra le champ de chemin et le menu d'adaptateur
 *   modifiables : les deux relancent la vérification, donc les poser sans elle donnerait
 *   des contrôles qui ne mènent à rien ;
 * - la ligne `hooks` en est la quatrième — issue #16 ;
 * - l'encart de découverte de l'état vide (« ash found these commands in your PATH »)
 *   attend que quelque chose sache lire le `PATH` : inventer des candidats serait afficher
 *   les données d'exemple de la maquette.
 */
export interface SettingsViewActions {
    selectSection(section: SettingsSection): void;
    startAdding(): void;
    cancelAdding(): void;
    editDraft(patch: Partial<ToolDraft>): void;
    submitDraft(): void;
    forgetTool(command: string): void;
}

/** Tout ce qu'il faut pour dessiner la fenêtre à un instant donné. */
export interface SettingsScene {
    section: SettingsSection;
    snapshot: SettingsSnapshot;
    /** La saisie en cours, ou `null` quand on n'ajoute pas. */
    draft: ToolDraft | null;
    /** Le dernier refus du backend, s'il en a opposé un. */
    failure: string | null;
}

export class SettingsView {
    readonly element: HTMLElement;

    private readonly nav = document.createElement("nav");
    private readonly panel = document.createElement("section");

    constructor(private readonly actions: SettingsViewActions) {
        this.element = document.createElement("div");
        this.element.className = "settings-layout";
        this.nav.className = "settings-nav";
        this.panel.className = "settings-panel";
        this.element.append(this.nav, this.panel);
    }

    render(scene: SettingsScene): void {
        this.nav.replaceChildren(...this.navRows(scene.section), navHint());
        this.panel.replaceChildren(...this.panelRows(scene));
    }

    /** Donne le focus à la section active — le pendant clavier d'un clic sur sa ligne. */
    focusActiveSection(): void {
        this.nav.querySelector<HTMLElement>(".settings-nav-row.is-active")?.focus();
    }

    private navRows(active: SettingsSection): HTMLElement[] {
        return SETTINGS_SECTIONS.map((section) => {
            // Un vrai bouton, et pas une `div` cliquable : c'est ce qui met la section sur
            // le chemin de `tab` et dans l'arbre d'accessibilité sans une ligne de code.
            const row = document.createElement("button");
            row.type = "button";
            row.className = "settings-nav-row";
            row.textContent = section;
            row.setAttribute("aria-current", section === active ? "true" : "false");
            if (section === active) row.classList.add("is-active");
            row.addEventListener("click", () => {
                this.actions.selectSection(section);
            });
            return row;
        });
    }

    private panelRows(scene: SettingsScene): Node[] {
        if (scene.section !== "tools") return placeholderSection(scene.section);
        return scene.draft === null ? this.toolsSection(scene) : this.addForm(scene, scene.draft);
    }

    /** La section `tools` : son en-tête, sa liste — ou son état vide — et son pied. */
    private toolsSection(scene: SettingsScene): Node[] {
        const tools = scene.snapshot.tools;
        const add = button("add", "is-primary");
        add.addEventListener("click", () => {
            this.actions.startAdding();
        });

        const body = document.createElement("div");
        body.className = "settings-body";
        if (tools.length === 0) {
            body.classList.add("is-empty");
            body.append(emptyState());
        } else {
            body.append(...tools.map((tool) => this.toolCard(tool)));
        }

        return [
            header("tools", describeToolCount(tools), [add]),
            scaleNote(),
            body,
            foot(
                tools.length === 0
                    ? "ash writes to no file until you declare a tool and install its hooks."
                    : "ash writes to no file until an entry is verified.",
            ),
        ];
    }

    private toolCard(tool: ToolDeclaration): HTMLElement {
        const shown = describeTool(tool);
        const card = document.createElement("article");
        card.className = "settings-card";

        const head = document.createElement("div");
        head.className = "settings-card-head";
        head.append(text("span", shown.name, "settings-card-name"));
        if (shown.badge !== null) head.append(text("span", shown.badge, "settings-card-badge"));
        head.append(
            text("span", tool.adapter, "settings-card-adapter"),
            spacer(),
            this.deleteButton(tool.command),
        );

        // La grille `44px 1fr` de la maquette. Les lignes `test` (#15) et `hooks` (#16)
        // s'y ajouteront telles quelles ; les libellés portent des interlignes en pixels
        // précisément pour rester alignés sur des contrôles de hauteurs différentes.
        const body = document.createElement("div");
        body.className = "settings-card-body";
        body.append(text("span", "config", "settings-card-key"), this.pathField(tool));

        card.append(head, body);
        return card;
    }

    /**
     * Le chemin de configuration, **en lecture** à ce jalon.
     *
     * Le rendre modifiable relancerait la vérification à chaque frappe (spec §9.1), et
     * cette vérification est l'issue #15 : un champ qu'on peut changer sans que rien ne le
     * re-juge dirait qu'Ash a accepté le nouveau chemin.
     */
    private pathField(tool: ToolDeclaration): HTMLElement {
        const field = document.createElement("div");
        field.className = "settings-field is-readonly";
        field.append(text("span", describeTool(tool).config, "settings-path"));
        if (!tool.verified) {
            // La pastille « modifié, non enregistré » de la maquette. Elle n'est pas
            // décorative : tant qu'une entrée n'a pas passé les quatre tests, elle vit en
            // mémoire et n'est **pas** dans `~/.ash/config.toml`. C'est le cas de toutes
            // les entrées à ce jalon, et le dire vaut mieux que de le taire.
            const dot = document.createElement("span");
            dot.className = "settings-unsaved";
            dot.setAttribute("aria-label", "not verified — nothing written to config.toml");
            dot.title = "not verified — nothing written to config.toml";
            field.append(dot);
        }
        return field;
    }

    private deleteButton(command: string): HTMLElement {
        const remove = document.createElement("button");
        remove.type = "button";
        remove.className = "settings-icon-button";
        remove.textContent = "✕";
        remove.title = "delete";
        remove.setAttribute("aria-label", `delete ${command}`);
        remove.addEventListener("click", () => {
            this.actions.forgetTool(command);
        });
        return remove;
    }

    /**
     * Le formulaire d'ajout — il **remplace le contenu de la section**, ce n'est ni une
     * modale ni un panneau latéral (§3.8).
     */
    private addForm(scene: SettingsScene, draft: ToolDraft): Node[] {
        const escape = text("button", "esc to cancel", "settings-link");
        escape.addEventListener("click", () => {
            this.actions.cancelAdding();
        });

        const body = document.createElement("div");
        body.className = "settings-body is-form";

        const grid = document.createElement("div");
        grid.className = "settings-form";
        grid.append(
            ...this.field("command", draft.command, "the name you type in the shell", (value) => {
                this.actions.editDraft({ command: value });
            }),
            ...this.field(
                "label",
                draft.label,
                "shown instead of the command",
                (value) => {
                    this.actions.editDraft({ label: value });
                },
                "optional",
            ),
            ...this.adapterField(scene, draft),
            ...this.field("config", draft.config, "adapter default", (value) => {
                this.actions.editDraft({ config: value });
            }),
        );

        const blocked = addBlockedReason(draft, scene.snapshot.tools);
        body.append(grid, this.formActions(blocked ?? scene.failure, blocked === null));

        return [header("new tool", null, [escape]), body];
    }

    private field(
        name: string,
        value: string,
        gloss: string,
        onInput: (value: string) => void,
        placeholder?: string,
    ): Node[] {
        const input = document.createElement("input");
        input.type = "text";
        input.className = "settings-input";
        input.value = value;
        input.setAttribute("aria-label", name);
        if (placeholder !== undefined) input.placeholder = placeholder;
        input.addEventListener("input", () => {
            onInput(input.value);
        });

        const line = document.createElement("div");
        line.className = "settings-form-line";
        line.append(input, text("span", gloss, "settings-gloss"));
        return [text("span", name, "settings-form-key"), line];
    }

    private adapterField(scene: SettingsScene, draft: ToolDraft): Node[] {
        const select = document.createElement("select");
        select.className = "settings-input is-menu";
        select.setAttribute("aria-label", "adapter");
        for (const adapter of scene.snapshot.adapters) {
            const option = document.createElement("option");
            option.value = adapter;
            option.textContent = adapter;
            option.selected = adapter === draft.adapter;
            select.append(option);
        }
        select.addEventListener("change", () => {
            this.actions.editDraft({ adapter: select.value });
        });

        const subject = degradedModeSubject(draft);
        const line = document.createElement("div");
        line.className = "settings-form-line";
        line.append(
            select,
            text(
                "span",
                subject === null ? "" : "degraded mode",
                subject === null ? "settings-gloss" : "settings-gloss is-warning",
            ),
        );

        const rows: Node[] = [text("span", "adapter", "settings-form-key"), line];
        // Une ligne de grille à **cellule de libellé vide** : l'avertissement se range sous
        // le menu qu'il commente, aligné sur lui, pas sur la colonne des libellés.
        if (subject !== null) rows.push(document.createElement("span"), degradedNotice(subject));
        return rows;
    }

    /** La barre d'action, poussée en bas : la raison à gauche, les boutons à droite. */
    private formActions(reason: string | null, canAdd: boolean): HTMLElement {
        const cancel = button("cancel");
        cancel.addEventListener("click", () => {
            this.actions.cancelAdding();
        });

        const add = button("add", "is-primary");
        // Éteint, jamais masqué : « le masquer ferait croire que ça n'existe pas ». La
        // raison reste lisible à gauche.
        add.disabled = !canAdd;
        add.addEventListener("click", () => {
            this.actions.submitDraft();
        });

        const bar = document.createElement("div");
        bar.className = "settings-form-actions";
        bar.append(
            text("span", reason ?? "hooks install after adding, once the four tests pass", "settings-gloss"),
            spacer(),
            cancel,
            add,
        );
        return bar;
    }
}

/** L'en-tête d'une section : titre, compteur, puis les actions à droite. */
function header(title: string, count: string | null, actions: readonly HTMLElement[]): HTMLElement {
    const head = document.createElement("div");
    head.className = "settings-head";
    head.append(text("h1", title, "settings-title"));
    if (count !== null) head.append(text("span", count, "settings-count"));
    head.append(spacer(), ...actions);
    return head;
}

/** La note de barème, sous l'en-tête de `tools` et nulle part ailleurs. */
function scaleNote(): HTMLElement {
    const note = document.createElement("p");
    note.className = "settings-note";
    note.append(
        document.createTextNode(
            "one command = one tool. ash re-runs the tests on every path or adapter change.",
        ),
        document.createElement("br"),
        document.createTextNode("tests · "),
    );
    // Les quatre tests de la spec §9.1, dans l'ordre — leur numéro est d'un ton plus clair
    // que le texte, comme dans la maquette.
    const tests = [
        "folder readable",
        "adapter signature",
        "command in PATH",
        "command uses this folder",
    ];
    tests.forEach((label, index) => {
        if (index > 0) note.append(document.createTextNode(" · "));
        note.append(text("span", String(index + 1), "settings-note-index"));
        note.append(document.createTextNode(` ${label}`));
    });
    return note;
}

function foot(sentence: string): HTMLElement {
    const bar = document.createElement("footer");
    bar.className = "settings-foot";
    bar.append(text("span", sentence, "settings-foot-text"));
    return bar;
}

/**
 * L'état vide : le constat, et ce qu'il coûte.
 *
 * Le corps ne défile pas — il se centre : il n'y a rien à parcourir.
 */
function emptyState(): HTMLElement {
    const empty = document.createElement("div");
    empty.className = "settings-empty";
    empty.append(
        text("p", "no tools declared", "settings-empty-title"),
        text(
            "p",
            "ash already shows your tabs, but it doesn't know which ones are agents. until a tool is declared, everything stays idle — no waiting, no notifications.",
            "settings-empty-prose",
        ),
    );
    return empty;
}

/**
 * L'avertissement du mode dégradé (§3.8).
 *
 * C'est le seul endroit de l'interface où du texte courant est teint par état : les quatre
 * mots portent les classes de `app/styles.css` que la sidebar et la ligne de statut
 * utilisent déjà, donc les mêmes couleurs, définies au même endroit.
 */
function degradedNotice(subject: string): HTMLElement {
    const notice = document.createElement("p");
    notice.className = "settings-degraded";
    // La maquette écrit « ash reads the process output ». Ash ne lit **jamais** la sortie
    // du PTY pour en déduire un état — [ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)
    // l'écarte explicitement. Il observe le processus (la sonde d'ADR-0005). La phrase est
    // corrigée ; ce qu'elle apprend — trois états au lieu de cinq — est identique.
    notice.append(
        document.createTextNode("without a dedicated adapter, ash watches the process, not its hooks."),
        document.createElement("br"),
        document.createTextNode(`${subject} will show as `),
        stateWord("idle"),
        document.createTextNode(" · "),
        stateWord("done"),
        document.createTextNode(" · "),
        stateWord("error"),
        document.createTextNode(" — never "),
        stateWord("waiting"),
        document.createTextNode(". no “waiting for a reply” notification for this tool."),
    );
    return notice;
}

function stateWord(state: "idle" | "done" | "error" | "waiting"): HTMLElement {
    return text("span", state, `ash-state-word is-${state}`);
}

/**
 * Les trois sections qui n'ont pas encore de contenu.
 *
 * Elles existent parce que la **navigation** les traverse, et elles disent où la chose
 * vit aujourd'hui plutôt que de laisser un panneau muet. Rien n'y est inventé : chaque
 * phrase décrit l'état réel du produit.
 */
function placeholderSection(section: Exclude<SettingsSection, "tools">): Node[] {
    const explanations: Record<Exclude<SettingsSection, "tools">, string> = {
        shortcuts:
            "the shortcuts are declared in the native menu — Terminal and View list them with their keys. changing them here comes later.",
        appearance:
            "the theme is chosen in View ▸ Theme: light, dark, or the one macOS is in. font, size and density come later.",
        notifications:
            "nothing is notified yet: waiting, done and error need the hooks, and the hooks need a verified tool.",
    };

    const body = document.createElement("div");
    body.className = "settings-body is-empty";
    body.append(text("p", explanations[section], "settings-empty-prose"));
    return [header(section, null, []), body];
}

function button(label: string, variant = ""): HTMLButtonElement {
    const element = document.createElement("button");
    element.type = "button";
    element.className = `settings-button ${variant}`.trim();
    element.textContent = label;
    return element;
}

function navHint(): HTMLElement {
    return text("p", "tab / ⌥↑↓ to move", "settings-nav-hint");
}

/** Le `flex: 1` qui pousse ce qui suit à droite. */
function spacer(): HTMLElement {
    const gap = document.createElement("span");
    gap.className = "settings-spacer";
    return gap;
}

function text(tag: string, content: string, className: string): HTMLElement {
    const element = document.createElement(tag);
    element.className = className;
    element.textContent = content;
    return element;
}
