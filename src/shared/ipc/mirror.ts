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
import type { Instrumented as RustInstrumented } from "./generated/Instrumented";
import type { MetadataChanged as RustMetadataChanged } from "./generated/MetadataChanged";
import type { Operation as RustOperation } from "./generated/Operation";
import type { PinnedRepo as RustPinnedRepo } from "./generated/PinnedRepo";
import type { PinnedWorktree as RustPinnedWorktree } from "./generated/PinnedWorktree";
import type { OperationKind as RustOperationKind } from "./generated/OperationKind";
import type { Progress as RustProgress } from "./generated/Progress";
import type { RecognizedAgent as RustRecognizedAgent } from "./generated/RecognizedAgent";
import type { RepoRef as RustRepoRef } from "./generated/RepoRef";
import type { Status as RustStatus } from "./generated/Status";
import type { Subagent as RustSubagent } from "./generated/Subagent";
import type { TabInfo as RustTabInfo } from "./generated/TabInfo";
import type { TabLocation as RustTabLocation } from "./generated/TabLocation";
import type { TreeStatus as RustTreeStatus } from "./generated/TreeStatus";
import type { Upstream as RustUpstream } from "./generated/Upstream";
import type { WorktreeMetadata as RustWorktreeMetadata } from "./generated/WorktreeMetadata";
import type { Workspaces as RustWorkspaces } from "./generated/Workspaces";
import type {
    AgentState,
    Instrumented,
    GitHead,
    GitOperation,
    GitOperationKind,
    GitProgress,
    GitStatus,
    GitTreeStatus,
    GitUpstream,
    RecognizedAgent,
    RepoRef,
    Subagent,
    TabInfo,
    TabLocation,
    Workspaces,
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

/**
 * Les trois mots de la reconnaissance d'ADR-0006. Un mot ajouté en Rust — un quatrième cas
 * d'instrumentation — ne compile plus tant que la sidebar n'a pas dit ce qu'elle en montre.
 */
export type InstrumentedStillMirrorsRust = Assert<Mirrors<RustInstrumented, Instrumented>>;
export type RecognizedAgentStillMirrorsRust = Assert<
    Mirrors<RustRecognizedAgent, RecognizedAgent>
>;
/**
 * Les épingles (spec §5.2). `RepoRef` et `TabLocation` sont écrits **une** fois côté
 * TypeScript et confrontés chacun à deux `struct` Rust — celles de `pty` et celles de
 * `workspaces` — parce que la sidebar range une ligne de la même façon d'où qu'elle vienne.
 * Le jour où les deux backends divergeraient, c'est ici que ça se verrait, et non à
 * l'exécution.
 */
export type PinnedRepoStillMirrorsRust = Assert<Mirrors<RustPinnedRepo, RepoRef>>;
export type PinnedWorktreeStillMirrorsRust = Assert<Mirrors<RustPinnedWorktree, TabLocation>>;
export type WorkspacesStillMirrorsRust = Assert<Mirrors<RustWorkspaces, Workspaces>>;

export type TabLocationStillMirrorsRust = Assert<Mirrors<RustTabLocation, TabLocation>>;
export type SubagentStillMirrorsRust = Assert<Mirrors<RustSubagent, Subagent>>;
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
