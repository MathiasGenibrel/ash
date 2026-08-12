//! Les refus de la feature `settings` — et **la langue dans laquelle ils se lisent**.
//!
//! Ces messages ne sont pas des messages de développeur : ils ressemblent à des messages de
//! développeur, et c'est exactement ce qui les a fait dériver. La fenêtre de réglages affiche
//! le refus **mot pour mot**, dans la barre d'action de son formulaire et sous le bouton
//! d'une carte — à côté de `installed`, `missing`, `block edited by hand`, `add` et
//! `re-verify all`. Un refus en français y met deux langues dans le même écran, sur la ligne
//! qui explique pourquoi Ash n'agit pas.
//!
//! # Deux surfaces, deux règles
//!
//! | surface | qui la lit | règle |
//! |---|---|---|
//! | [`Display`](fmt::Display), et la sérialisation qui en découle | la fenêtre, **verbatim** | **texte d'interface : en anglais**, minuscule, sans point final, comme tout le reste de la fenêtre |
//! | [`Debug`] | une sortie d'erreur, un rapport de panique, l'échec d'un test | le nom de la variante et ses données — **jamais traduit, et jamais montré** |
//!
//! La question qui évite la rechute n'est donc pas « est-ce une erreur ? » — elles le sont
//! toutes — mais **qui lit cette phrase-là**. Le `eprintln!` de
//! [`commands::open`](super::commands) parle français sur la sortie d'erreur,
//! [`HooksReport`](super::hooks::HooksReport) parle anglais à l'écran, et les deux ont
//! raison : ce fichier était le seul endroit où ils se confondaient. La même question reste
//! ouverte pour `PtyError` et `GitError`, restés en français : elle se tranche message par
//! message, selon qu'il atteint la fenêtre ou meurt dans un journal — pas par une traduction
//! en bloc.
//!
//! # Une variante ou une phrase ?
//!
//! Ces phrases sont du **texte d'interface écrit en Rust**, et c'est la règle du dépôt, pas
//! une exception : [`verification`](super::verification) y compose `nothing at {raw}` et
//! `use the {other} adapter instead?`, `HooksReport` y compose `installed · v1`, et les
//! libellés de [`TestDescription`](super::commands::TestDescription) « voyagent du backend
//! vers l'écran ».
//!
//! Le partage est net, et c'est lui qui décide : **une variante quand l'écran doit trancher,
//! une phrase quand il ne fait que lire**. [`HookState`](super::hooks::HookState) est une
//! variante parce que la fenêtre en tire une forme d'icône,
//! [`HookAction`](super::hooks::HookAction) parce qu'elle en tire le mot d'un bouton ; un
//! refus, lui, n'est qu'affiché. Quand les deux sont nécessaires, ils voyagent **côte à
//! côte** plutôt que l'un à la place de l'autre — c'est
//! [`SuggestedFix`](super::verification::SuggestedFix), dont la `question` est lue et l'`apply`
//! est agi. Le jour où la fenêtre devrait agir sur une raison précise, c'est cette forme-là
//! qu'il faudrait donner à un refus, et non remplacer la phrase par un code.
//!
//! Deux de ces phrases ont un **jumeau littéral** dans `model.ts`, que le frontend prononce
//! quand il refuse sans appeler le backend
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md) : la règle est en Rust,
//! l'écran n'en garde qu'une copie pour ce qu'il peut trancher seul). Les deux arbres sont
//! marqués dans [`Display`](fmt::Display) — rien ne les fait tomber ensemble, seul le
//! commentaire relie les deux côtés.

use std::fmt;

use super::values::Command;

