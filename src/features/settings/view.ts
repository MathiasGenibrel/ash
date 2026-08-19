import type { AgentState } from "@/shared/ipc";
import { button, FOCUS_KEY, paint, toNode, type UiChild } from "@/shared/ui";

import type {
    Appearance,
    FixAction,
    FontStep,
    NotificationsReport,
    SettingsSnapshot,
    Shortcut,
    ThemeMode,
    ToolDraft,
    Verification,
} from "./contract";
import { describeToolCount } from "./model";
import { type SettingsSection } from "./sections";
import {
    addForm,
    appearanceSection,
    conflictScreen,
    duplicateBanner,
    foot,
    navColumn,
    noToolsYet,
    notificationsSection,
    pathFocusKey,
    scaleNote,
    sectionHeader,
    shortcutsSection,
    tag,
    toolCard,
} from "./components";

/**
 * La fenêtre de réglages : **un assemblage au-dessus, une classe mince en dessous**.
 *
 * C'est le motif du dépôt (`terminal/status-line.ts`, `sidebar/view.ts`,
 * `sidebar/tree.ts`) : on compose un modèle, puis on le peint. Ce fichier l'avait rompu —
 * 986 lignes, 79 `document`, aucune fonction pure — et trois passes architecturales
 * d'affilée y ont trouvé une règle produit cachée, toujours la même famille : **la vue qui
 * supprime une information que le backend envoie**. Rien ne pouvait les attraper, parce que
 * `bun test` ne monte pas de DOM.
 *
 * Depuis cette refonte, l'assemblage rend des [descriptions](../../shared/ui/node.ts) et se
 * lit dans un test ; la classe, elle, ne fait que trois choses que seul le DOM sait faire :
 * poser les nœuds, retenir le champ actif, et lui rendre son curseur.
 *
 * **Aucune règle ne vit ici.** Les cinq états de la ligne `hooks`, la précédence de leurs
 * raisons et le droit d'écrire viennent du backend ; le compte des entrées invalides, le
 * découpage du diff, la bannière de doublon, ce que le `↺` peut faire, `stopped at test n`
 * et l'attente des quatre tests viennent de [`model`](./model.ts) ; le fichier montré ou
 * tu par la ligne `hooks` vient de [`verification-state`](./verification-state.ts).
 *
 * **Ce qu'elle laisse volontairement vide** : l'encart de découverte de l'état vide (« ash
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
    /** `see the diff` — **n'écrit rien**, ouvre le diff de ce qu'Ash écrirait. */
    openConflict(command: string): void;
    /** `← back to the list`. */
    closeConflict(): void;
    /**
     * Un thème choisi dans la section `appearance` — la **seconde surface** du choix.
     *
     * Elle ne pose aucune palette : `features::theme` retient le mode et l'annonce, et la
     * fenêtre le rend quand l'annonce revient
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    chooseTheme(mode: ThemeMode): void;
    /** Un pas de taille de police, jamais une taille : les bornes sont en Rust. */
    stepFontSize(step: FontStep): void;
    /**
     * L'un des trois interrupteurs de la section `notifications` (spec §9).
     *
     * Elle ne coupe rien : `features::agents` retient le choix, et c'est lui qui le consulte
     * au moment de poster une bannière. La fenêtre redessine la section que le backend lui
     * répond ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    setNotification(state: AgentState, enabled: boolean): void;
}

/**
 * Ce que l'assemblage sait demander : les gestes de l'utilisateur, plus **le seul geste de
 * rendu** qu'une description ne peut pas décrire.
 *
 * Ramener le curseur dans un champ (`choose another folder…`) demande de retrouver un
 * élément monté : c'est la classe qui monte qui le fait, comme elle rend le focus après un
 * rendu. Le composant, lui, dit ce qu'il veut, pas comment.
 */
export interface SettingsRendering extends SettingsViewActions {
    focusPath(command: string): void;
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
     * L'écran du diff **remplace la liste** (§4.4) : ce n'est ni une modale ni un
     * panneau. Rien ne s'y écrit tant que l'utilisateur n'a pas tranché.
     */
    conflict: string | null;
    /**
     * La section `notifications` telle que le backend la compose, ou `null` tant qu'il n'a
     * pas répondu (spec §8).
     *
     * Elle n'est **pas** dans `snapshot` : la liste des outils et l'autorisation macOS ne
     * changent ni au même moment ni pour la même raison, et les faire voyager ensemble
     * obligerait chaque ajout d'entrée à redemander une permission au système.
     */
    notifications: NotificationsReport | null;
    /**
     * Le thème et la taille de police que le backend détient, ou `null` tant qu'il n'a pas
     * répondu (spec §9).
     *
     * Ce n'est **pas** l'état de l'écran : c'est celui de `features::theme`, relu ici pour
     * être rendu. Un clic sur `dark` ne le change pas — il part au backend, qui l'annonce à
     * toutes les fenêtres, et c'est cette annonce qui repose la scène. Sans quoi la coche du
     * menu Vue et cet écran finiraient par ne plus dire la même chose
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    appearance: Appearance | null;
    /**
     * Les raccourcis que le menu natif déclare, ou `null` tant qu'il ne les a pas rendus.
     *
     * Demandés une fois : le menu est construit au démarrage et ne change plus.
     */
    shortcuts: readonly Shortcut[] | null;
}

/** La colonne de gauche. */
export function settingsNav(
    scene: SettingsScene,
    actions: SettingsRendering,
): readonly UiChild[] {
    return navColumn(scene.section, scene.snapshot.tools, (section) => {
        actions.selectSection(section);
    });
}

