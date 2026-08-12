import type {
    FixAction,
    HooksReport,
    SettingsSnapshot,
    TestDescription,
    ToolDeclaration,
    ToolDraft,
    Verification,
} from "./contract";
import {
    type AddAction,
    countProblems,
    degradedFixSubject,
    degradedModeSubject,
    describeAddAction,
    describeDuplicates,
    describeReset,
    describeStop,
    describeTool,
    describeToolCount,
    hookActionLabel,
    NOTHING_VERIFIED_YET,
    parseDiff,
    type ToolHeading,
} from "./model";
import { SETTINGS_SECTIONS, type SettingsSection } from "./sections";
import {
    hooksGlyph,
    presentHooks,
    presentVerification,
    testTileClass,
    testTileLabel,
    verificationGlyph,
} from "./verification-state";

/**
 * Le rendu de la fenêtre de réglages. Il ne décide rien : il reçoit ce que le backend
 * détient et ce que [`model`](./model.ts) en a conclu, et le pose dans le DOM.
 *
 * Le DOM est reconstruit à chaque rendu, comme la sidebar et la barre d'onglets : quelques
 * dizaines de nœuds, contre le risque d'une liste qui diverge de celle du backend. Le
 * champ qui a le focus et la position du curseur sont **rendus** après coup — sans ça, la
 * relance de la vérification arracherait le curseur des mains de celui qui tape.
 *
 * **Rien n'est décidé dans ce fichier, et c'est une règle du dépôt** : `bun test` n'y monte
 * pas de DOM, donc tout ce qui s'y glisse échappe aux tests. Les cinq états de la ligne
 * `hooks`, la précédence de leurs raisons et le droit d'écrire viennent du backend ; le
 * découpage du diff, la bannière de doublon et ce que le `↺` peut faire viennent de
 * [`model`](./model.ts). Ce fichier pose des nœuds et des chaînes littérales.
 *
 * **Ce qu'il laisse volontairement vide** : l'encart de découverte de l'état vide (« ash
 * found these commands in your PATH ») et le bouton `Browse…` attendent que quelque chose
 * sache lire le `PATH` et ouvrir le Finder — inventer des candidats serait afficher les
 * données d'exemple de la maquette.
 */
export interface SettingsViewActions {
    selectSection(section: SettingsSection): void;
    startAdding(): void;
    cancelAdding(): void;
    editDraft(patch: Partial<ToolDraft>): void;
    submitDraft(): void;
    forgetTool(command: string): void;
    /** Une frappe dans le champ de chemin d'une carte — relance différée. */
    typePath(command: string, value: string): void;
    /** Un geste qui ne sera suivi d'aucune frappe : `⏎`, une perte de focus. */
    commitPath(command: string): void;
    /** Le menu d'adaptateur d'une carte — relance immédiate. */
    selectAdapter(command: string, adapter: string): void;
    /** Le bouton `re-verify` d'une carte. */
    verifyTool(command: string): void;
    /** Le bouton `re-verify all` de l'en-tête. */
    verifyAll(): void;
    /** Le bouton `apply` d'une correction proposée. */
    applyFix(command: string, fix: FixAction): void;
    /** Le `↺` d'une carte — retour au dernier dossier valide (spec §9.1). */
    resetTool(command: string): void;
    /** Le `undo the reset` de la bannière, et le `restore` de la ligne `was`. */
    undoReset(command: string): void;
    /** Le bouton de la ligne `hooks` : poser le bloc, ou le mettre à jour. */
    installHooks(command: string): void;
    /** Le `remove` de l'état `installed`. */
    removeHooks(command: string): void;
    /** `see the diff` — **n'écrit rien**, ouvre l'écran de conflit. */
    openConflict(command: string): void;
    /** `← back to the list`. */
    closeConflict(): void;
}

