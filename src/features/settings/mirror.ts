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
import type { HookAction as RustHookAction } from "@/shared/ipc/generated/HookAction";
import type { HooksReport as RustHooksReport } from "@/shared/ipc/generated/HooksReport";
import type { HookState as RustHookState } from "@/shared/ipc/generated/HookState";
import type { Mismatch as RustMismatch } from "@/shared/ipc/generated/Mismatch";
import type { NewTool as RustNewTool } from "@/shared/ipc/generated/NewTool";
import type { SettingsSnapshot as RustSettingsSnapshot } from "@/shared/ipc/generated/SettingsSnapshot";
import type { SuggestedFix as RustSuggestedFix } from "@/shared/ipc/generated/SuggestedFix";
import type { TestDescription as RustTestDescription } from "@/shared/ipc/generated/TestDescription";
import type { TestOutcome as RustTestOutcome } from "@/shared/ipc/generated/TestOutcome";
import type { ToolDeclaration as RustToolDeclaration } from "@/shared/ipc/generated/ToolDeclaration";
import type { Verification as RustVerification } from "@/shared/ipc/generated/Verification";
import type { VerificationState as RustVerificationState } from "@/shared/ipc/generated/VerificationState";
import type { Verified as RustVerified } from "@/shared/ipc/generated/Verified";
import type { Accepts, Assert, Mirrors } from "@/shared/ipc/mirroring";

import type {
    FixAction,
    HookAction,
    HooksReport,
    HookState,
    Mismatch,
    SettingsSnapshot,
    SuggestedFix,
    TestDescription,
    TestOutcome,
    ToolDeclaration,
    ToolDraft,
    Verification,
    VerificationState,
    Verified,
} from "./contract";

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

/** Ce que l'event `ash://settings-verified` porte — la ligne `hooks` comprise (#16). */
export type VerifiedStillMirrorsRust = Assert<Mirrors<RustVerified, Verified>>;

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
