/**
 * Ce qui tient `contract.ts` collé aux `struct` de `src-tauri/src/features/settings/`.
 *
 * Même dispositif que `shared/ipc/mirror.ts`, et il est ici plutôt que là-bas pour la même
 * raison que le contrat lui-même : ces formes n'ont qu'un lecteur, la fenêtre de réglages.
 *
 * C'est la feature qui a coûté #16 — l'event du second temps ne portait pas la ligne
 * `hooks`, et la fenêtre gardait celle du premier temps, donc un bouton `install` allumé
 * sur une entrée que le backend refusait. `Verified` est le type qui portait ce trou ; il
 * est vérifié plus bas comme les autres.
 */

import type { FixAction as RustFixAction } from "@/shared/ipc/generated/FixAction";
import type { FocusedTool as RustFocusedTool } from "@/shared/ipc/generated/FocusedTool";
import type { FontStep as RustFontStep } from "@/shared/ipc/generated/FontStep";
import type { HookAction as RustHookAction } from "@/shared/ipc/generated/HookAction";
import type { HooksReport as RustHooksReport } from "@/shared/ipc/generated/HooksReport";
import type { HookState as RustHookState } from "@/shared/ipc/generated/HookState";
import type { Mismatch as RustMismatch } from "@/shared/ipc/generated/Mismatch";
import type { NewTool as RustNewTool } from "@/shared/ipc/generated/NewTool";
import type { NotificationPermission as RustNotificationPermission } from "@/shared/ipc/generated/NotificationPermission";
import type { NotificationsReport as RustNotificationsReport } from "@/shared/ipc/generated/NotificationsReport";
import type { NotificationSwitch as RustNotificationSwitch } from "@/shared/ipc/generated/NotificationSwitch";
import type { Outcome as RustOutcome } from "@/shared/ipc/generated/Outcome";
import type { PlannedRemoval as RustPlannedRemoval } from "@/shared/ipc/generated/PlannedRemoval";
import type { RemovalOutcome as RustRemovalOutcome } from "@/shared/ipc/generated/RemovalOutcome";
import type { RemovalPlan as RustRemovalPlan } from "@/shared/ipc/generated/RemovalPlan";
import type { RemovalReport as RustRemovalReport } from "@/shared/ipc/generated/RemovalReport";
import type { RemovedFile as RustRemovedFile } from "@/shared/ipc/generated/RemovedFile";
import type { SettingsSnapshot as RustSettingsSnapshot } from "@/shared/ipc/generated/SettingsSnapshot";
import type { CapturePreview as RustCapturePreview } from "@/shared/ipc/generated/CapturePreview";
import type { ConflictChoice as RustConflictChoice } from "@/shared/ipc/generated/ConflictChoice";
import type { JournalReport as RustJournalReport } from "@/shared/ipc/generated/JournalReport";
import type { KeyStroke as RustKeyStroke } from "@/shared/ipc/generated/KeyStroke";
import type { Reservation as RustReservation } from "@/shared/ipc/generated/Reservation";
import type { ReservedBy as RustReservedBy } from "@/shared/ipc/generated/ReservedBy";
import type { ShortcutConflict as RustShortcutConflict } from "@/shared/ipc/generated/ShortcutConflict";
import type { ShortcutRow as RustShortcutRow } from "@/shared/ipc/generated/ShortcutRow";
import type { ShortcutsReport as RustShortcutsReport } from "@/shared/ipc/generated/ShortcutsReport";
import type { SidebarDensity as RustSidebarDensity } from "@/shared/ipc/generated/SidebarDensity";
import type { SuggestedFix as RustSuggestedFix } from "@/shared/ipc/generated/SuggestedFix";
import type { ThemeMode as RustThemeMode } from "@/shared/ipc/generated/ThemeMode";
import type { TestDescription as RustTestDescription } from "@/shared/ipc/generated/TestDescription";
import type { TestOutcome as RustTestOutcome } from "@/shared/ipc/generated/TestOutcome";
import type { ToolDeclaration as RustToolDeclaration } from "@/shared/ipc/generated/ToolDeclaration";
import type { ToolSuggestion as RustToolSuggestion } from "@/shared/ipc/generated/ToolSuggestion";
import type { Verification as RustVerification } from "@/shared/ipc/generated/Verification";
import type { VerificationState as RustVerificationState } from "@/shared/ipc/generated/VerificationState";
import type { Verified as RustVerified } from "@/shared/ipc/generated/Verified";
import type { Readability as RustReadability } from "@/shared/ipc/generated/Readability";
import type { UsageReport as RustUsageReport } from "@/shared/ipc/generated/UsageReport";
import type { Accepts, Assert, Mirrors } from "@/shared/ipc/mirroring";

