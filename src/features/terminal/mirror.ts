/**
 * Ce qui tient le `PtyFrame` de `ports.ts` collé à celui de
 * `src-tauri/src/features/pty/commands.rs`.
 *
 * Il n'y est plus seul : les sept interrupteurs de la ligne de statut y sont aussi, pour la
 * même raison — ils ne servent qu'à cette feature. Les autres formes que la feature terminal
 * manipule —
 * `TabInfo`, `WorktreeMetadata` — viennent de `shared/ipc`, et sont gardées là-bas. Celle-ci
 * est écrite ici parce qu'elle ne sert qu'à cette feature : c'est ce que le canal Tauri
 * d'un onglet transporte, et rien d'autre ne l'ouvre.
 */

import type { PtyFrame as RustPtyFrame } from "@/shared/ipc/generated/PtyFrame";
import type { StatusBarSegment as RustStatusBarSegment } from "@/shared/ipc/generated/StatusBarSegment";
import type { StatusBarSegments as RustStatusBarSegments } from "@/shared/ipc/generated/StatusBarSegments";
import type { Assert, Mirrors } from "@/shared/ipc/mirroring";

import type { PtyFrame } from "./ports";
import type { StatusBarSegmentId, StatusBarSegments } from "./status-bar";

export type PtyFrameStillMirrorsRust = Assert<Mirrors<RustPtyFrame, PtyFrame>>;

/**
 * Les sept interrupteurs de la ligne de statut, tels que `status_bar_segments` et
 * `ash://status-bar-segments` les sérialisent.
 *
 * Le côté écrit à la main est déclaré dans `status-bar.ts`, avec le menu qui le rend. Ce que
 * cette ligne attrape est la faute que `parseStatusBarSegments` ne peut **pas** voir : il
 * sait retomber sur les défauts quand un champ manque, il ne sait pas dire qu'il manque
 * depuis un renommage côté Rust. Un `cwd` devenu `directory` laisserait la ligne toujours
 * montrer son répertoire, quoi qu'on décoche, sans un mot. C'est la forme de #16 et #48.
 */
export type StatusBarSegmentsStillMirrorsRust = Assert<
    Mirrors<RustStatusBarSegments, StatusBarSegments>
>;

/**
 * L'identifiant qui part en bascule. Le miroir garantit **l'ensemble des sept noms** : un
 * segment ajouté côté Rust sans sa ligne de menu, ou un nom écrit de travers ici, ferait un
 * `toggle_status_bar_segment` que le backend rejetterait en silence.
 */
export type StatusBarSegmentIdStillMirrorsRust = Assert<
    Mirrors<RustStatusBarSegment, StatusBarSegmentId>
>;
