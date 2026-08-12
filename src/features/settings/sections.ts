/**
 * Les quatre sections de la fenêtre, et la façon de passer de l'une à l'autre.
 *
 * L'ordre est celui de la maquette et n'est pas négociable au fil d'une tâche : `tools`,
 * `shortcuts`, `appearance`, `notifications`. Il va du plus structurant au plus
 * accessoire, et c'est aussi l'ordre dans lequel `⌥↓` les parcourt.
 */

export type SettingsSection = "tools" | "shortcuts" | "appearance" | "notifications";

export const SETTINGS_SECTIONS: readonly SettingsSection[] = [
    "tools",
    "shortcuts",
    "appearance",
    "notifications",
];

/** De quoi décider d'un déplacement sans dépendre d'un `KeyboardEvent` réel. */
export interface SectionKey {
    key: string;
    altKey: boolean;
}

/**
 * Le déplacement qu'une frappe demande, ou rien.
 *
 * `⌥↑` et `⌥↓`, et **uniquement** avec la touche option : les flèches nues appartiennent
 * à ce qui a le focus — une liste, un champ de saisie. Les prendre ici rendrait un champ
 * de chemin impossible à parcourir au clavier.
 *
 * `tab`, lui, n'est pas ici : c'est le parcours du navigateur, et le laisser faire est
 * exactement ce qu'il faut faire. La colonne de navigation est faite de vrais boutons,
 * donc `tab` les traverse sans qu'on écrive une ligne — et sans casser le parcours des
 * lecteurs d'écran.
 */
export function sectionStep(event: SectionKey): -1 | 1 | null {
    if (!event.altKey) return null;
    if (event.key === "ArrowUp") return -1;
    if (event.key === "ArrowDown") return 1;
    return null;
}

/**
 * La section atteinte depuis `current` après un déplacement de `step`.
 *
 * **Aux extrémités, on reste** : la liste est courte et visible en entier, donc un retour
 * silencieux de `notifications` à `tools` se lirait comme un saut, pas comme une
 * navigation. C'est aussi ce que fait une liste native de macOS.
 */
export function moveSection(current: SettingsSection, step: -1 | 1): SettingsSection {
    const index = SETTINGS_SECTIONS.indexOf(current);
    const next = Math.min(Math.max(index + step, 0), SETTINGS_SECTIONS.length - 1);
    return SETTINGS_SECTIONS[next] ?? current;
}
