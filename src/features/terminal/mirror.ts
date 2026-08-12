/**
 * Ce qui tient le `PtyFrame` de `ports.ts` collé à celui de
 * `src-tauri/src/features/pty/commands.rs`.
 *
 * Il est seul dans ce fichier : les autres formes que la feature terminal manipule —
 * `TabInfo`, `WorktreeMetadata` — viennent de `shared/ipc`, et sont gardées là-bas. Celle-ci
 * est écrite ici parce qu'elle ne sert qu'à cette feature : c'est ce que le canal Tauri
 * d'un onglet transporte, et rien d'autre ne l'ouvre.
 */

import type { PtyFrame as RustPtyFrame } from "@/shared/ipc/generated/PtyFrame";
import type { Assert, Mirrors } from "@/shared/ipc/mirroring";

import type { PtyFrame } from "./ports";

export type PtyFrameStillMirrorsRust = Assert<Mirrors<RustPtyFrame, PtyFrame>>;
