import type { AgentState } from "@/shared/ipc";
import { button, FOCUS_KEY, paint, toNode, type UiChild } from "@/shared/ui";

import type {
    Appearance,
    FixAction,
    FontStep,
    JournalReport,
    NotificationsReport,
    UsageReport,
    ConflictChoice,
    SettingsSnapshot,
    ShortcutsReport,
    SidebarDensity,
    ThemeMode,
    ToolDraft,
    ToolSuggestion,
    Verification,
} from "./contract";
import { describeToolCount, emptyToolsProse, pendingSuggestions } from "./model";
import { type SettingsSection } from "./sections";
import {
    addForm,
    appearanceSection,
    conflictScreen,
    duplicateBanner,
    foot,
    journalRow,
    navColumn,
    noToolsYet,
    notificationsSection,
    usageSection,
    pathFocusKey,
    scaleNote,
    sectionHeader,
    shortcutsSection,
    suggestionList,
    tag,
    toolCard,
    uninstallRow,
    uninstallScreen,
    type RemovalStage,
    type ShortcutCapture,
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
    /**
     * Le `declare` d'un outil qu'Ash a vu tourner (ADR-0006).
     *
     * **Aucun hook n'est posé** : elle ajoute l'entrée, qui repart dans le flux qui existe
     * déjà — vérification en deux temps, puis bouton d'installation. Rien n'est écrit chez
     * l'utilisateur tant que ce bouton-là n'a pas été pressé
     * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)).
     */
    declareSuggestion(suggestion: ToolSuggestion): void;
    /** `← back to the list`. */
    closeConflict(): void;
    /**
     * `remove ash from every file` — **n'écrit rien** : elle demande l'annonce (spec §10).
     *
     * Le pendant exact de `see the diff` pour le geste inverse, et la même règle : ce qui
     * touche un fichier de l'utilisateur se lit avant d'être posé.
     */
    planRemoval(): void;
    /** Le clic pris devant l'annonce — celui qui écrit. */
    removeEverything(): void;
    /** `← cancel`, et `← back to the list` du compte rendu. */
    closeRemoval(): void;
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
     * Une famille choisie dans la liste que le backend a rendue.
     *
     * Une **valeur** et non un pas, contrairement à la taille : il n'existe pas de « police
     * suivante », et ce qui est proposé est ce que le système porte. Elle ne pose rien non
     * plus — `features::theme` retient et annonce.
     */
    chooseFont(family: string): void;
    /** La densité de la sidebar, même chemin exactement. */
    chooseDensity(density: SidebarDensity): void;
    /**
     * L'un des trois interrupteurs de la section `notifications` (spec §9).
     *
     * Elle ne coupe rien : `features::agents` retient le choix, et c'est lui qui le consulte
     * au moment de poster une bannière. La fenêtre redessine la section que le backend lui
     * répond ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    setNotification(state: AgentState, enabled: boolean): void;
    /**
     * L'interrupteur de la section `usage` — les appels sortants d'ADR-0016.
     *
     * Elle ne coupe rien ici : `features::usage` retient le choix, et c'est son portillon qui
     * le consulte au moment de sortir. La fenêtre redessine la section que le backend lui
     * répond ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — un
     * interrupteur qui basculerait tout seul afficherait `off` sur des appels qui
     * partiraient encore.
     */
    setUsagePolling(enabled: boolean): void;
    /**
     * Efface le journal d'attribution (spec §10, ADR-0014).
     *
     * Elle n'efface rien ici : elle demande, et le backend rend la fiche relue après coup.
     * C'est le seul geste de cet écran qui touche à ce qu'Ash a écrit **pour lui-même**, et
     * non à un fichier de l'utilisateur — d'où l'absence d'annonce préalable.
     */
    purgeJournal(): void;
    /**
     * Les six gestes de la section `shortcuts` (spec §4.4, issue #22).
     *
     * Aucun ne pose de combinaison : ils demandent, et le backend rend l'instantané entier.
     * `openCapture` et `cancelCapture` sont les deux seuls à ne rien demander du tout — ils
     * ouvrent et referment un bloc, ce qui est un fait d'affichage.
     */
    openCapture(action: string): void;
    cancelCapture(): void;
    /** `⏎` — pose ce qui a été frappé, ou ouvre le conflit que ça produirait. */
    confirmCapture(): void;
    /** `⌫` — la ligne n'a plus de raccourci, et garde son entrée de menu. */
    clearShortcut(action: string): void;
    /** L'icône de retour d'une ligne changée, et le `reset all` de l'en-tête. */
    resetShortcut(action: string): void;
    resetAllShortcuts(): void;
    /** L'une des deux issues nommées du bloc de conflit. */
    resolveConflict(choice: ConflictChoice): void;
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
    /**
     * Les outils qu'Ash a vus tourner et que personne n'a déclarés (ADR-0006).
     *
     * Ils ne sont **pas** dans `snapshot`, et c'est le backend qui le décide : l'instantané
     * traverse à chaque geste, et y glisser les suggestions ferait relire un fichier de
     * configuration à chaque frappe dans un champ de chemin. Ils arrivent donc par un second
     * aller-retour, comme `journal` et `usage` — et comme eux, ils sont rendus, jamais
     * détenus ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    suggestions: readonly ToolSuggestion[];
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
     * L'écran de désinstallation, ou `null` quand on ne désinstalle pas.
     *
     * Il **remplace la liste** comme celui du diff, et pour la même raison : ce qui va
     * toucher plusieurs fichiers de l'utilisateur se lit en entier. Ce qu'il porte vient du
     * backend — l'annonce, puis le compte rendu — et n'est jamais recomposé ici
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    removal: RemovalStage | null;
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
     * La section `usage` telle que le backend la compose, ou `null` tant qu'il n'a pas
     * répondu (ADR-0016, condition 3).
     *
     * Elle n'est pas dans `snapshot` pour la raison qui vaut pour `notifications`, et une de
     * plus qui lui est propre : la lisibilité du jeton change **pendant** qu'Ash tourne, au
     * premier appel du fil de fond, sans qu'aucun outil ait été déclaré.
     */
    usage: UsageReport | null;
    /**
     * Ce que le journal d'attribution pèse, ou `null` tant que le backend n'a pas répondu.
     *
     * Il n'est pas dans `snapshot` pour la raison qui vaut pour `notifications` : le journal
     * se remplit sans qu'aucun outil soit déclaré — l'attribution ne dépend que de la sonde
     * (ADR-0014) — et les faire voyager ensemble lierait deux choses qui ne changent ni au
     * même moment ni pour la même raison.
     */
    journal: JournalReport | null;
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
     * Les familles monospace installées, ou `null` tant que le backend ne les a pas lues.
     *
     * Séparées de [`appearance`] parce qu'elles ne changent pas au même rythme : l'apparence
     * revient à chaque annonce, la liste est lue une fois. Ce n'est pas un état de l'écran —
     * c'est ce que le système porte.
     */
    fonts: readonly string[] | null;
    /**
     * Les raccourcis en vigueur, ou `null` tant que le backend ne les a pas rendus.
     *
     * Ce n'est **pas** l'état de l'écran : c'est celui de `features::shortcuts`, dont le menu
     * natif dérive aussi. Une capture ne le modifie pas ici — elle part au backend, qui refait
     * le menu et renvoie l'instantané
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    shortcuts: ShortcutsReport | null;
    /**
     * La ligne ouverte en capture, ou `null`.
     *
     * Le seul état de la section qui vive ici, et ce n'est pas une liaison : c'est « quelle
     * ligne est ouverte, et ce que le backend a dit de la dernière frappe ».
     */
    capture: ShortcutCapture | null;
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
 * Le panneau de droite — l'un des cinq écrans, jamais deux.
 *
 * Le `switch` couvre `SettingsSection` **en entier**, et il n'a pas de `default` : c'est ce
 * qui fait échouer `bun run typecheck` le jour où une section s'ajoute, à l'endroit
 * exact où son écran manque. Une chaîne de `if` avec `tools` en dernier recours l'aurait
 * silencieusement affichée sous le titre d'une autre section — c'est le filet que le
 * `Record<EmptySection, string>` des sections vides tendait avant qu'elles aient du contenu.
 */
export function settingsPanel(
    scene: SettingsScene,
    actions: SettingsRendering,
): readonly UiChild[] {
    switch (scene.section) {
        case "usage":
            return usageSection(scene.usage, {
                setPolling: (enabled) => {
                    actions.setUsagePolling(enabled);
                },
            });
        case "notifications":
            return notificationsSection(scene.notifications, {
                setNotification: (state, enabled) => {
                    actions.setNotification(state, enabled);
                },
            });
        case "shortcuts":
            return shortcutsSection(scene.shortcuts, scene.capture, {
                openCapture: (action) => {
                    actions.openCapture(action);
                },
                resetShortcut: (action) => {
                    actions.resetShortcut(action);
                },
                resetAll: () => {
                    actions.resetAllShortcuts();
                },
                resolveConflict: (choice) => {
                    actions.resolveConflict(choice);
                },
            });
        case "appearance":
            return appearanceSection(scene.appearance, scene.fonts, {
                chooseTheme: (mode) => {
                    actions.chooseTheme(mode);
                },
                stepFontSize: (step) => {
                    actions.stepFontSize(step);
                },
                chooseFont: (family) => {
                    actions.chooseFont(family);
                },
                chooseDensity: (density) => {
                    actions.chooseDensity(density);
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
    if (scene.removal !== null) return uninstallScreen(scene.removal, actions);
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
    // Le filtre est un fait d'affichage : le backend applique déjà la règle, mais la liste et
    // les suggestions n'arrivent pas par le même aller-retour — voir `pendingSuggestions`.
    const suggested = pendingSuggestions(scene.suggestions, tools);

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
            ? [noToolsYet(emptyToolsProse(suggested))]
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
        // **Sous** les cartes, et hors du corps qui défile centré quand il est vide : ce
        // qu'Ash a vu tourner n'est pas une entrée, et la liste déclarée reste la liste.
        ...suggestionList(suggested, actions),
        // Sans entrée déclarée, il n'y a aucun fichier à énumérer : le geste n'aurait rien
        // à annoncer, et un bouton qui ne peut rien dire ne se propose pas.
        ...(empty ? [] : [uninstallRow(actions)]),
        // La ligne du journal ne suit **pas** la même condition que la désinstallation : le
        // journal se remplit dès qu'un agent reconnu commite, y compris quand aucune entrée
        // n'est déclarée et qu'aucun hook n'est posé — l'attribution ne dépend que de la
        // sonde (ADR-0014). La seule fois où elle ne s'affiche pas est celle où elle n'aurait
        // rien à dire : rien de déclaré, et rien d'observé. L'écran vide garde alors son
        // unique geste, « add ».
        ...(empty && (scene.journal?.entries ?? 0) === 0 ? [] : [journalRow(scene.journal, actions)]),
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
        // Le bloc de capture consomme les frappes : sans focus dedans, `esc`, `⌫` et `⏎`
        // n'arriveraient jamais jusqu'à la fenêtre. Il est refait à chaque frappe — le panneau
        // entier est reposé —, donc le focus se redonne à chaque rendu, et pas seulement à
        // l'ouverture.
        this.panel.querySelector<HTMLElement>(".settings-capture")?.focus();
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
