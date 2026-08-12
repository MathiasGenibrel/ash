/**
 * Ce qui tient les deux préférences d'apparence de `app/` collées aux types de
 * `src-tauri/src/features/theme/` : le `ThemeMode` de `theme.ts` à celui de `mode.rs`, et
 * la taille de police de `font-size.ts` à celle de `font_size.rs`.
 *
 * Le thème est déjà le seul endroit du frontend qui se défende à l'exécution :
 * `parseThemeMode` refuse ce qu'il ne reconnaît pas et retombe sur le système. Ce garde-fou
 * dit ce qu'il faut faire d'une valeur inattendue ; il ne dit pas qu'elle est inattendue —
 * un mode renommé côté Rust se serait traduit par un thème qui ne s'applique plus, en
 * silence. C'est exactement la forme qu'avait #48.
 */

import type { FontSize as RustFontSize } from "@/shared/ipc/generated/FontSize";
import type { ThemeMode as RustThemeMode } from "@/shared/ipc/generated/ThemeMode";
import type { Assert, Mirrors } from "@/shared/ipc/mirroring";

import type { ThemeMode } from "./theme";

export type ThemeModeStillMirrorsRust = Assert<Mirrors<RustThemeMode, ThemeMode>>;

/**
 * La taille de police, telle que `terminal_font_size` et `ash://terminal-font-size` la
 * sérialisent.
 *
 * Le côté écrit à la main est un `number` nu, et non un type nommé : `FontSize` est
 * `#[serde(transparent)]` autour d'un entier, donc **sur le fil c'est un nombre**, et les
 * bornes ne sont pas des bornes de forme — elles vivent en Rust, où elles sont décidées
 * (`font_size.rs`), et `parseFontSize` ne fait que refuser ce qui n'est pas une taille.
 *
 * Ce que cette ligne attrape est l'autre faute, celle que le garde-fou d'exécution ne voit
 * pas : le jour où `FontSize` cesserait d'être un nombre — un `{ points }`, une chaîne avec
 * son unité — `parseFontSize` rendrait `null` sur **chaque** annonce et la taille ne
 * bougerait plus jamais, en silence. C'est la forme de #16 et #48.
 */
export type FontSizeStillMirrorsRust = Assert<Mirrors<RustFontSize, number>>;
