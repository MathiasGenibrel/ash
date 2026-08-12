/**
 * Ce qui tient `index.ts` collé aux `struct` Rust dont il est le miroir.
 *
 * Chaque alias ci-dessous est une assertion que `tsc` évalue : à gauche le type que `ts-rs`
 * tire de la `struct`, à droite celui que `index.ts` écrit à la main. Rien n'existe à
 * l'exécution — ce fichier ne produit pas une ligne de JavaScript.
 *
 * Il vit à côté du contrat qu'il garde, et non dans un dossier de tests : c'est la feature
 * qui recopie une forme qui doit prouver qu'elle la recopie encore. Les mêmes fichiers
 * existent dans `features/settings/`, `features/terminal/` et `app/`, pour les formes que
 * ces trois-là écrivent de leur côté.
 *
 * Voir [`mirroring.ts`](./mirroring.ts) pour ce que `Mirrors` compare, et pourquoi dans les
 * deux sens.
 */

import type { AgentState as RustAgentState } from "./generated/AgentState";
import type { Head as RustHead } from "./generated/Head";
import type { MetadataChanged as RustMetadataChanged } from "./generated/MetadataChanged";
import type { Operation as RustOperation } from "./generated/Operation";
import type { OperationKind as RustOperationKind } from "./generated/OperationKind";
import type { Progress as RustProgress } from "./generated/Progress";
import type { RepoRef as RustRepoRef } from "./generated/RepoRef";
import type { Status as RustStatus } from "./generated/Status";
import type { TabInfo as RustTabInfo } from "./generated/TabInfo";
import type { TabLocation as RustTabLocation } from "./generated/TabLocation";
import type { TreeStatus as RustTreeStatus } from "./generated/TreeStatus";
import type { Upstream as RustUpstream } from "./generated/Upstream";
import type { WorktreeMetadata as RustWorktreeMetadata } from "./generated/WorktreeMetadata";
import type {
    AgentState,
    GitHead,
    GitOperation,
    GitOperationKind,
    GitProgress,
    GitStatus,
    GitTreeStatus,
    GitUpstream,
    RepoRef,
    TabInfo,
    TabLocation,
    WorktreeMetadata,
    WorktreeMetadataChanged,
} from "./index";
import type { Assert, Mirrors } from "./mirroring";

/**
 * Les cinq états. Le `match` exhaustif de `features/agents/state.rs` force à **nommer** un
 * état ajouté ; cette ligne-ci force à le reporter ici.
 */
export type AgentStateStillMirrorsRust = Assert<Mirrors<RustAgentState, AgentState>>;

export type RepoRefStillMirrorsRust = Assert<Mirrors<RustRepoRef, RepoRef>>;
export type TabLocationStillMirrorsRust = Assert<Mirrors<RustTabLocation, TabLocation>>;
export type TabInfoStillMirrorsRust = Assert<Mirrors<RustTabInfo, TabInfo>>;

export type GitHeadStillMirrorsRust = Assert<Mirrors<RustHead, GitHead>>;
export type GitOperationKindStillMirrorsRust = Assert<Mirrors<RustOperationKind, GitOperationKind>>;
export type GitProgressStillMirrorsRust = Assert<Mirrors<RustProgress, GitProgress>>;
export type GitOperationStillMirrorsRust = Assert<Mirrors<RustOperation, GitOperation>>;
export type GitTreeStatusStillMirrorsRust = Assert<Mirrors<RustTreeStatus, GitTreeStatus>>;
export type GitUpstreamStillMirrorsRust = Assert<Mirrors<RustUpstream, GitUpstream>>;
export type GitStatusStillMirrorsRust = Assert<Mirrors<RustStatus, GitStatus>>;
export type WorktreeMetadataStillMirrorsRust = Assert<
    Mirrors<RustWorktreeMetadata, WorktreeMetadata>
>;

/** Le contenu de l'event `ash://git-metadata`. */
export type WorktreeMetadataChangedStillMirrorsRust = Assert<
    Mirrors<RustMetadataChanged, WorktreeMetadataChanged>
>;