import type {
    FixAction,
    FocusedTool,
    FontStep,
    HookAction,
    HooksReport,
    HookState,
    Mismatch,
    NotificationPermission,
    JournalReport,
    UsageReadability,
    UsageReport,
    NotificationsReport,
    NotificationSwitch,
    Outcome,
    PlannedRemoval,
    RemovalOutcome,
    RemovalPlan,
    RemovalReport,
    RemovedFile,
    CapturePreview,
    ConflictChoice,
    KeyStroke,
    Reservation,
    ReservedBy,
    SettingsSnapshot,
    ShortcutConflict,
    ShortcutRow,
    ShortcutsReport,
    SidebarDensity,
    SuggestedFix,
    TestDescription,
    TestOutcome,
    ThemeMode,
    ToolDeclaration,
    ToolDraft,
    ToolSuggestion,
    Verification,
    VerificationState,
    Verified,
} from "./contract";

/** L'outil que la sidebar désigne — une demande d'affichage, jamais une écriture. */
export type FocusedToolStillMirrorsRust = Assert<Mirrors<RustFocusedTool, FocusedTool>>;

/** L'outil qu'Ash a vu tourner et qu'un clic déclare — il ne porte aucun geste. */
export type ToolSuggestionStillMirrorsRust = Assert<Mirrors<RustToolSuggestion, ToolSuggestion>>;

export type HookStateStillMirrorsRust = Assert<Mirrors<RustHookState, HookState>>;
export type HookActionStillMirrorsRust = Assert<Mirrors<RustHookAction, HookAction>>;
export type HooksReportStillMirrorsRust = Assert<Mirrors<RustHooksReport, HooksReport>>;

export type TestOutcomeStillMirrorsRust = Assert<Mirrors<RustTestOutcome, TestOutcome>>;
export type VerificationStateStillMirrorsRust = Assert<
    Mirrors<RustVerificationState, VerificationState>
>;
export type MismatchStillMirrorsRust = Assert<Mirrors<RustMismatch, Mismatch>>;
export type FixActionStillMirrorsRust = Assert<Mirrors<RustFixAction, FixAction>>;
export type SuggestedFixStillMirrorsRust = Assert<Mirrors<RustSuggestedFix, SuggestedFix>>;
export type VerificationStillMirrorsRust = Assert<Mirrors<RustVerification, Verification>>;

export type TestDescriptionStillMirrorsRust = Assert<Mirrors<RustTestDescription, TestDescription>>;
export type ToolDeclarationStillMirrorsRust = Assert<Mirrors<RustToolDeclaration, ToolDeclaration>>;
export type SettingsSnapshotStillMirrorsRust = Assert<
    Mirrors<RustSettingsSnapshot, SettingsSnapshot>
>;

/**
 * La section `notifications` (spec §8), et l'autorisation qu'elle affiche.
 *
 * Elle est vérifiée comme le reste, et pas seulement par prudence : `switches` porte les
 * états qui peuvent interrompre **et** leur position, et un interrupteur ajouté ou retiré
 * côté `agents` doit se voir ici plutôt qu'à l'écran.
 */
export type NotificationPermissionStillMirrorsRust = Assert<
    Mirrors<RustNotificationPermission, NotificationPermission>
>;
export type NotificationsReportStillMirrorsRust = Assert<
    Mirrors<RustNotificationsReport, NotificationsReport>
>;
export type NotificationSwitchStillMirrorsRust = Assert<
    Mirrors<RustNotificationSwitch, NotificationSwitch>
>;

/**
 * Le journal d'attribution (ADR-0014), et pourquoi sa forme est vérifiée comme les autres.
 *
 * Aucune autre commande ne la renvoie, donc un champ perdu en route ne se verrait nulle
 * part ailleurs — et le champ qui compte est celui qui porte la promesse de la spec §10 sur
 * des prompts.
 */
export type JournalReportStillMirrorsRust = Assert<Mirrors<RustJournalReport, JournalReport>>;

/**
 * « Retirer ash de tous les fichiers » (spec §10), dans ses deux temps.
 *
 * Les deux formes sont vérifiées pour la raison qui a coûté #16 : ce sont les seules du
 * contrat qu'aucune autre commande ne renvoie, donc un champ perdu en route ne se verrait
 * qu'au moment où quelqu'un désinstalle — c'est-à-dire trop tard pour l'apprendre.
 */
export type PlannedRemovalStillMirrorsRust = Assert<Mirrors<RustPlannedRemoval, PlannedRemoval>>;
export type RemovalPlanStillMirrorsRust = Assert<Mirrors<RustRemovalPlan, RemovalPlan>>;
export type OutcomeStillMirrorsRust = Assert<Mirrors<RustOutcome, Outcome>>;
export type RemovedFileStillMirrorsRust = Assert<Mirrors<RustRemovedFile, RemovedFile>>;
export type RemovalReportStillMirrorsRust = Assert<Mirrors<RustRemovalReport, RemovalReport>>;
export type RemovalOutcomeStillMirrorsRust = Assert<Mirrors<RustRemovalOutcome, RemovalOutcome>>;

