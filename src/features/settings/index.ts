/**
 * API publique de la feature settings.
 *
 * Le reste du frontend n'importe que ce fichier : ni `view`, ni `model`, ni `sections`,
 * ni `bridge` ne sont des points d'entrée.
 *
 * La fenêtre **rend** les commandes déclarées et ce que les quatre tests en ont dit ; elle
 * ne les détient pas. La liste et la vérification vivent en Rust, où leurs autres lecteurs
 * les trouveront — la sonde qui reconnaît un agent (ADR-0006) et l'installation des hooks
 * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)) ne passent pas par cette fenêtre.
 */

import "./settings.css";

import type { AgentState } from "@/shared/ipc";

import type {
    Appearance,
    FixAction,
    FocusedTool,
    KeyStroke,
    NotificationsReport,
    RemovalPlan,
    SettingsPorts,
    SettingsSnapshot,
    ShortcutsReport,
    ToolDraft,
    Verification,
    WindowPorts,
} from "./contract";
import type { RemovalStage, ShortcutCapture } from "./components";
import { captureIntent, focusedDraft, GENERIC_ADAPTER, readStroke } from "./model";
import { createRelaunch, type Timer, windowTimer } from "./relaunch";
import { moveSection, sectionStep, type SettingsSection } from "./sections";
import { SettingsView } from "./view";

export type {
    Appearance,
    FocusedTool,
    FontStep,
    NotificationPermission,
    NotificationsReport,
    CapturePreview,
    ConflictChoice,
    KeyStroke,
    SettingsPorts,
    SettingsSnapshot,
    ShortcutRow,
    ShortcutsReport,
    SidebarDensity,
    ThemeMode,
    ToolDeclaration,
    ToolDraft,
    Verification,
    WindowPorts,
} from "./contract";
export { revealTool, tauriSettings } from "./bridge";
export { RELAUNCH_DELAY, type Timer } from "./relaunch";
export { SETTINGS_SECTIONS, type SettingsSection } from "./sections";
export {
    HOOK_STATES,
    hooksGlyph,
    presentHooks,
    presentVerification,
    VERIFICATION_STATES,
    verificationGlyph,
} from "./verification-state";

export interface Settings {
    readonly element: HTMLElement;
}

/** La clé de report du formulaire d'ajout — il n'a pas encore de commande à son nom. */
// Une barre oblique ne peut pas figurer dans un nom de commande — `tool.rs` la refuse,
// parce que la sonde compare un nom de processus. La clé du brouillon ne peut donc entrer
// en collision avec aucune entrée déclarée, et elle reste lisible dans un diff.
const DRAFT = "/draft";

/** Une saisie vierge. `generic` est le premier adaptateur proposé, à défaut d'un autre. */
function emptyDraft(adapters: readonly string[]): ToolDraft {
    return { command: "", label: "", adapter: adapters[0] ?? GENERIC_ADAPTER, config: "" };
}

/**
 * Monte la fenêtre de réglages dans `root`.
 *
 * Ce que cette fonction détient est **ce qu'on regarde**, et rien d'autre : la section
 * ouverte, la saisie en cours, le texte des champs entre une frappe et la vérification
 * qu'elle déclenche, et le dernier refus du backend. La liste des outils et ce que les
 * quatre tests en disent, eux, ne sont jamais modifiés ici — chaque appel en rapporte une
 * version neuve ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * `timer` est injecté pour la même raison que l'horloge de `shared/time.rs` : le debounce
 * de 400 ms est une règle, et une règle se prouve sans faire dormir un test.
 *
 * `windowPorts` porte ce qui n'appartient pas à `features::settings` — le thème, la taille de
 * police, la liste des raccourcis. Deux jeux de ports plutôt qu'un, parce que ce sont deux
 * backends : la fenêtre les **rend** tous les deux et n'en détient aucun.
 */
