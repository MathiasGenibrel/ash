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

import type {
    FixAction,
    SettingsPorts,
    SettingsSnapshot,
    ToolDraft,
    Verification,
} from "./contract";
import { GENERIC_ADAPTER } from "./model";
import { createRelaunch, type Timer, windowTimer } from "./relaunch";
import { moveSection, sectionStep, type SettingsSection } from "./sections";
import { SettingsView } from "./view";

export type {
    SettingsPorts,
    SettingsSnapshot,
    ToolDeclaration,
    ToolDraft,
    Verification,
} from "./contract";
export { tauriSettings } from "./bridge";
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
 */
export function mountSettings(
    root: HTMLElement,
    ports: SettingsPorts,
    timer: Timer = windowTimer,
): Settings {
    let section: SettingsSection = "tools";
    let snapshot: SettingsSnapshot = { tools: [], adapters: [], tests: [] };
    let draft: ToolDraft | null = null;
    let draftVerification: Verification | null = null;
    let failure: string | null = null;
    /** L'entrée dont on regarde le conflit — un écran, pas un état d'outil. */
    let conflict: string | null = null;
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
            section = next;
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
    });

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
        view.render({ section, snapshot, draft, draftVerification, failure, edits, conflict });
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
        draw();
        view.focusActiveSection();
    });

    draw();
    void apply(ports.tools());

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