/** Ce que l'event `ash://settings-verified` porte — la ligne `hooks` comprise (#16). */
export type VerifiedStillMirrorsRust = Assert<Mirrors<RustVerified, Verified>>;

/**
 * Les sections `appearance` et `shortcuts` (#110).
 *
 * Les trois formes sont **détenues ailleurs** — `features::theme` pour les deux premières,
 * `src-tauri/src/menu.rs` pour la troisième — et c'est précisément ce qui les rend fragiles :
 * un mode renommé, un pas de police renommé ou un champ de raccourci disparu ne casserait
 * rien à la compilation, et l'écran montrerait un choix qui n'agit plus ou une liste vide.
 * `ThemeMode` est vérifié deux fois dans le dépôt, ici et dans `src/app/mirror.ts` : les deux
 * copies TypeScript existent pour que la feature n'importe pas le composition root, donc les
 * deux ont besoin du même filet.
 */
export type SettingsThemeModeStillMirrorsRust = Assert<Mirrors<RustThemeMode, ThemeMode>>;
export type FontStepStillMirrorsRust = Assert<Mirrors<RustFontStep, FontStep>>;
export type ShortcutRowStillMirrorsRust = Assert<Mirrors<RustShortcutRow, ShortcutRow>>;
export type ShortcutsReportStillMirrorsRust = Assert<Mirrors<RustShortcutsReport, ShortcutsReport>>;
export type ShortcutConflictStillMirrorsRust = Assert<
    Mirrors<RustShortcutConflict, ShortcutConflict>
>;
export type ReservationStillMirrorsRust = Assert<Mirrors<RustReservation, Reservation>>;
export type ReservedByStillMirrorsRust = Assert<Mirrors<RustReservedBy, ReservedBy>>;
export type CapturePreviewStillMirrorsRust = Assert<Mirrors<RustCapturePreview, CapturePreview>>;

/**
 * Les deux formes que la fenêtre **envoie** au sujet des raccourcis.
 *
 * Elles vont dans l'autre sens, donc `Accepts` : une frappe et une issue de conflit sont ce
 * que le backend doit savoir lire. Un code de touche renommé d'un côté seulement ferait
 * refuser toutes les captures, à l'exécution et sans message clair.
 */
export type RustAcceptsKeyStroke = Assert<Accepts<RustKeyStroke, KeyStroke>>;
export type RustAcceptsConflictChoice = Assert<Accepts<RustConflictChoice, ConflictChoice>>;
/**
 * La densité de la sidebar, arrivée avec l'aperçu du thème (#22).
 *
 * Elle est vérifiée ici **et** dans `src/app/mirror.ts`, comme `ThemeMode` et pour la même
 * raison : les deux copies TypeScript existent pour que la feature n'importe pas le
 * composition root, donc les deux ont besoin du même filet.
 */
export type SettingsSidebarDensityStillMirrorsRust = Assert<
    Mirrors<RustSidebarDensity, SidebarDensity>
>;

/**
 * La seule forme qui va dans l'autre sens : le formulaire d'ajout **envoie** une saisie.
 *
 * Une seule direction, donc, et c'est le bon sens : `ToolDraft` n'a que du texte là où
 * `NewTool` accepte aussi l'absence (`#[serde(default)]` sur `label` et `config`). Exiger
 * l'égalité forcerait le formulaire à porter un `| null` qu'un champ de saisie ne produit
 * jamais — la fenêtre rend une chaîne vide, et c'est le Rust qui décide qu'une chaîne vide
 * vaut « absent ».
 */
export type ToolDraftIsAcceptedByRust = Assert<Accepts<RustNewTool, ToolDraft>>;

/**
 * La section `usage` (ADR-0016, condition 3 ; ADR-0017, conséquences).
 *
 * Les deux assertions qui portent le ticket sont `Readability` et `endpoint`. La première
 * cesserait de compiler le jour où quelqu'un fondrait « refusé », « absent » et « illisible »
 * en un seul mot — c'est-à-dire au moment précis où la fenêtre redeviendrait incapable de
 * dire *laquelle* des trois s'applique, ce que les conséquences d'ADR-0017 lui demandent
 * nommément. La seconde tient l'adresse affichée à celle que le code appelle : une chaîne
 * recopiée à la main finirait par mentir sur ce qui sort de la machine.
 */
export type UsageReadabilityStillMirrorsRust = Assert<Mirrors<RustReadability, UsageReadability>>;

export type UsageReportStillMirrorsRust = Assert<Mirrors<RustUsageReport, UsageReport>>;