/** Tout ce qu'il faut pour dessiner la fenêtre à un instant donné. */
export interface SettingsScene {
    section: SettingsSection;
    snapshot: SettingsSnapshot;
    /** La saisie en cours, ou `null` quand on n'ajoute pas. */
    draft: ToolDraft | null;
    /** Ce que les quatre tests disent de cette saisie, ou `null` s'ils n'ont pas répondu. */
    draftVerification: Verification | null;
    /** Le dernier refus du backend, s'il en a opposé un. */
    failure: string | null;
    /**
     * Ce qui est tapé dans les champs d'une carte, tant que le backend n'a pas répondu.
     *
     * Ce n'est **pas** un état d'outil vivant à côté du backend : c'est le contenu d'un
     * champ de saisie, entre la frappe et la vérification qu'elle déclenche. La liste, elle,
     * reste celle de `snapshot` ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    edits: ReadonlyMap<string, string>;
    /**
     * L'entrée dont on regarde le conflit, ou `null`.
     *
     * L'écran de conflit **remplace la liste** (§4.4) : ce n'est ni une modale ni un
     * panneau. Rien n'y est écrit — c'est le refus lui-même qu'on affiche.
     */
    conflict: string | null;
}

/** La clé de focus de la saisie en cours — voir [`SettingsView.render`]. */
const FOCUS_KEY = "data-focus-key";

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
        const focused = this.rememberFocus();
        this.nav.replaceChildren(...this.navRows(scene), navHint());
        this.panel.replaceChildren(...this.panelRows(scene));
        this.restoreFocus(focused);
    }

    /** Donne le focus à la section active — le pendant clavier d'un clic sur sa ligne. */
    focusActiveSection(): void {
        this.nav.querySelector<HTMLElement>(".settings-nav-row.is-active")?.focus();
    }

    /**
     * Le champ qui a le focus et la position du curseur, avant que le DOM ne soit refait.
     *
     * Sans ça, la relance à 400 ms redessinerait la carte au milieu d'un mot et le curseur
     * partirait — c'est-à-dire que le mécanisme censé rendre le champ vivant le rendrait
     * inutilisable.
     */
    private rememberFocus(): { key: string; caret: number | null } | null {
        const active = document.activeElement;
        if (!(active instanceof HTMLInputElement)) return null;
        const key = active.getAttribute(FOCUS_KEY);
        return key === null ? null : { key, caret: active.selectionStart };
    }

    private restoreFocus(focused: { key: string; caret: number | null } | null): void {
        if (focused === null) return;
        const field = this.element.querySelector<HTMLInputElement>(
            `input[${FOCUS_KEY}="${focused.key}"]`,
        );
        if (field === null) return;
        field.focus();
        if (focused.caret !== null) field.setSelectionRange(focused.caret, focused.caret);
    }

    private navRows(scene: SettingsScene): HTMLElement[] {
        // Le compteur de problèmes de la colonne : il n'apparaît que si la section en a un,
        // et il compte comme celui de l'en-tête — la règle est dans `model`, pas ici.
        const invalid = countProblems(scene.snapshot.tools);

        return SETTINGS_SECTIONS.map((section) => {
            // Un vrai bouton, et pas une `div` cliquable : c'est ce qui met la section sur
            // le chemin de `tab` et dans l'arbre d'accessibilité sans une ligne de code.
            const row = document.createElement("button");
            row.type = "button";
            row.className = "settings-nav-row";
            row.append(text("span", section, "settings-nav-name"));
            if (section === "tools" && invalid > 0) {
                row.append(spacer(), text("span", String(invalid), "settings-nav-count"));
            }
            row.setAttribute("aria-current", section === scene.section ? "true" : "false");
            if (section === scene.section) row.classList.add("is-active");
            row.addEventListener("click", () => {
                this.actions.selectSection(section);
            });
            return row;
        });
    }

    private panelRows(scene: SettingsScene): Node[] {
        if (scene.section !== "tools") return placeholderSection(scene.section);
        if (scene.draft !== null) return this.addForm(scene, scene.draft);
        const conflicting = scene.snapshot.tools.find((tool) => tool.command === scene.conflict);
        if (conflicting !== undefined) return this.conflictScreen(conflicting);
        return this.toolsSection(scene);
    }

    /** La section `tools` : son en-tête, sa liste — ou son état vide — et son pied. */
    private toolsSection(scene: SettingsScene): Node[] {
        const tools = scene.snapshot.tools;
        const add = button("add", "is-primary");
        add.addEventListener("click", () => {
            this.actions.startAdding();
        });

        const actions: HTMLElement[] = [];
        if (tools.length > 0) {
            const all = button("re-verify all");
            all.addEventListener("click", () => {
                this.actions.verifyAll();
            });
            actions.push(all);
        }
        actions.push(add);

        const body = document.createElement("div");
        body.className = "settings-body";
        if (tools.length === 0) {
            body.classList.add("is-empty");
            body.append(emptyState());
        } else {
            body.append(...tools.map((tool) => this.toolCard(tool, scene)));
        }

        return [
            header("tools", describeToolCount(tools), actions),
            scaleNote(scene.snapshot.tests),
            ...this.duplicateBanner(tools),
            body,
            foot(
                tools.length === 0
                    ? "ash writes to no file until you declare a tool and install its hooks."
                    : "ash writes to no file until an entry is verified.",
            ),
        ];
    }

    /**
     * La bannière de doublon (§3.7) — **entre l'en-tête et la liste**, parce qu'elle ne
     * décrit aucune des deux cartes en particulier.
     */
    private duplicateBanner(tools: readonly ToolDeclaration[]): Node[] {
        const banner = describeDuplicates(tools);
        if (banner === null) return [];

        const line = document.createElement("div");
        line.className = "settings-banner is-warning";
        line.append(
            hooksGlyph("outdated", 12),
            text("span", banner.sentence, "settings-banner-text"),
        );
        if (banner.undo !== null) {
            const undo = button("undo the reset", "is-small is-nowrap");
            const command = banner.undo;
            undo.addEventListener("click", () => {
                this.actions.undoReset(command);
            });
            line.append(spacer(), undo);
        }
        return [line];
    }

    private toolCard(tool: ToolDeclaration, scene: SettingsScene): HTMLElement {
        const shown = describeTool(tool);
        const state = presentVerification(tool.verification.state);
        const card = document.createElement("article");
        // Deux teintes possibles, et le doublon l'emporte : une entrée valide qu'une autre
        // double n'écrira rien, et c'est ça qu'il faut voir en premier.
        const duplicated = tool.duplicates.length > 0 ? "is-duplicate" : "";
        card.className = `settings-card ${duplicated || state.cardClassName}`.trim();

        const head = document.createElement("div");
        head.className = "settings-card-head";
        head.append(text("span", shown.name, "settings-card-name"));
        if (shown.badge !== null) head.append(text("span", shown.badge, "settings-card-badge"));
        head.append(
            this.adapterMenu(tool, scene.snapshot.adapters),
            spacer(),
            this.verifyButton(tool),
            this.resetButton(tool),
            this.deleteButton(tool.command),
        );

        // La grille `44px 1fr` de la maquette. La ligne `hooks` (#16) s'y ajoutera telle
        // quelle ; les libellés portent des interlignes en pixels précisément pour rester
        // alignés sur des contrôles de hauteurs différentes.
        const body = document.createElement("div");
        body.className = "settings-card-body";
        body.append(
            text("span", "config", "settings-card-key"),
            this.pathField(tool, shown, scene.edits),
            ...this.wasRow(tool),
            text("span", "test", "settings-card-key is-test"),
            this.testLine(tool.verification, scene.snapshot.tests),
            ...this.testDetail(tool),
            text("span", "hooks", "settings-card-key is-hooks"),
            this.hooksLine(tool),
            document.createElement("span"),
            hooksNote(tool.hooks),
        );

        card.append(head, body);
        return card;
    }

    /**
     * Le menu d'adaptateur — **modifiable**, parce que le changer relance la séquence.
     *
     * #14 l'avait laissé en lecture : un contrôle qu'on peut bouger sans que rien ne re-juge
     * dirait qu'Ash a accepté le nouvel adaptateur. C'est la vérification qui le rend
     * honnête, et un changement de menu ne peut pas être suivi d'une frappe — il relance
     * donc tout de suite, sans les 400 ms.
     */
    private adapterMenu(tool: ToolDeclaration, adapters: readonly string[]): HTMLElement {
        const select = document.createElement("select");
        select.className = "settings-card-adapter";
        select.setAttribute("aria-label", `adapter for ${tool.command}`);
        for (const adapter of adapters) {
            const option = document.createElement("option");
            option.value = adapter;
            option.textContent = adapter;
            option.selected = adapter === tool.adapter;
            select.append(option);
        }
        select.addEventListener("change", () => {
            this.actions.selectAdapter(tool.command, select.value);
        });
        return select;
    }

    private verifyButton(tool: ToolDeclaration): HTMLElement {
        const label = presentVerification(tool.verification.state).action;
        const verify = button(label, "is-small");
        // `cancel` n'annule rien tant que rien n'est annulable : la commande du test 4 est
        // déjà partie, et prétendre l'arrêter serait mentir. Elle relance, comme les autres.
        verify.addEventListener("click", () => {
            this.actions.verifyTool(tool.command);
        });
        return verify;
    }

    /**
     * Le `↺` : retour au **dernier dossier valide de cette entrée** (spec §9.1).
     *
     * Il reste visible même quand il ne peut rien faire, avec sa raison en infobulle — la
     * même règle que celle du bouton d'installation, et pour la même raison : le masquer
     * ferait croire que le geste n'existe pas.
     */
    private resetButton(tool: ToolDeclaration): HTMLElement {
        const reset = describeReset(tool);
        const button_ = document.createElement("button");
        button_.type = "button";
        button_.className = `settings-icon-button ${tool.resetFrom === null ? "" : "is-warning"}`.trim();
        button_.textContent = "↺";
        button_.disabled = !reset.enabled;
        button_.title = tool.resetFrom === null ? reset.reason : "reset just now";
        button_.setAttribute("aria-label", `reset ${tool.command}: ${reset.reason}`);
        button_.addEventListener("click", () => {
            this.actions.resetTool(tool.command);
        });
        return button_;
    }

    /**
     * La ligne `was` — elle n'existe **que** juste après une réinitialisation.
     *
     * À ne pas confondre avec l'étiquette de doublon, qui existe dès que deux entrées
     * collisionnent : ce sont deux conditions indépendantes (§7.3).
     */
    private wasRow(tool: ToolDeclaration): Node[] {
        if (tool.resetFrom === null) return [];

        const line = document.createElement("div");
        line.className = "settings-was";
        line.append(text("span", tool.resetFrom, "settings-was-path"));

        const restore = text("button", "restore", "settings-link");
        restore.addEventListener("click", () => {
            this.actions.undoReset(tool.command);
        });
        line.append(restore);
        return [text("span", "was", "settings-card-key"), line];
    }

    /**
     * La ligne `hooks`, dans **les cinq états** que le backend distingue.
     *
     * Rien n'est décidé ici : l'état, la phrase, le fichier, l'action et le fait que le
     * bouton soit allumé viennent tous de `tool.hooks`, calculé en Rust — c'est la règle qui
     * autorise Ash à écrire chez l'utilisateur, et elle n'a qu'un propriétaire
     * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)).
     */
    private hooksLine(tool: ToolDeclaration): HTMLElement {
        const { hooks } = tool;
        const shown = presentHooks(hooks.state);
        const line = document.createElement("div");
        line.className = `settings-hooks ${shown.rowClassName}`.trim();
        line.append(
            hooksGlyph(hooks.state),
            text("span", hooks.summary, "settings-hooks-reason"),
        );
        if (hooks.file !== null && shown.showsFile) {
            line.append(text("span", hooks.file, "settings-hooks-file"));
        }

        // Un seul bouton, toujours présent, éteint quand il ne peut rien faire.
        const action = button(hookActionLabel(hooks.action), hooks.action === "remove" ? "" : "is-primary");
        action.disabled = !hooks.enabled;
        action.addEventListener("click", () => {
            if (hooks.action === "remove") this.actions.removeHooks(tool.command);
            else if (hooks.action === "seeTheDiff") this.actions.openConflict(tool.command);
            else this.actions.installHooks(tool.command);
        });
        line.append(spacer(), action);
        return line;
    }

    /**
     * L'écran de conflit (§4.4) — il **remplace la liste**, et n'écrit rien.
     *
     * C'est le refus lui-même : la spec §10 ne demande pas seulement de refuser, elle
     * demande de signaler, de proposer le diff, et de demander. Ash ne propose donc ici ni
     * `replace`, ni `merge` : l'un écraserait les lignes de l'utilisateur, l'autre
     * demanderait d'écrire hors des marqueurs — ce que toute la feature `hooks` interdit.
     * Le seul geste qui reste est de sortir, et de décider dans son éditeur.
     */
    private conflictScreen(tool: ToolDeclaration): Node[] {
        const back = button("← back to the list");
        back.addEventListener("click", () => {
            this.actions.closeConflict();
        });

        const body = document.createElement("div");
        body.className = "settings-body is-conflict";

        const banner = document.createElement("div");
        banner.className = "settings-banner is-error";
        banner.append(
            hooksGlyph("conflict", 12),
            text(
                "span",
                "the ash block in this file was edited by hand — ash writes nothing until this is settled",
                "settings-banner-text",
            ),
        );

        const where = document.createElement("p");
        where.className = "settings-locate";
        where.append(text("span", tool.hooks.file ?? "", "settings-locate-path"));

        body.append(banner, where, diffPanel(tool.hooks.diff ?? ""), hooksNote(tool.hooks));
        return [header("tools", `${tool.command} · ${tool.hooks.file ?? ""}`, [back]), body];
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
     * Le chemin de configuration, **modifiable** depuis cette issue.
     *
     * Il ne l'était pas : le changer sans que rien ne le re-juge aurait dit qu'Ash a accepté
     * le nouveau chemin. Il l'est maintenant parce que chaque frappe arme la relance de la
     * séquence, 400 ms plus tard — ou tout de suite sur `⏎`.
     */
    private pathField(
        tool: ToolDeclaration,
        shown: ToolHeading,
        edits: ReadonlyMap<string, string>,
    ): HTMLElement {
        const invalid = tool.verification.state === "invalid";
        const duplicated = tool.duplicates.length > 0;
        const field = document.createElement("div");
        // Le champ lui-même prend la teinte : c'est **le dossier** qui est en cause, dans
        // les deux cas, et c'est lui qu'on va corriger.
        const marking = duplicated ? "is-duplicate" : invalid ? "is-invalid" : "";
        field.className = `settings-field ${marking}`.trim();

        const input = document.createElement("input");
        input.type = "text";
        input.className = "settings-path";
        input.value = edits.get(tool.command) ?? shown.path;
        // La chaîne affichée quand rien n'est saisi n'est pas un chemin : c'est ce que
        // l'absence veut dire. La mettre dans la valeur en ferait un dossier nommé
        // « adapter default ».
        input.placeholder = "adapter default";
        input.setAttribute("aria-label", `configuration folder for ${tool.command}`);
        input.setAttribute(FOCUS_KEY, `path:${tool.command}`);
        input.addEventListener("input", () => {
            this.actions.typePath(tool.command, input.value);
        });
        input.addEventListener("keydown", (event) => {
            if (event.key !== "Enter") return;
            // `⏎` ne valide rien à la place de l'utilisateur (ADR-0015) : il dit seulement
            // « j'ai fini de taper », et abrège l'attente des 400 ms.
            event.preventDefault();
            this.actions.commitPath(tool.command);
        });
        input.addEventListener("blur", () => {
            this.actions.commitPath(tool.command);
        });
        field.append(input);

        if (duplicated) {
            // L'étiquette est sur **les deux** cartes, pas seulement sur celle qu'on vient
            // de toucher (spec §9.1) : c'est le registre qui l'a posée sur chacune.
            field.append(
                text(
                    "span",
                    `duplicate · also ${tool.duplicates.join(", ")}`,
                    "settings-duplicate-tag",
                ),
            );
        }

        if (!tool.verified) {
            // La pastille « modifié, non enregistré » de la maquette. Tant qu'une entrée n'a
            // pas prouvé son dossier, elle vit en mémoire et n'est **pas** dans
            // `~/.ash/config.toml`.
            const dot = document.createElement("span");
            dot.className = "settings-unsaved";
            dot.setAttribute("aria-label", "not verified — nothing written to config.toml");
            dot.title = "not verified — nothing written to config.toml";
            field.append(dot);
        }
        return field;
    }

    /** La ligne `test` : le glyphe, la phrase, les quatre pastilles, et où ça s'est arrêté. */
    private testLine(
        verification: Verification,
        tests: readonly TestDescription[],
    ): HTMLElement {
        const line = document.createElement("div");
        line.className = "settings-test";
        line.append(
            verificationGlyph(verification.state, 13),
            text("span", verification.summary, "settings-test-summary"),
            spacer(),
            tileRow(verification, tests),
        );
        const stop = describeStop(verification);
        if (stop !== null) line.append(text("span", stop, "settings-stopped"));
        return line;
    }

    /**
     * Ce qu'un état ajoute sous la ligne `test` — des lignes de grille à **cellule de
     * libellé vide**, donc alignées sous elle.
     */
    private testDetail(tool: ToolDeclaration): Node[] {
        const { verification } = tool;
        const rows: Node[] = [];

        if (verification.launched !== null) {
            // La commande réellement lancée. Ce qui part sans qu'on l'ait tapé doit être
            // lisible : c'est la contrepartie du fait qu'Ash lance un programme tout seul.
            rows.push(document.createElement("span"), inset(verification.launched, "is-command"));
        }

        if (verification.detail !== null) {
            const recall = document.createElement("p");
            recall.className = "settings-recall";
            recall.append(
                document.createTextNode("expected: "),
                text("span", verification.detail.expected, "settings-recall-expected"),
                document.createTextNode(` — found: ${verification.detail.found}`),
            );
            rows.push(document.createElement("span"), recall);
        }

        if (verification.fix !== null) {
            rows.push(document.createElement("span"), this.fixInset(tool));
            // `generic` est un mode dégradé, et l'écran le dit **avant** qu'on l'applique :
            // le bouton `apply` juste au-dessus est celui qui y bascule.
            const degrading = degradedFixSubject(tool);
            if (degrading !== null) rows.push(...degradedRow(degrading));
        }

        return rows;
    }

    /** La correction proposée : la question, et ce qu'on peut en faire. */
    private fixInset(tool: ToolDeclaration): HTMLElement {
        const fix = tool.verification.fix;
        const box = document.createElement("div");
        box.className = "settings-fix";
        box.append(text("span", fix?.question ?? "", "settings-fix-question"), spacer());

        const apply = fix?.apply ?? null;
        if (apply !== null) {
            const button_ = button("apply", "is-primary is-small");
            button_.addEventListener("click", () => {
                this.actions.applyFix(tool.command, apply);
            });
            box.append(button_);
        }

        // Toujours là, et secondaire : quand rien ne peut être appliqué, c'est la seule
        // chose qui reste à faire — et elle ne se fait pas à la place de l'utilisateur.
        const choose = button("choose another folder…", "is-small");
        choose.addEventListener("click", () => {
            this.element
                .querySelector<HTMLInputElement>(`input[${FOCUS_KEY}="path:${tool.command}"]`)
                ?.focus();
        });
        box.append(choose);
        return box;
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
            text("span", "test", "settings-form-key"),
            this.draftTestLine(scene),
        );

        body.append(
            grid,
            this.formActions(
                describeAddAction(
                    draft,
                    scene.snapshot.tools,
                    scene.failure,
                    scene.draftVerification,
                ),
            ),
        );

        return [header("new tool", null, [escape]), body];
    }

    /** La ligne `test` du formulaire : la même rangée que dans une carte. */
    private draftTestLine(scene: SettingsScene): HTMLElement {
        return this.testLine(scene.draftVerification ?? NOTHING_VERIFIED_YET, scene.snapshot.tests);
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
        input.setAttribute(FOCUS_KEY, `draft:${name}`);
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
        if (subject !== null) rows.push(...degradedRow(subject));
        return rows;
    }

    /** La barre d'action, poussée en bas : la raison à gauche, les boutons à droite. */
    private formActions(action: AddAction): HTMLElement {
        const cancel = button("cancel");
        cancel.addEventListener("click", () => {
            this.actions.cancelAdding();
        });

        const add = button("add", "is-primary");
        // Éteint, jamais masqué : « le masquer ferait croire que ça n'existe pas ». La
        // raison reste lisible à gauche.
        add.disabled = !action.enabled;
        add.addEventListener("click", () => {
            this.actions.submitDraft();
        });

        const bar = document.createElement("div");
        bar.className = "settings-form-actions";
        bar.append(text("span", action.reason, "settings-gloss"), spacer(), cancel, add);
        return bar;
    }
}

