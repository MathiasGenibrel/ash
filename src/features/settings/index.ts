/**
 * API publique de la feature settings.
 *
 * Le reste du frontend n'importe que ce fichier : ni `view`, ni `model`, ni `sections`,
 * ni `bridge` ne sont des points d'entrée.
 *
 * La fenêtre **rend** les commandes déclarées ; elle ne les détient pas. La liste vit en
 * Rust, où ses autres lecteurs la trouveront — la sonde qui reconnaît un agent (ADR-0006)
 * et l'installation des hooks
 * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)) ne passent pas par cette fenêtre.
 */

import "./settings.css";

import type { SettingsPorts, SettingsSnapshot, ToolDraft } from "./contract";
import { GENERIC_ADAPTER } from "./model";
import { moveSection, sectionStep, type SettingsSection } from "./sections";
import { SettingsView } from "./view";

export type { SettingsPorts, SettingsSnapshot, ToolDeclaration, ToolDraft } from "./contract";
export { tauriSettings } from "./bridge";
export { SETTINGS_SECTIONS, type SettingsSection } from "./sections";

export interface Settings {
    readonly element: HTMLElement;
}

/** Une saisie vierge. `generic` est le premier adaptateur proposé, à défaut d'un autre. */
function emptyDraft(adapters: readonly string[]): ToolDraft {
    return { command: "", label: "", adapter: adapters[0] ?? GENERIC_ADAPTER, config: "" };
}

/**
 * Monte la fenêtre de réglages dans `root`.
 *
 * Ce que cette fonction détient est **ce qu'on regarde**, et rien d'autre : la section
 * ouverte, la saisie en cours, et le dernier refus du backend. La liste des outils, elle,
 * n'est jamais modifiée ici — chaque appel en rapporte une neuve.
 */
export function mountSettings(root: HTMLElement, ports: SettingsPorts): Settings {
    let section: SettingsSection = "tools";
    let snapshot: SettingsSnapshot = { tools: [], adapters: [] };
    let draft: ToolDraft | null = null;
    let failure: string | null = null;

    const view = new SettingsView({
        selectSection: (next) => {
            section = next;
            draw();
        },
        startAdding: () => {
            draft = emptyDraft(snapshot.adapters);
            failure = null;
            draw();
        },
        cancelAdding: () => {
            draft = null;
            failure = null;
            draw();
        },
        editDraft: (patch) => {
            if (draft === null) return;
            draft = { ...draft, ...patch };
            // Un refus du backend porte sur la saisie qu'on vient de corriger : le garder
            // à l'écran ferait lire une erreur qui ne décrit plus rien.
            failure = null;
            draw();
        },
        submitDraft: () => {
            if (draft === null) return;
            void apply(ports.declareTool(draft), () => {
                draft = null;
            });
        },
        forgetTool: (command) => {
            void apply(ports.forgetTool(command));
        },
    });

    function draw(): void {
        view.render({ section, snapshot, draft, failure });
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
            draft = null;
            failure = null;
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

    root.append(view.element);
    return { element: view.element };
}
