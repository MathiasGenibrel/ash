import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { AgentState } from "@/shared/ipc";

import type {
    FocusedTool,
    NotificationsReport,
    SettingsPorts,
    SettingsSnapshot,
    ToolDraft,
    Verification,
    Verified,
} from "./contract";

/**
 * L'implémentation réelle du pont vers `features::settings` : treize commandes, un event, et
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

/** L'event qui amène une fenêtre **déjà ouverte** sur un outil (ADR-0006). */
export const SETTINGS_FOCUS_TOOL = "ash://settings-focus-tool";

/**
 * Ouvre les réglages sur un outil — le geste du marqueur « non instrumenté » de la sidebar.
 *
 * Elle est exportée par la feature parce que son appelant est la **fenêtre principale**, qui
 * ne connaît de `settings` que ce nom : la sidebar informe, l'écran agit
 * ([ADR-0010](../../../docs/adr/0010-sidebar-informe-terminal-agit.md)). Elle n'écrit rien —
 * elle ouvre une fenêtre.
 */
export function revealTool(command: string, adapter: string): void {
    void invoke("settings_reveal_tool", { command, adapter });
}

/** Le dossier vide se dit `null` au backend : « celui de l'adaptateur », pas « aucun ». */
function folder(config: string): string | null {
    const trimmed = config.trim();
    return trimmed === "" ? null : trimmed;
}

export const tauriSettings: SettingsPorts = {
    tools: () => invoke<SettingsSnapshot>("settings_tools"),
    notifications: () => invoke<NotificationsReport>("settings_notifications"),
    setNotification: (state: AgentState, enabled: boolean) =>
        invoke<NotificationsReport>("settings_set_notification", { state, enabled }),
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
    pendingFocus: () => invoke<FocusedTool | null>("settings_pending_focus"),
    proposedConfig: (adapter: string) =>
        invoke<string | null>("settings_proposed_config", { adapter }),
    onFocusTool: async (listener) => {
        const stop = await listen<FocusedTool>(SETTINGS_FOCUS_TOOL, (event) => {
            listener(event.payload);
        });
        return stop;
    },
    onVerified: async (listener) => {
        const stop = await listen<Verified>(SETTINGS_VERIFIED, (event) => {
            listener(event.payload);
        });
        return stop;
    },
};