/** L'avertissement du mode dégradé, posé sous ce qu'il commente. */
function degradedRow(subject: string): Node[] {
    return [document.createElement("span"), degradedNotice(subject)];
}

/**
 * La prose sous la ligne `hooks` : **la conséquence**, et la copie annoncée avant l'action.
 *
 * La phrase vient du backend, qui seul sait dans quel état est le fichier. Ce que la vue
 * ajoute est la promesse de sauvegarde, écrite **avant** le geste et pas après (§4.2).
 */
function hooksNote(hooks: HooksReport): HTMLElement {
    const note = document.createElement("p");
    note.className = "settings-hooks-note";
    note.append(document.createTextNode(hooks.note));
    if (hooks.backup !== null) {
        note.append(
            document.createElement("br"),
            document.createTextNode(`before writing: ${hooks.backup}`),
        );
    }
    return note;
}

/**
 * Le diff d'un conflit — les lignes qui divergent, et ce qui n'est pas touché.
 *
 * La légende dit le sens **du diff qu'on affiche** : `−` ce qu'Ash écrirait, `+` ce que le
 * fichier porte. C'est celui du backend, et l'annoncer autrement ferait lire chaque ligne à
 * l'envers.
 */
function diffPanel(diff: string): HTMLElement {
    const panel = document.createElement("div");
    panel.className = "settings-diff";

    const head = document.createElement("div");
    head.className = "settings-diff-head";
    head.append(
        text("span", "− the ash block", "settings-diff-legend is-removed"),
        text("span", "+ this file", "settings-diff-legend is-added"),
    );

    const body = document.createElement("pre");
    body.className = "settings-diff-body";
    for (const line of parseDiff(diff)) {
        const sign = line.kind === "removed" ? "−" : line.kind === "added" ? "+" : " ";
        body.append(text("span", `${sign} ${line.text}`, `settings-diff-line is-${line.kind}`));
    }

    panel.append(
        head,
        body,
        text(
            "p",
            "outside the ash block the file is untouched — ash changes nothing between its markers either, until this is settled.",
            "settings-diff-foot",
        ),
    );
    return panel;
}

