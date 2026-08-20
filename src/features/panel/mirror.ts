/**
 * Ce qui tient l'état du panneau collé à la `struct` Rust dont il est le miroir —
 * `src-tauri/src/features/theme/bottom_panel.rs`.
 *
 * Il vit ici plutôt que dans `app/`, à côté du type écrit à la main : c'est la feature qui
 * recopie une forme, donc c'est à elle de prouver qu'elle la recopie encore. Rien n'existe à
 * l'exécution — ce fichier ne produit pas une ligne de JavaScript.
 *
 * C'est la faute que `parseBottomPanel` ne peut pas voir : il sait refuser une valeur
 * inattendue, il ne sait pas dire qu'elle l'est devenue. Une cinquième vue ajoutée côté Rust,
 * un `open` renommé, une hauteur qui cesserait d'être un nombre nu — et le garde-fou rendrait
 * `null` sur **chaque** annonce, laissant un panneau qui ne s'ouvre plus jamais, sans un mot.
 * C'est la forme de #16 et #48.
 */

import type { BottomPanel as RustBottomPanel } from "@/shared/ipc/generated/BottomPanel";
import type { PanelView as RustPanelView } from "@/shared/ipc/generated/PanelView";
import type { Assert, Mirrors } from "@/shared/ipc/mirroring";

import type { BottomPanelState, PanelView } from "./layout";

export type BottomPanelStillMirrorsRust = Assert<Mirrors<RustBottomPanel, BottomPanelState>>;
export type PanelViewStillMirrorsRust = Assert<Mirrors<RustPanelView, PanelView>>;