/// Erreurs de la feature `settings`.
///
/// Un type par feature, comme `PtyError` : le frontend n'a pas à distinguer une erreur de
/// PTY d'un refus de déclaration, et le cœur n'a pas à connaître les erreurs du système.
///
/// Chaque variante porte **ce qui a été refusé**, pas la phrase qui le dit : la phrase est le
/// travail de [`Display`](fmt::Display), et elle suit la règle du module.
#[derive(Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// Une entrée sans commande ne peut être associée à aucun processus (ADR-0006).
    EmptyCommand,
    /// Une commande est un **nom de processus**, pas une ligne de commande ni un chemin :
    /// c'est ce que la sonde d'ADR-0005 lit dans `tcgetpgrp`, et rien d'autre ne peut
    /// jamais correspondre.
    NotACommandName(String),
    /// `match` est la clé de la spec §9 : deux entrées homonymes désigneraient le même
    /// processus, et Ash ne saurait laquelle instrumenter.
    DuplicateCommand(Command),
    /// L'adaptateur nommé n'est pas de ceux que cette version d'Ash embarque
    /// ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)).
    UnknownAdapter(String),
    /// L'entrée à oublier n'est pas — ou n'est plus — déclarée.
    UnknownTool(Command),
    /// L'entrée n'a jamais été valide : la réinitialisation n'a rien où revenir (spec §9.1).
    ///
    /// **Sa phrase dit « never been verified », et c'est inexact** : le déclencheur est
    /// `last_valid_config.is_none()`, que [`ToolDeclaration::verified_by`](super::tool::ToolDeclaration::verified_by)
    /// ne remplit que sur `Valid` — une entrée vérifiée et **invalide** tombe donc ici tout
    /// en ayant été vérifiée. L'écran n'y mène pas de lui-même : `describeReset` éteint le
    /// `↺` avec sa propre phrase, exacte celle-là — `no verified folder to go back to yet`.
    /// Corriger le mot change le sens du refus, donc appartient à sa propre tâche.
    NothingToRestore(Command),
    /// Rien n'a été réinitialisé, donc rien à annuler.
    NothingToUndo(Command),
    /// L'entrée ne désigne aucun dossier, et son adaptateur n'en propose pas.
    NoConfigFolder(Command),
    /// Ash a refusé d'écrire, et dit pourquoi.
    ///
    /// C'est le refus le plus important du produit : il porte la phrase que la ligne `hooks`
    /// affichait déjà, parce que le backend refuse pour **la même raison** qui avait éteint
    /// le bouton ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
    ///
    /// La phrase est donc composée par [`super::hooks::report`] — en anglais, comme tout
    /// [`HooksReport`](super::hooks::HooksReport) — et traverse ici **sans être retouchée** :
    /// la reformuler ferait lire un second refus à côté de celui que la ligne annonçait.
    HooksRefused(String),
    /// Le registre a été empoisonné par la panique d'un autre fil.
    Poisoned,
}

/// La phrase que la fenêtre affiche, **en anglais** — voir la règle du module.
impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::EmptyCommand => write!(f, "an entry must name a command"),
            // Les deux jumeaux littéraux de `blockedReason`, dans
            // `src/features/settings/model.ts` : le même refus, selon qu'il est attrapé par
            // l'écran ou par le registre. Deux formulations en feraient lire deux refus
            // différents pour une seule et même saisie. Aucun test ne relie les deux côtés —
            // changer une de ces deux phrases, c'est changer aussi celle de `model.ts`.
            SettingsError::NotACommandName(command) => {
                write!(f, "{command} is not a command name")
            }
            SettingsError::DuplicateCommand(command) => {
                write!(f, "{command} is already declared")
            }
            SettingsError::UnknownAdapter(adapter) => write!(f, "unknown adapter: {adapter}"),
            SettingsError::UnknownTool(command) => write!(f, "unknown tool: {command}"),
            SettingsError::NothingToRestore(command) => write!(
                f,
                "{command} has never been verified: there is no folder to go back to"
            ),
            SettingsError::NothingToUndo(command) => {
                write!(f, "{command} was not reset")
            }
            SettingsError::NoConfigFolder(command) => {
                write!(f, "{command} points at no configuration folder")
            }
            SettingsError::HooksRefused(why) => write!(f, "{why}"),
            SettingsError::Poisoned => write!(f, "tool registry poisoned"),
        }
    }
}

impl std::error::Error for SettingsError {}

// Le frontend reçoit un message, pas une structure : il n'a aucune décision à prendre sur
// la variante — il affiche la raison à côté du bouton, comme le demande la maquette.
//
// C'est **une chaîne**, et le rester est un contrat : `invoke` rejette avec la valeur
// sérialisée telle quelle, et `index.ts` en fait `error instanceof Error ? error.message :
// String(error)`. Un objet balisé y deviendrait `[object Object]` à l'écran, sans qu'aucune
// vérification de type ne s'en aperçoive — le test ci-dessous est là pour ça.
impl serde::Serialize for SettingsError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_refusal_the_window_also_words_on_its_own_when_it_crosses_the_wire_then_it_is_the_sentence_the_window_would_have_written(
    ) {
        // Given — `model.ts` refuse `/usr/local/bin/claude` sans appeler le backend, et
        // `settings_declare_tool` le refuse aussi : c'est la même saisie, et l'utilisateur
        // ne sait pas lequel des deux a parlé
        let refused = SettingsError::NotACommandName("/usr/local/bin/claude".to_owned());

        // When — la fenêtre lit ce que `invoke` a rejeté, sans le retoucher
        let on_the_wire = serde_json::to_string(&refused).expect("un refus se sérialise");

        // Then — une chaîne, et la phrase exacte du jumeau de `blockedReason`
        assert_eq!(
            on_the_wire,
            "\"/usr/local/bin/claude is not a command name\""
        );
    }

    #[test]
    fn given_a_write_the_hooks_line_had_already_refused_when_the_backend_refuses_too_then_it_says_it_in_the_same_words(
    ) {
        // Given — la ligne `hooks` a éteint son bouton avec cette phrase-là ; le backend
        // refuse pour la même raison (ADR-0007). La reformuler ferait lire un second refus
        let refused =
            SettingsError::HooksRefused("already written by claude in this file".to_owned());

        // When
        let shown = refused.to_string();

        // Then
        assert_eq!(shown, "already written by claude in this file");
    }
}
