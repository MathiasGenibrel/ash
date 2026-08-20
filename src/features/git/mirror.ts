/**
 * Ce qui tient `contract.ts` collé aux `struct` de `src-tauri/src/features/git/history.rs`.
 *
 * Même dispositif que `shared/ipc/mirror.ts` et que `features/settings/mirror.ts` : chaque
 * alias est une assertion que `tsc` évalue, et rien ici ne produit une ligne de JavaScript.
 *
 * La chaîne tient en deux des vérifications obligatoires, **dans cet ordre** : `cargo test`
 * régénère `shared/ipc/generated/`, puis `bun run typecheck` compare. Sauter la première
 * laisse comparer un contrat périmé.
 */

import type { CommitGraph as RustCommitGraph } from "@/shared/ipc/generated/CommitGraph";
import type { CommitRow as RustCommitRow } from "@/shared/ipc/generated/CommitRow";
import type { FoldedBranch as RustFoldedBranch } from "@/shared/ipc/generated/FoldedBranch";
import type { GraphLink as RustGraphLink } from "@/shared/ipc/generated/GraphLink";
import type { Assert, Mirrors } from "@/shared/ipc/mirroring";

import type { CommitGraph, CommitRow, FoldedBranch, GraphLink } from "./contract";

export type GraphLinkStillMirrorsRust = Assert<Mirrors<RustGraphLink, GraphLink>>;
export type CommitRowStillMirrorsRust = Assert<Mirrors<RustCommitRow, CommitRow>>;
export type FoldedBranchStillMirrorsRust = Assert<Mirrors<RustFoldedBranch, FoldedBranch>>;
export type CommitGraphStillMirrorsRust = Assert<Mirrors<RustCommitGraph, CommitGraph>>;
