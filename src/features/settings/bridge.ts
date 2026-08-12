import { invoke } from "@tauri-apps/api/core";

import type { SettingsPorts, SettingsSnapshot, ToolDraft } from "./contract";

/**
 * L'implémentation réelle du pont vers `features::settings` : trois commandes, et rien
 * d'autre.
 *
 * Le pendant de `pty-bridge.ts` et de `git-bridge.ts`, posé pour la même raison : la
 * feature qui consomme un état écrit le pont vers lui. Elle ne connaît du backend que ces
 * trois noms.
 *
 * Chaque appel rend l'**instantané entier**, et la fenêtre redessine à partir de lui : un
 * ajout ne modifie jamais une liste locale, il rapporte celle que le backend détient
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export const tauriSettings: SettingsPorts = {
    tools: () => invoke<SettingsSnapshot>("settings_tools"),
    // Le nom du paramètre est `tool` des deux côtés : Tauri passe les arguments par nom, et
    // une faute de frappe ici se verrait à l'exécution, pas à la compilation.
    declareTool: (draft: ToolDraft) => invoke<SettingsSnapshot>("settings_declare_tool", { tool: draft }),
    forgetTool: (command: string) => invoke<SettingsSnapshot>("settings_forget_tool", { command }),
};