export function mountSettings(
    root: HTMLElement,
    ports: SettingsPorts,
    windowPorts: WindowPorts,
    timer: Timer = windowTimer,
): Settings {
    let section: SettingsSection = "tools";
    let snapshot: SettingsSnapshot = { tools: [], adapters: [], tests: [] };
    let draft: ToolDraft | null = null;
    let draftVerification: Verification | null = null;
    let failure: string | null = null;
    /** L'entrée dont on regarde le conflit — un écran, pas un état d'outil. */
    let conflict: string | null = null;
    /**
     * L'annonce de la désinstallation, puis son compte rendu — ou `null`.
     *
     * Ce n'est **pas** un état de ce qui est écrit sur le disque : c'est ce que le backend
     * vient de dire, retenu le temps qu'on le lise. Chaque ouverture le redemande, parce que
     * le fichier de l'utilisateur a pu changer entre-temps
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    let removal: RemovalStage | null = null;
    /** Ce que le backend dit des notifications macOS, ou `null` tant qu'il n'a rien dit. */
    let notifications: NotificationsReport | null = null;
    /**
     * Le thème et la taille de police que `features::theme` détient.
     *
     * Ce n'est pas un état de la fenêtre : c'est une copie de rendu, remplacée à chaque
     * annonce du backend — le menu Vue en produit autant que cet écran
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    let appearance: Appearance | null = null;
    /**
     * Les raccourcis en vigueur, tels que `features::shortcuts` les détient.
     *
     * Relus à chaque geste, et jamais modifiés ici : chaque appel en rapporte une version
     * neuve, celle dont le menu natif vient d'être refait
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    let shortcuts: ShortcutsReport | null = null;
    /** La ligne ouverte en capture, ou `null`. Un fait d'affichage, pas une liaison. */
    let capture: ShortcutCapture | null = null;
    /**
     * La dernière frappe que le backend a acceptée, celle que `⏎` posera.
     *
     * Elle est gardée telle qu'elle a été lue, et non re-fabriquée depuis les glyphes
     * affichés : ce sont les glyphes qui dérivent de la frappe, jamais l'inverse.
     */
    let captured: KeyStroke | null = null;
    /**
     * Les familles monospace installées, ou `null` tant que le backend ne les a pas lues.
     *
     * Lues une fois, comme les raccourcis : installer une police passe par le Livre des
     * polices, pas par Ash. Ce n'est pas un état de la fenêtre — c'est ce que le système
     * porte, rendu ici.
     */
    let fonts: readonly string[] | null = null;
    /** Ce qui est tapé dans le champ de chemin d'une carte, tant qu'il n'a pas été jugé. */
    const edits = new Map<string, string>();

    /**
     * La relance automatique : 400 ms après la dernière frappe, tout de suite autrement.
     *
     * Elle est **par entrée** — deux cartes se vérifient indépendamment — et elle sert
     * aussi bien une carte que le formulaire d'ajout, qui n'ont ni la même commande à
     * appeler ni le même endroit où poser la réponse.
     */
    const relaunch = createRelaunch((key) => {
        if (key === DRAFT) {
            void verifyDraft();
            return;
        }
        const tool = snapshot.tools.find((one) => one.command === key);
        if (tool === undefined) return;
        void apply(ports.retargetTool(key, tool.adapter, edits.get(key) ?? tool.config ?? ""));
    }, timer);

    const view = new SettingsView({
        selectSection: (next) => {
            // Une capture ouverte n'a plus de bloc où s'afficher dès qu'on change de
            // section, et c'est le même filet que le `blur` de la fenêtre : sans elle, les
            // entrées du menu resteraient éteintes et chaque frappe serait avalée par un
            // bloc qu'on ne voit plus — `esc` en sortirait, mais rien ne dirait de le
            // presser.
            if (capture !== null) closeCapture();
            section = next;
            // L'autorisation macOS se change dans les Réglages Système pendant qu'Ash
            // tourne : la relire en ouvrant la section est la seule façon de ne pas
            // afficher un état d'hier.
            if (next === "notifications") void askNotifications();
            draw();
        },
        startAdding: () => {
            draft = emptyDraft(snapshot.adapters);
            draftVerification = null;
            failure = null;
            draw();
        },
        cancelAdding: () => {
            closeDraft();
            draw();
        },
        editDraft: (patch) => {
            if (draft === null) return;
            draft = { ...draft, ...patch };
            // Un refus du backend porte sur la saisie qu'on vient de corriger : le garder
            // à l'écran ferait lire une erreur qui ne décrit plus rien.
            failure = null;
            // Et ce que les tests ont dit décrivait l'ancienne saisie : `add` retombe sur
            // sa patience plutôt que sur un verdict périmé.
            draftVerification = null;
            // Le menu d'adaptateur ne sera suivi d'aucune frappe ; le reste, si.
            if (patch.adapter === undefined) relaunch.soon(DRAFT);
            else relaunch.now(DRAFT);
            draw();
        },
        submitDraft: () => {
            if (draft === null) return;
            void apply(ports.declareTool(draft), () => {
                closeDraft();
            });
        },
        forgetTool: (command) => {
            // Le report en cours désigne une carte qui va disparaître : le laisser courir
            // ferait vérifier une entrée qui n'est plus là.
            relaunch.cancel(command);
            edits.delete(command);
            void apply(ports.forgetTool(command));
        },
        typePath: (command, value) => {
            edits.set(command, value);
            relaunch.soon(command);
            // Pas de redessin : le champ est déjà à jour, et refaire le DOM sous les doigts
            // de celui qui tape n'apporterait rien.
        },
        commitPath: (command) => {
            // `⏎` et la perte de focus disent « j'ai fini de taper » ; ils n'ajoutent pas
            // de vérification quand il n'y a rien de nouveau à juger.
            const tool = snapshot.tools.find((one) => one.command === command);
            if (tool === undefined) return;
            if ((edits.get(command) ?? tool.config ?? "") === (tool.config ?? "")) {
                relaunch.cancel(command);
                return;
            }
            relaunch.now(command);
        },
        selectAdapter: (command, adapter) => {
            relaunch.cancel(command);
            const tool = snapshot.tools.find((one) => one.command === command);
            void apply(
                ports.retargetTool(command, adapter, edits.get(command) ?? tool?.config ?? ""),
            );
        },
        verifyTool: (command) => {
            relaunch.cancel(command);
            void apply(ports.verifyTool(command));
        },
        verifyAll: () => {
            relaunch.cancelAll();
            void apply(ports.verifyAll());
        },
        applyFix: (command, fix) => {
            void apply(retarget(command, fix));
        },
        resetTool: (command) => {
            relaunch.cancel(command);
            // Ce qui était tapé décrivait l'ancien dossier : le garder ferait réapparaître
            // le chemin qu'on vient justement d'abandonner.
            edits.delete(command);
            void apply(ports.resetTool(command));
        },
        undoReset: (command) => {
            relaunch.cancel(command);
            edits.delete(command);
            void apply(ports.undoReset(command));
        },
        installHooks: (command) => {
            void apply(ports.installHooks(command));
        },
        removeHooks: (command) => {
            void apply(ports.removeHooks(command));
        },
        openConflict: (command) => {
            // `see the diff` n'écrit rien : elle ouvre un écran, et c'est ce qui la
            // distingue des trois autres actions de la ligne.
            conflict = command;
            draw();
        },
        closeConflict: () => {
            conflict = null;
            draw();
        },
        // Les trois gestes de la désinstallation. Le premier **lit**, le deuxième écrit, et
        // ils sont séparés parce que la règle du produit l'exige : Ash dit ce qu'il va faire
        // avant de le faire (spec §10).
        planRemoval: () => {
            void askRemovalPlan();
        },
        removeEverything: () => {
            void removeEverything();
        },
        closeRemoval: () => {
            removal = null;
            draw();
        },
        // Les deux gestes de la section `appearance` **ne changent rien ici** : ils demandent,
        // et c'est l'annonce du backend qui repose la scène. Poser le nouveau mode au passage
        // ferait de cette fenêtre un second détenteur de l'apparence, et une bascule refusée
        // — ou un mode déjà en vigueur, que le backend ne réémet pas — laisserait l'écran
        // affirmer un choix qui n'a pas eu lieu.
        chooseTheme: (mode) => {
            void windowPorts.chooseThemeMode(mode).catch(() => undefined);
        },
        stepFontSize: (step) => {
            void windowPorts.stepTerminalFontSize(step).catch(() => undefined);
        },
        // La police et la densité suivent exactement le même chemin, bien qu'elles n'aient
        // qu'une surface : c'est le backend qui retient et annonce, et la scène ne bouge
        // qu'au retour de l'annonce.
        chooseFont: (family) => {
            void windowPorts.chooseTerminalFont(family).catch(() => undefined);
        },
        chooseDensity: (density) => {
            void windowPorts.chooseSidebarDensity(density).catch(() => undefined);
        },
        // L'interrupteur, lui, **rapporte** la section : contrairement au thème, il n'y a pas
        // d'annonce à toutes les fenêtres — le réglage n'a qu'une surface, et c'est celle-ci.
        // La position affichée reste donc celle que le backend a retenue, jamais celle qu'on
        // vient de demander ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
        setNotification: (state, enabled) => {
            void flipNotification(state, enabled);
        },
        // Les six gestes de la section `shortcuts`. Les deux premiers ouvrent et referment un
        // bloc — c'est de l'affichage ; les quatre autres demandent, et le backend rend
        // l'instantané dont le menu natif vient d'être refait.
        openCapture: (action) => {
            capture = { action, keys: "", why: null, note: null };
            captured = null;
            // Les entrées du menu s'éteignent le temps de la capture : sinon `⌘W` frappé ici
            // fermerait la fenêtre au lieu d'être lu — un accélérateur de menu est consommé
            // avant la webview sur macOS.
            void windowPorts.listenForShortcut(true).catch(() => undefined);
            draw();
        },
        cancelCapture: () => {
            closeCapture();
            draw();
        },
        confirmCapture: () => {
            const asked = capture;
            const stroke = captured;
            // `⏎` sur un bloc où rien n'a encore été frappé, ou sur une frappe que le backend
            // a refusée : il n'y a rien à poser, et refermer le bloc ferait croire à un choix.
            if (asked === null || stroke === null) return;
            void askShortcuts(windowPorts.bindShortcut(asked.action, stroke), () => {
                closeCapture();
            });
        },
        clearShortcut: (action) => {
            void askShortcuts(windowPorts.clearShortcut(action), () => {
                closeCapture();
            });
        },
        resetShortcut: (action) => {
            void askShortcuts(windowPorts.resetShortcut(action));
        },
        resetAllShortcuts: () => {
            void askShortcuts(windowPorts.resetAllShortcuts());
        },
        resolveConflict: (choice) => {
            void askShortcuts(windowPorts.resolveShortcutConflict(choice));
        },
    });

    function closeCapture(): void {
        capture = null;
        captured = null;
        void windowPorts.listenForShortcut(false).catch(() => undefined);
    }

    /** Bascule un interrupteur. Un refus laisse la section telle qu'elle était. */
    async function flipNotification(state: AgentState, enabled: boolean): Promise<void> {
        try {
            notifications = await ports.setNotification(state, enabled);
        } catch {
            return;
        }
        draw();
    }

    /** Ce qu'`apply` fait d'une correction proposée — un seul champ change à la fois. */
    function retarget(command: string, fix: FixAction): Promise<SettingsSnapshot> {
        const tool = snapshot.tools.find((one) => one.command === command);
        const config = edits.get(command) ?? tool?.config ?? "";
        if (fix.kind === "useAdapter") {
            return ports.retargetTool(command, fix.adapter, config);
        }
        edits.set(command, fix.path);
        return ports.retargetTool(command, tool?.adapter ?? GENERIC_ADAPTER, fix.path);
    }

    /**
     * Demande l'annonce du retrait, et ouvre l'écran dessus. **Rien n'est écrit.**
     *
     * Un refus n'ouvre pas d'écran vide : il s'affiche là où les autres refus s'affichent,
     * et la liste reste sous les yeux.
     */
    async function askRemovalPlan(): Promise<void> {
        let plan: RemovalPlan;
        try {
            plan = await ports.removalPlan();
        } catch (error: unknown) {
            failure = error instanceof Error ? error.message : String(error);
            draw();
            return;
        }
        removal = { step: "asked", plan };
        failure = null;
        draw();
    }

    /** Le geste qui écrit — et qui ne part que du clic pris devant l'annonce. */
    async function removeEverything(): Promise<void> {
        try {
            const outcome = await ports.removeAllHooks();
            snapshot = outcome.snapshot;
            for (const tool of snapshot.tools) edits.delete(tool.command);
            removal = { step: "done", report: outcome.report };
            failure = null;
        } catch (error: unknown) {
            // Le refus laisse l'annonce à l'écran : c'est elle qui décrit ce qui n'a pas eu
            // lieu, et la refermer ferait disparaître la question avec la réponse.
            failure = error instanceof Error ? error.message : String(error);
        }
        draw();
    }

    /** Relit la section `notifications`. Un refus la laisse à ce qu'elle montrait. */
    async function askNotifications(): Promise<void> {
        try {
            notifications = await ports.notifications();
        } catch {
            return;
        }
        draw();
    }

    /** Lit les polices installées. Un refus laisse la liste à `null` — la section le dit. */
    async function askFonts(): Promise<void> {
        try {
            fonts = await windowPorts.monospaceFonts();
        } catch {
            return;
        }
        draw();
    }

    /** Relit l'apparence. Un refus la laisse à ce qu'elle montrait, comme `notifications`. */
    async function askAppearance(): Promise<void> {
        try {
            appearance = await windowPorts.appearance();
        } catch {
            return;
        }
        draw();
    }

    /**
     * Se pose sur l'outil que la sidebar a désigné (ADR-0006).
     *
     * **Après** la liste, et non en parallèle : sans elle, un outil déjà déclaré ne serait
     * pas reconnu comme tel et l'écran ouvrirait une saisie en double. Un refus ne fait
     * rien — la fenêtre s'ouvre alors comme elle s'ouvre toujours.
     */
    async function focusTool(focused: FocusedTool): Promise<void> {
        section = "tools";
        const prefilled = focusedDraft(focused, snapshot);
        if (prefilled === null) {
            // Un outil déjà déclaré n'ouvre aucune saisie, donc ne fait lire aucun dossier :
            // c'est cette sortie-là qui garantit qu'on ne touche au disque que pour remplir
            // un champ qui existe.
            draw();
            return;
        }
        // Le dossier conventionnel est demandé **ici**, pour l'adaptateur que cette saisie
        // porte — celui qu'on va vraiment montrer, pas celui qui a été reconnu. Un refus
        // laisse le champ vide : on tape son chemin comme avant cette proposition.
        draft = { ...prefilled, config: (await proposeConfig(prefilled.adapter)) ?? "" };
        draftVerification = null;
        failure = null;
        // La séquence part tout de suite, comme après un choix d'adaptateur : la saisie est
        // complète, et rien n'est encore écrit chez l'utilisateur.
        relaunch.now(DRAFT);
        draw();
    }

    /** Ce que le backend propose pour cet adaptateur, ou rien s'il ne répond pas. */
    async function proposeConfig(adapter: string): Promise<string | null> {
        try {
            return await ports.proposedConfig(adapter);
        } catch {
            return null;
        }
    }

    async function askFocus(): Promise<void> {
        try {
            const focused = await ports.pendingFocus();
            if (focused !== null) await focusTool(focused);
        } catch {
            // Aucune demande lisible : la fenêtre s'ouvre comme elle s'ouvre toujours.
        }
    }

    /**
     * Rejoue un appel aux liaisons : l'instantané rendu devient l'écran.
     *
     * Un refus ne fait rien disparaître et n'invente rien : la scène reste celle du dernier
     * instantané connu. Le backend a déjà refait le menu quand il répond, donc l'écran et le
     * menu disent la même chose à la même seconde
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    async function askShortcuts(
        call: Promise<ShortcutsReport>,
        onSuccess?: () => void,
    ): Promise<void> {
        try {
            shortcuts = await call;
            onSuccess?.();
        } catch {
            // Le refus est déjà écrit par le backend, et le bloc de capture l'affiche au
            // moment de la frappe : le rejouer ici ne dirait rien de plus.
        }
        draw();
    }

    /** Ce que le backend dit de la frappe en cours — il ne pose rien, `⏎` seul pose. */
    async function previewStroke(stroke: KeyStroke): Promise<void> {
        const asked = capture;
        let previewed;
        try {
            previewed = await windowPorts.previewShortcut(stroke);
        } catch {
            return;
        }
        // La capture a pu se refermer, ou changer de ligne, pendant l'aller-retour : une
        // réponse à une question qu'on ne pose plus n'a rien à afficher.
        if (capture === null || asked === null || capture.action !== asked.action) return;
        capture = {
            action: capture.action,
            keys: previewed.keys,
            why: previewed.why,
            note: previewed.reservation?.note ?? null,
        };
        // Une combinaison refusée n'est pas gardée : `⏎` n'aurait rien à poser.
        captured = previewed.accepted ? stroke : null;
        draw();
    }

    function closeDraft(): void {
        relaunch.cancel(DRAFT);
        draft = null;
        draftVerification = null;
        failure = null;
    }

    async function verifyDraft(): Promise<void> {
        if (draft === null) return;
        const asked = draft;
        try {
            const verification = await ports.verifyDraft(asked);
            // La saisie a pu changer pendant l'aller-retour : une réponse à une question
            // qu'on ne pose plus n'a rien à afficher.
            if (draft !== asked) return;
            draftVerification = verification;
        } catch {
            // Le refus est soumis à la **même** règle de fraîcheur que la réponse : le
            // backend refuse désormais une saisie dont le champ `command` n'est pas un nom
            // de processus, donc ce chemin se prend en tapant. Sans cette garde, le refus
            // d'une saisie qu'on a déjà corrigée effacerait la vérification de celle qui
            // l'a remplacée, et la rangée de pastilles resterait vide jusqu'à la frappe
            // suivante.
            if (draft !== asked) return;
            draftVerification = null;
        }
        draw();
    }

    function draw(): void {
        // Le conflit se referme tout seul quand il n'y en a plus : une installation réussie
        // ailleurs, une entrée oubliée, un fichier remis d'aplomb. Garder l'écran ouvert sur
        // un diff que le backend ne rapporte plus montrerait un refus qui n'existe plus.
        const shown = snapshot.tools.find((tool) => tool.command === conflict);
        if (shown === undefined || shown.hooks.state !== "conflict") conflict = null;
        view.render({
            section,
            snapshot,
            draft,
            draftVerification,
            failure,
            edits,
            conflict,
            removal,
            notifications,
            appearance,
            fonts,
            shortcuts,
            capture,
        });
    }

    /**
     * Rejoue un appel au backend : l'instantané rendu devient l'écran, un refus devient
     * une raison affichée.
     *
     * Un refus ne fait **rien** disparaître : la saisie reste, et sa raison s'affiche là
     * où le bouton l'annonçait déjà.
     */
    async function apply(call: Promise<SettingsSnapshot>, onSuccess?: () => void): Promise<void> {
        try {
            snapshot = await call;
            // Le backend a jugé ce qui était tapé : les champs repartent de ce qu'il
            // détient, sauf ceux qu'on est en train de modifier ailleurs.
            for (const tool of snapshot.tools) edits.delete(tool.command);
            failure = null;
            onSuccess?.();
        } catch (error: unknown) {
            failure = error instanceof Error ? error.message : String(error);
        }
        draw();
    }

    // Deux raccourcis, et deux seulement : `⌥↑↓` change de section, `esc` abandonne un
    // ajout. `tab` n'est pas ici — c'est le parcours du navigateur, et la colonne est faite
    // de vrais boutons.
    root.addEventListener("keydown", (event) => {
        // Le bloc de capture consomme **tout** tant qu'il est ouvert : c'est le sens de la
        // capture, et c'est aussi pourquoi ses trois issues sont testées ailleurs
        // (`captureIntent`) — se tromper d'issue signifierait ne plus pouvoir en sortir.
        if (capture !== null) {
            const intent = captureIntent(event);
            if (intent === "ignore") return;
            event.preventDefault();
            if (intent === "cancel") {
                closeCapture();
                draw();
            } else if (intent === "clear") {
                void askShortcuts(windowPorts.clearShortcut(capture.action), closeCapture);
            } else if (intent === "confirm") {
                if (captured !== null) {
                    void askShortcuts(
                        windowPorts.bindShortcut(capture.action, captured),
                        closeCapture,
                    );
                }
            } else {
                void previewStroke(readStroke(event));
            }
            return;
        }

        if (event.key === "Escape" && draft !== null) {
            closeDraft();
            draw();
            return;
        }

        const step = sectionStep(event);
        if (step === null) return;
        // La frappe est consommée : sans ça, `⌥↓` déplace aussi le curseur dans le champ
        // qui a le focus.
        event.preventDefault();
        section = moveSection(section, step);
        if (section === "notifications") void askNotifications();
        draw();
        view.focusActiveSection();
    });

    // Une capture ouverte pendant qu'on va cliquer ailleurs n'attend plus rien : elle se
    // referme, et le menu retrouve ses touches. C'est aussi le filet de la fenêtre — un bloc
    // laissé ouvert garderait les entrées du menu éteintes.
    root.ownerDocument.defaultView?.addEventListener("blur", () => {
        if (capture === null) return;
        closeCapture();
        draw();
    });

    draw();
    // La demande de la sidebar se lit **après** la liste : voir [`focusTool`].
    void apply(ports.tools()).then(askFocus);
    void askNotifications();
    // L'apparence et les raccourcis sont lus au montage et non à l'ouverture de leur section,
    // contrairement à l'autorisation macOS : celle-ci se change dans les Réglages Système
    // pendant qu'Ash tourne, alors que le thème et le menu ne changent que par Ash lui-même —
    // et le thème qui change, on l'apprend par l'annonce ci-dessous.
    void askAppearance();
    void askFonts();
    void askShortcuts(windowPorts.shortcuts());

    // L'apparence change aussi depuis le menu Vue, pendant que cette fenêtre est ouverte.
    // C'est le **même** chemin qu'un choix fait ici : les deux surfaces ne peuvent donc pas
    // se contredire ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    windowPorts.onAppearanceChanged((changed) => {
        appearance = changed;
        draw();
    });

    /**
     * Le **second temps** : le test 4 a répondu, parfois plusieurs secondes après le
     * premier.
     *
     * La charge utile est celle que le backend détient — il l'a déjà posée sur son entrée
     * avant de l'émettre, et il ne l'émet pas quand il l'a jetée. La recopier ici est du
     * rendu, pas de la détention : la fenêtre ne la calcule ni ne la corrige, et une entrée
     * qu'elle ne connaît pas est ignorée.
     *
     * **La ligne `hooks` voyage avec le résultat**, et remplace celle du premier temps : le
     * test 4 peut retirer à une entrée le droit d'écrire qu'elle avait pendant qu'elle
     * l'attendait. La garder ferait montrer un bouton `install` allumé sur une entrée que
     * le backend refuse désormais.
     */
    // Le même geste quand la fenêtre était déjà ouverte : l'event, lui, a un abonné.
    void ports.onFocusTool((focused) => {
        void focusTool(focused);
    });

    void ports.onVerified(({ command, verification, verified, hooks }) => {
        if (draft !== null && draft.command.trim() === command) {
            draftVerification = verification;
            draw();
            return;
        }
        snapshot = {
            ...snapshot,
            tools: snapshot.tools.map((tool) =>
                tool.command === command
                    ? { ...tool, verification, verified, hooks: hooks ?? tool.hooks }
                    : tool,
            ),
        };
        draw();
    });

    root.append(view.element);
    return { element: view.element };
}
