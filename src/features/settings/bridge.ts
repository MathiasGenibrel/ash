import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
    SettingsPorts,
    SettingsSnapshot,
    ToolDraft,
    Verification,
    Verified,
} from "./contract";

/**
 * L'implémentation réelle du pont vers `features::settings` : onze commandes, un event, et
 * rien d'autre.
 *
 * Le pendant de `pty-bridge.ts` et de `git-bridge.ts`, posé pour la même raison : la
 * feature qui consomme un état écrit le pont vers lui. Elle ne connaît du backend que ces
 * noms.
 *
 * Chaque appel rend l'**instantané entier**, et la fenêtre redessine à partir de lui : un
 * ajout ne modifie jamais une liste locale, il rapporte celle que le backend détient
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * `onVerified` est le **second temps** du résultat : les commandes de vérification
 * répondent dès que les tests 1 à 3 ont parlé, et le quatrième — qui lance un programme —
 * arrive par cet event, parfois plusieurs secondes plus tard.
 */
export const SETTINGS_VERIFIED = "ash://settings-verified";

/** Le dossier vide se dit `null` au backend : « celui de l'adaptateur », pas « aucun ». */
function folder(config: string): string | null {
    const trimmed = config.trim();
    return trimmed === "" ? null : trimmed;
}

export const tauriSettings: SettingsPorts = {
    tools: () => invoke<SettingsSnapshot>("settings_tools"),
    // Le nom du paramètre est `tool` des deux côtés : Tauri passe les arguments par nom, et
    // une faute de frappe ici se verrait à l'exécution, pas à la compilation.
    declareTool: (draft: ToolDraft) =>
        invoke<SettingsSnapshot>("settings_declare_tool", { tool: draft }),
    forgetTool: (command: string) => invoke<SettingsSnapshot>("settings_forget_tool", { command }),
    retargetTool: (command: string, adapter: string, config: string) =>
        invoke<SettingsSnapshot>("settings_retarget_tool", {
            command,
            adapter,
            config: folder(config),
        }),
    verifyTool: (command: string) => invoke<SettingsSnapshot>("settings_verify_tool", { command }),
    verifyAll: () => invoke<SettingsSnapshot>("settings_verify_all"),
    verifyDraft: (draft: ToolDraft) =>
        invoke<Verification>("settings_verify_draft", { tool: draft }),
    resetTool: (command: string) => invoke<SettingsSnapshot>("settings_reset_tool", { command }),
    undoReset: (command: string) => invoke<SettingsSnapshot>("settings_undo_reset", { command }),
    installHooks: (command: string) =>
        invoke<SettingsSnapshot>("settings_install_hooks", { command }),
    removeHooks: (command: string) =>
        invoke<SettingsSnapshot>("settings_remove_hooks", { command }),
    onVerified: async (listener) => {
        const stop = await listen<Verified>(SETTINGS_VERIFIED, (event) => {
            listener(event.payload);
        });
        return stop;
    },
};