/**
 * Le panneau de droite — l'un des quatre écrans, jamais deux.
 *
 * Le `switch` couvre `SettingsSection` **en entier**, et il n'a pas de `default` : c'est ce
 * qui fait échouer `bun run typecheck` le jour où une cinquième section s'ajoute, à l'endroit
 * exact où son écran manque. Une chaîne de `if` avec `tools` en dernier recours l'aurait
 * silencieusement affichée sous le titre d'une autre section — c'est le filet que le
 * `Record<EmptySection, string>` des sections vides tendait avant qu'elles aient du contenu.
 */
export function settingsPanel(
    scene: SettingsScene,
    actions: SettingsRendering,
): readonly UiChild[] {
    switch (scene.section) {
        case "notifications":
            return notificationsSection(scene.notifications, {
                setNotification: (state, enabled) => {
                    actions.setNotification(state, enabled);
                },
            });
        case "shortcuts":
            return shortcutsSection(scene.shortcuts);
        case "appearance":
            return appearanceSection(scene.appearance, {
                chooseTheme: (mode) => {
                    actions.chooseTheme(mode);
                },
                stepFontSize: (step) => {
                    actions.stepFontSize(step);
                },
            });
        case "tools":
            return toolsPanel(scene, actions);
    }
}

/**
 * La section `tools` et les deux écrans qui la remplacent — le formulaire d'ajout, et la
 * carte en conflit.
 *
 * Ils sont ici et non dans le `switch` parce que ce ne sont pas des sections : la navigation
 * ne les atteint pas, et on y entre depuis `tools` pour y revenir.
 */
function toolsPanel(scene: SettingsScene, actions: SettingsRendering): readonly UiChild[] {
    if (scene.draft !== null) {
        return addForm(
            scene.draft,
            scene.snapshot,
            scene.draftVerification,
            scene.failure,
            actions,
        );
    }
    const conflicting = scene.snapshot.tools.find((tool) => tool.command === scene.conflict);
    if (conflicting !== undefined) {
        return conflictScreen(conflicting, actions, () => {
            actions.closeConflict();
        });
    }
    return toolsSection(scene, actions);
}

/** La section `tools` : son en-tête, sa liste — ou son état vide — et son pied. */
function toolsSection(scene: SettingsScene, actions: SettingsRendering): readonly UiChild[] {
    const tools = scene.snapshot.tools;
    const empty = tools.length === 0;

    const controls: UiChild[] = [];
    if (!empty) {
        controls.push(
            button("re-verify all")
                .class("settings-button")
                .onClick(() => {
                    actions.verifyAll();
                }),
        );
    }
    controls.push(
        button("add")
            .class("settings-button", "is-primary")
            .onClick(() => {
                actions.startAdding();
            }),
    );

    // Le corps ne défile pas quand il est vide — il se centre : il n'y a rien à parcourir.
    const body = tag("div", "settings-body", empty ? "is-empty" : "").add(
        ...(empty
            ? [noToolsYet()]
            : tools.map((tool) =>
                  toolCard(
                      tool,
                      {
                          adapters: scene.snapshot.adapters,
                          tests: scene.snapshot.tests,
                          edits: scene.edits,
                      },
                      actions,
                  ),
              )),
    );

    return [
        sectionHeader("tools", describeToolCount(tools), controls),
        scaleNote(scene.snapshot.tests),
        ...duplicateBanner(tools, actions),
        body,
        foot(
            empty
                ? "ash writes to no file until you declare a tool and install its hooks."
                : "ash writes to no file until an entry is verified.",
        ),
    ];
}

/** Ce que la classe retient d'un rendu à l'autre : un champ, et où en était le curseur. */
interface FocusedField {
    readonly key: string;
    readonly caret: number | null;
}

/**
 * Ce qui monte l'assemblage — et **rien d'autre**.
 *
 * Trois gestes que seul le DOM sait faire : poser les nœuds, retenir le champ actif avant
 * de les refaire, et lui rendre son curseur après. Le `mount(container, node)` générique a
 * été écarté du socle pour cette raison : il aurait besoin de `document.activeElement`,
 * donc d'un **second** fichier de `shared/ui/` qui touche le DOM — et c'est cette unicité
 * qui met tout le reste sous test. Il se généralisera à la troisième vue convertie, pas
 * avant.
 */
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
        const rendering: SettingsRendering = {
            ...this.actions,
            focusPath: (command) => {
                this.focusPath(command);
            },
        };
        this.nav.replaceChildren(...settingsNav(scene, rendering).map(painted));
        this.panel.replaceChildren(...settingsPanel(scene, rendering).map(painted));
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
    private rememberFocus(): FocusedField | null {
        const active = document.activeElement;
        if (!(active instanceof HTMLInputElement)) return null;
        const key = active.getAttribute(FOCUS_KEY);
        return key === null ? null : { key, caret: active.selectionStart };
    }

    private restoreFocus(focused: FocusedField | null): void {
        if (focused === null) return;
        const field = this.field(focused.key);
        if (field === null) return;
        field.focus();
        if (focused.caret !== null) field.setSelectionRange(focused.caret, focused.caret);
    }

    /** `choose another folder…` : le seul geste de la fenêtre qui déplace le focus. */
    private focusPath(command: string): void {
        this.field(pathFocusKey(command))?.focus();
    }

    private field(key: string): HTMLInputElement | null {
        return this.element.querySelector<HTMLInputElement>(`input[${FOCUS_KEY}="${key}"]`);
    }
}

function painted(child: UiChild): Node {
    return paint(toNode(child));
}
