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

import type { AccountUsage as RustAccountUsage } from "./generated/AccountUsage";
import type { Quota as RustQuota } from "./generated/Quota";
import type { AgentState as RustAgentState } from "./generated/AgentState";
import type { BranchCard as RustBranchCard } from "./generated/BranchCard";
import type { CardLog as RustCardLog } from "./generated/CardLog";
import type { CardMode as RustCardMode } from "./generated/CardMode";
import type { LogState as RustLogState } from "./generated/LogState";
import type { ActionOffer as RustActionOffer } from "./generated/ActionOffer";
import type { ActionOutcome as RustActionOutcome } from "./generated/ActionOutcome";
import type { Branch as RustBranch } from "./generated/Branch";
import type { BranchAction as RustBranchAction } from "./generated/BranchAction";
import type { BranchGroup as RustBranchGroup } from "./generated/BranchGroup";
import type { BranchKind as RustBranchKind } from "./generated/BranchKind";
import type { BranchOverview as RustBranchOverview } from "./generated/BranchOverview";
import type { BranchSection as RustBranchSection } from "./generated/BranchSection";
import type { BranchWorktree as RustBranchWorktree } from "./generated/BranchWorktree";
import type { BusyAgent as RustBusyAgent } from "./generated/BusyAgent";
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
import type { ComposeOutcome as RustComposeOutcome } from "./generated/ComposeOutcome";
import type { Status as RustStatus } from "./generated/Status";
import type { StoppedCommit as RustStoppedCommit } from "./generated/StoppedCommit";
import type { StoppedOperation as RustStoppedOperation } from "./generated/StoppedOperation";
import type { SessionUsage as RustSessionUsage } from "./generated/SessionUsage";
import type { Subagent as RustSubagent } from "./generated/Subagent";
import type { TabInfo as RustTabInfo } from "./generated/TabInfo";
import type { TabLocation as RustTabLocation } from "./generated/TabLocation";
import type { TreeStatus as RustTreeStatus } from "./generated/TreeStatus";
import type { Upstream as RustUpstream } from "./generated/Upstream";
import type { WorktreeMetadata as RustWorktreeMetadata } from "./generated/WorktreeMetadata";
import type { SidebarRows as RustSidebarRows } from "./generated/SidebarRows";
import type { ConflictFile as RustConflictFile } from "./generated/ConflictFile";
import type { Hunk as RustHunk } from "./generated/Hunk";
import type { MergeOutcome as RustMergeOutcome } from "./generated/MergeOutcome";
import type { MergeSides as RustMergeSides } from "./generated/MergeSides";
import type { MergeTab as RustMergeTab } from "./generated/MergeTab";
import type { MergeView as RustMergeView } from "./generated/MergeView";
import type { SideLabel as RustSideLabel } from "./generated/SideLabel";
import type { StoppedView as RustStoppedView } from "./generated/StoppedView";
import type { Tab as RustTab } from "./generated/Tab";
import type { LastWork as RustLastWork } from "./generated/LastWork";
import type { WorktreeRemoval as RustWorktreeRemoval } from "./generated/WorktreeRemoval";
import type { RepoLine as RustRepoLine } from "./generated/RepoLine";
import type { WorkSource as RustWorkSource } from "./generated/WorkSource";
import type { WorktreeAgent as RustWorktreeAgent } from "./generated/WorktreeAgent";
import type { WorktreeRow as RustWorktreeRow } from "./generated/WorktreeRow";
import type {
    AccountUsage,
    ActionOffer,
    ActionOutcome,
    AgentState,
    BranchCard,
    CardLog,
    CardLogState,
    CardMode,
    Branch,
    BranchAction,
    BranchGroup,
    BranchKind,
    BranchOverview,
    BranchSection,
    BranchWorktree,
    BusyAgent,
    ComposeOutcome,
    Instrumented,
    GitHead,
    GitOperation,
    GitOperationKind,
    GitProgress,
    GitStatus,
    GitTreeStatus,
    GitUpstream,
    Quota,
    RecognizedAgent,
    RepoRef,
    StoppedCommit,
    StoppedOperation,
    SessionUsage,
    Subagent,
    ConflictFile,
    MergeHunk,
    MergeOutcome,
    MergeSides,
    MergeStopped,
    MergeTab,
    MergeView,
    SideLabel,
    Tab,
    TabInfo,
    TabLocation,
    SidebarRows,
    LastWork,
    WorktreeRemoval,
    WorkSource,
    WorktreeAgent,
    WorktreeMetadata,
    WorktreeMetadataChanged,
    WorktreeRepo,
    WorktreeRow,
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
 * `sidebar` — parce que la colonne range une ligne de la même façon d'où qu'elle vienne.
 * Le jour où les deux backends divergeraient, c'est ici que ça se verrait, et non à
 * l'exécution.
 */
export type PinnedRepoStillMirrorsRust = Assert<Mirrors<RustPinnedRepo, RepoRef>>;
export type PinnedWorktreeStillMirrorsRust = Assert<Mirrors<RustPinnedWorktree, TabLocation>>;
export type SidebarRowsStillMirrorsRust = Assert<Mirrors<RustSidebarRows, SidebarRows>>;

export type TabLocationStillMirrorsRust = Assert<Mirrors<RustTabLocation, TabLocation>>;
export type SubagentStillMirrorsRust = Assert<Mirrors<RustSubagent, Subagent>>;
export type SessionUsageStillMirrorsRust = Assert<Mirrors<RustSessionUsage, SessionUsage>>;
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

/**
 * La popup de branches (spec §7.1).
 *
 * Les deux assertions qui portent le ticket sont `BranchWorktree` — la colonne qui nomme le
 * worktree quand la branche vit ailleurs — et `BusyAgent` : un champ perdu là, et
 * l'avertissement cesserait de **nommer** l'agent pour se contenter de compter.
 */
export type BranchKindStillMirrorsRust = Assert<Mirrors<RustBranchKind, BranchKind>>;
export type BranchWorktreeStillMirrorsRust = Assert<Mirrors<RustBranchWorktree, BranchWorktree>>;
export type BranchStillMirrorsRust = Assert<Mirrors<RustBranch, Branch>>;
export type BranchGroupStillMirrorsRust = Assert<Mirrors<RustBranchGroup, BranchGroup>>;
export type BranchSectionStillMirrorsRust = Assert<Mirrors<RustBranchSection, BranchSection>>;
export type BusyAgentStillMirrorsRust = Assert<Mirrors<RustBusyAgent, BusyAgent>>;
export type BranchOverviewStillMirrorsRust = Assert<Mirrors<RustBranchOverview, BranchOverview>>;
export type BranchActionStillMirrorsRust = Assert<Mirrors<RustBranchAction, BranchAction>>;
export type ActionOfferStillMirrorsRust = Assert<Mirrors<RustActionOffer, ActionOffer>>;
export type ActionOutcomeStillMirrorsRust = Assert<Mirrors<RustActionOutcome, ActionOutcome>>;

/**
 * Le rebase arrêté de la spec §7.4, et l'issue d'une composition.
 *
 * `escapes` et `conflicts` sont des listes de **texte à montrer** : le jour où le backend
 * y mettrait autre chose — une action, un identifiant — cette ligne cesserait de compiler
 * avant que l'écran ne se mette à exécuter quoi que ce soit.
 */
export type StoppedCommitStillMirrorsRust = Assert<Mirrors<RustStoppedCommit, StoppedCommit>>;
export type StoppedOperationStillMirrorsRust = Assert<
    Mirrors<RustStoppedOperation, StoppedOperation>
>;

/**
 * Les quatre issues d'ADR-0015. Une cinquième ajoutée en Rust — un nouveau refus — ne
 * compile plus tant que la fenêtre n'a pas dit ce qu'elle en montre à l'utilisateur.
 */
export type ComposeOutcomeStillMirrorsRust = Assert<Mirrors<RustComposeOutcome, ComposeOutcome>>;

/**
 * L'onglet de merge (spec §7.4, issue #30).
 *
 * `Tab` est l'assertion qui compte : elle confronte la somme écrite à la main à
 * l'énumération étiquetée de `src-tauri/src/tabs.rs`. Le jour où un troisième genre
 * d'onglet naîtrait côté Rust — ADR-0003 laisse la porte ouverte aux surfaces d'outil —,
 * c'est cette ligne qui refuserait de compiler avant que la webview ne dise ce qu'elle en
 * montre.
 */
export type SideLabelStillMirrorsRust = Assert<Mirrors<RustSideLabel, SideLabel>>;
export type MergeSidesStillMirrorsRust = Assert<Mirrors<RustMergeSides, MergeSides>>;
export type MergeHunkStillMirrorsRust = Assert<Mirrors<RustHunk, MergeHunk>>;
export type ConflictFileStillMirrorsRust = Assert<Mirrors<RustConflictFile, ConflictFile>>;
export type MergeStoppedStillMirrorsRust = Assert<Mirrors<RustStoppedView, MergeStopped>>;
export type MergeViewStillMirrorsRust = Assert<Mirrors<RustMergeView, MergeView>>;
export type MergeOutcomeStillMirrorsRust = Assert<Mirrors<RustMergeOutcome, MergeOutcome>>;
export type MergeTabStillMirrorsRust = Assert<
    Mirrors<RustMergeTab, Omit<MergeTab, "kind">>
>;
export type TabStillMirrorsRust = Assert<Mirrors<RustTab, Tab>>;

/**
 * La fiche de branche d'ADR-0013, et l'état de la seule zone qu'Ash y écrit.
 *
 * Un neuvième état de bloc ajouté en Rust — un nouveau refus — ne compile plus tant que la
 * fenêtre n'a pas dit ce qu'elle en montre. C'est exactement ce qu'on veut d'un dispositif
 * dont chaque valeur signifie « Ash ne touche pas au fichier, et voici pourquoi ».
 */
export type CardModeStillMirrorsRust = Assert<Mirrors<RustCardMode, CardMode>>;
export type CardLogStateStillMirrorsRust = Assert<Mirrors<RustLogState, CardLogState>>;
export type CardLogStillMirrorsRust = Assert<Mirrors<RustCardLog, CardLog>>;
export type BranchCardStillMirrorsRust = Assert<Mirrors<RustBranchCard, BranchCard>>;

/**
 * Le tableau des worktrees (spec §7.3).
 *
 * `WorktreeRepo` est confronté au `RepoLine` du backend **et** doit rester la même forme que
 * [`RepoRef`] : c'est par cette clé que la fiche de branche (#31) parlera du même dépôt que
 * la colonne de gauche. Le jour où l'une des deux bougerait, l'une de ces deux lignes
 * cesserait de compiler.
 */
export type WorktreeRepoStillMirrorsRust = Assert<Mirrors<RustRepoLine, WorktreeRepo>>;
export type WorktreeRepoStillMatchesRepoRef = Assert<Mirrors<RepoRef, WorktreeRepo>>;
export type WorktreeAgentStillMirrorsRust = Assert<Mirrors<RustWorktreeAgent, WorktreeAgent>>;
/**
 * Les deux sources de `last worked by`. Une troisième ajoutée en Rust ne compile plus tant
 * que la fenêtre n'a pas dit ce qu'elle en montre — et cette colonne n'affirme que ce qu'Ash
 * a observé (ADR-0014).
 */
export type WorkSourceStillMirrorsRust = Assert<Mirrors<RustWorkSource, WorkSource>>;
export type LastWorkStillMirrorsRust = Assert<Mirrors<RustLastWork, LastWork>>;
export type WorktreeRowStillMirrorsRust = Assert<Mirrors<RustWorktreeRow, WorktreeRow>>;
/**
 * La fiche de suppression. `carries` et `command` sont du **texte à montrer** : le jour où le
 * backend y mettrait une action, cette ligne cesserait de compiler avant que l'écran ne se
 * mette à supprimer quoi que ce soit (ADR-0015).
 */
export type WorktreeRemovalStillMirrorsRust = Assert<Mirrors<RustWorktreeRemoval, WorktreeRemoval>>;

/**
 * Les deux quotas du compte (spec §4.2, ADR-0016).
 *
 * L'assertion qui porte le ticket est [`Quota`] : elle confronte **deux** champs, et c'est
 * le second qui compte. Le jour où quelqu'un remplacerait `resetsAt` par un décompte côté
 * Rust — `resetsIn`, en secondes, parce que c'est ce que la maquette montre —, cette ligne
 * cesserait de compiler avant que l'event ne se mette à repartir chaque seconde pour animer
 * un compteur.
 */
export type QuotaStillMirrorsRust = Assert<Mirrors<RustQuota, Quota>>;

/** Le contenu de l'event `ash://account-usage`. */
export type AccountUsageStillMirrorsRust = Assert<Mirrors<RustAccountUsage, AccountUsage>>;