/** Les quatre pastilles, dans l'ordre où les tests se lancent. */
function tileRow(
    verification: Verification,
    tests: readonly TestDescription[],
): HTMLElement {
    const row = document.createElement("div");
    row.className = "settings-tiles";
    tests.forEach((test, index) => {
        const outcome = verification.tests[index] ?? "pending";
        const tile = text("span", String(test.number), testTileClass(outcome));
        tile.title = testTileLabel(outcome, test);
        tile.setAttribute("aria-label", tile.title);
        row.append(tile);
    });
    return row;
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

/**
 * La note de barème, sous l'en-tête de `tools` et nulle part ailleurs.
 *
 * Les quatre libellés viennent du **contrat** : les tests existent en Rust, donc c'est là
 * qu'ils se nomment. Recopiés ici, ils finiraient par décrire un test que la séquence ne
 * lance plus.
 */
function scaleNote(tests: readonly TestDescription[]): HTMLElement {
    const note = document.createElement("p");
    note.className = "settings-note";
    note.append(
        document.createTextNode(
            "one command = one tool. ash re-runs the tests on every path or adapter change.",
        ),
        document.createElement("br"),
        document.createTextNode("tests · "),
    );
    tests.forEach((test, index) => {
        if (index > 0) note.append(document.createTextNode(" · "));
        note.append(text("span", String(test.number), "settings-note-index"));
        note.append(document.createTextNode(` ${test.shortLabel}`));
    });
    return note;
}

function foot(sentence: string): HTMLElement {
    const bar = document.createElement("footer");
    bar.className = "settings-foot";
    bar.append(text("span", sentence, "settings-foot-text"));
    return bar;
}

/** Un encart dans une carte — la commande lancée, un rappel. */
function inset(content: string, variant = ""): HTMLElement {
    return text("p", content, `settings-inset ${variant}`.trim());
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
        document.createTextNode(
            "without a dedicated adapter, ash watches the process, not its hooks.",
        ),
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
