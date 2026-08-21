/**
 * Ce qui tient le `PtyFrame` de `ports.ts` collé à celui de
 * `src-tauri/src/features/pty/commands.rs`.
 *
 * Il n'y est plus seul : la barre de statut y est aussi, pour la
 * même raison — elle ne sert qu'à cette feature. Les autres formes que la feature terminal
 * manipule —
 * `TabInfo`, `WorktreeMetadata` — viennent de `shared/ipc`, et sont gardées là-bas. Celle-ci
 * est écrite ici parce qu'elle ne sert qu'à cette feature : c'est ce que le canal Tauri
 * d'un onglet transporte, et rien d'autre ne l'ouvre.
 */

import type { PtyFrame as RustPtyFrame } from "@/shared/ipc/generated/PtyFrame";
import type { StatusBarItem as RustStatusBarItem } from "@/shared/ipc/generated/StatusBarItem";
import type { StatusBarLayout as RustStatusBarLayout } from "@/shared/ipc/generated/StatusBarLayout";
import type { StatusBarSegment as RustStatusBarSegment } from "@/shared/ipc/generated/StatusBarSegment";
import type { Assert, Mirrors } from "@/shared/ipc/mirroring";

import type { PtyFrame } from "./ports";
import type { StatusBarItemId, StatusBarLayout, StatusBarSegmentId } from "./status-bar";

export type PtyFrameStillMirrorsRust = Assert<Mirrors<RustPtyFrame, PtyFrame>>;

/**
 * La barre de statut, telle que `status_bar_layout` et `ash://status-bar-layout` la
 * sérialisent — une suite de mots.
 *
 * Le côté écrit à la main est déclaré dans `status-bar.ts`, avec les panneaux qui la rendent.
 * Ce que cette ligne attrape est la faute que `parseStatusBarLayout` ne peut **pas** voir :
 * il sait jeter un mot qu'il ne connaît pas, il ne sait pas dire qu'il ne le connaît plus
 * depuis un renommage côté Rust. Un `cwd` devenu `directory` viderait la barre du répertoire
 * de tout le monde, en silence, dès la première écriture. C'est la forme de #16 et #48.
 *
 * Elle porte aussi le mot que le mode édition a introduit : `spacer` n'est pas un segment, et
 * rien d'autre que ce miroir ne le tient collé à la variante `StatusBarItem::Spacer`.
 */
export type StatusBarLayoutStillMirrorsRust = Assert<
    Mirrors<RustStatusBarLayout, StatusBarLayout>
>;

/** Le même mot, pris à l'unité — c'est lui que `set_status_bar_layout` reçoit. */
export type StatusBarItemStillMirrorsRust = Assert<Mirrors<RustStatusBarItem, StatusBarItemId>>;

/**
 * L'identifiant qui part en bascule. Le miroir garantit **l'ensemble des sept noms** : un
 * segment ajouté côté Rust sans sa ligne de menu, ou un nom écrit de travers ici, ferait un
 * `toggle_status_bar_segment` que le backend rejetterait en silence.
 */
export type StatusBarSegmentIdStillMirrorsRust = Assert<
    Mirrors<RustStatusBarSegment, StatusBarSegmentId>
>;
