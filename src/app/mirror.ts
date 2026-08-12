/**
 * Ce qui tient le `ThemeMode` de `theme.ts` collé à celui de
 * `src-tauri/src/features/theme/mode.rs`.
 *
 * Le thème est déjà le seul endroit du frontend qui se défende à l'exécution :
 * `parseThemeMode` refuse ce qu'il ne reconnaît pas et retombe sur le système. Ce garde-fou
 * dit ce qu'il faut faire d'une valeur inattendue ; il ne dit pas qu'elle est inattendue —
 * un mode renommé côté Rust se serait traduit par un thème qui ne s'applique plus, en
 * silence. C'est exactement la forme qu'avait #48.
 */

import type { ThemeMode as RustThemeMode } from "@/shared/ipc/generated/ThemeMode";
import type { Assert, Mirrors } from "@/shared/ipc/mirroring";

import type { ThemeMode } from "./theme";

export type ThemeModeStillMirrorsRust = Assert<Mirrors<RustThemeMode, ThemeMode>>;
