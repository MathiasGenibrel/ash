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
import type { SettingsSnapshot as RustSettingsSnapshot } from "@/shared/ipc/generated/SettingsSnapshot";
import type { Shortcut as RustShortcut } from "@/shared/ipc/generated/Shortcut";
import type { SuggestedFix as RustSuggestedFix } from "@/shared/ipc/generated/SuggestedFix";
import type { ThemeMode as RustThemeMode } from "@/shared/ipc/generated/ThemeMode";
import type { TestDescription as RustTestDescription } from "@/shared/ipc/generated/TestDescription";
import type { TestOutcome as RustTestOutcome } from "@/shared/ipc/generated/TestOutcome";
import type { ToolDeclaration as RustToolDeclaration } from "@/shared/ipc/generated/ToolDeclaration";
import type { Verification as RustVerification } from "@/shared/ipc/generated/Verification";
import type { VerificationState as RustVerificationState } from "@/shared/ipc/generated/VerificationState";
import type { Verified as RustVerified } from "@/shared/ipc/generated/Verified";
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
    NotificationsReport,
    SettingsSnapshot,
    Shortcut,
    SuggestedFix,
    TestDescription,
    TestOutcome,
    ThemeMode,
    ToolDeclaration,
    ToolDraft,
    Verification,
    VerificationState,
    Verified,
} from "./contract";

/** L'outil que la sidebar désigne — une demande d'affichage, jamais une écriture. */
export type FocusedToolStillMirrorsRust = Assert<Mirrors<RustFocusedTool, FocusedTool>>;

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

export type TestDescriptionStillMirrorsRust = Assert<
    Mirrors<RustTestDescription, TestDescription>
>;
export type ToolDeclarationStillMirrorsRust = Assert<
    Mirrors<RustToolDeclaration, ToolDeclaration>
>;
export type SettingsSnapshotStillMirrorsRust = Assert<
    Mirrors<RustSettingsSnapshot, SettingsSnapshot>
>;

/**
 * La section `notifications` (spec §8), et l'autorisation qu'elle affiche.
 *
 * Elle est vérifiée comme le reste, et pas seulement par prudence : `notified` porte les
 * deux états qui interrompent, et un état ajouté ou retiré côté `agents` doit se voir ici
 * plutôt qu'à l'écran.
 */
export type NotificationPermissionStillMirrorsRust = Assert<
    Mirrors<RustNotificationPermission, NotificationPermission>
>;
export type NotificationsReportStillMirrorsRust = Assert<
    Mirrors<RustNotificationsReport, NotificationsReport>
>;

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
export type ShortcutStillMirrorsRust = Assert<Mirrors<RustShortcut, Shortcut>>;

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
