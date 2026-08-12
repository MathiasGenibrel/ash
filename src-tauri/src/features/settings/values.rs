//! Les deux valeurs de la feature : **ce qu'une entrée nomme**, et **ce qu'elle vise**.
//!
//! Les deux portaient une règle vérifiée une fois puis oubliée : un nom de commande était
//! jugé par `NewTool::declare`, puis voyageait en `String` nu dans une douzaine de
//! signatures dont aucune ne pouvait savoir si le jugement avait eu lieu ; un dossier se
//! résolvait de trois façons — l'entrée, le défaut de l'adaptateur, le `~` étendu — et
//! quatre lecteurs dépendaient de leur accord.
//!
//! C'est le geste de [`Document`](crate::features::hooks::Document) appliqué ici : la règle
//! passe des tests à la signature. Un nom de commande ne se fabrique qu'en le validant, un
//! dossier visé ne se fabrique qu'en nommant ses deux formes — et il n'y a qu'un producteur
//! de chacun.

use std::fmt;
use std::path::{Path, PathBuf};

use super::error::SettingsError;
use super::ports::expand_home;

/// Le `match` d'un `[[command]]` : **un nom de processus**, et rien d'autre.
///
/// La règle — non vide, ni espace, ni barre oblique — n'est pas une coquetterie de saisie.
/// La sonde d'[ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md) lit le nom du
/// processus en avant-plan, et [ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)
/// le **compare** à ce que les réglages déclarent. Un chemin ou une ligne de commande ne
/// correspondrait donc jamais à rien, tout en se lisant comme une entrée valide : l'outil
/// paraîtrait déclaré et ne serait jamais reconnu.
///
/// C'est aussi la **frontière de sécurité du test 3** : le nom est ce que
/// [`CommandRunner::locate`](super::CommandRunner::locate) cherche dans le `PATH`, et un
/// chemin déguisé en nom ferait sortir la résolution du `PATH` pour exécuter un fichier
/// désigné à la main dans un champ de réglages. Le port prend un `Command`, donc la
/// question ne se repose plus à l'autre bout.
///
/// **[`Command::parse`] est le seul constructeur de production.** C'est ce qui fait qu'une
/// commande Tauri de plus — il en arrive à chaque tranche — ne peut pas passer à côté de la
/// règle sans que le compilateur le dise.
///
/// Sérialisé **transparent** : sur le fil c'est une chaîne, exactement comme avant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct Command(String);

impl Command {
    /// Juge une saisie, ou dit pourquoi elle n'est pas un nom de commande.
    ///
    /// Les espaces de tête et de queue sont retirés — les taper est une faute de frappe, pas
    /// une intention — et ce qui en reste ne doit contenir ni espace ni barre oblique.
    pub fn parse(raw: &str) -> Result<Self, SettingsError> {
        let name = raw.trim();
        if name.is_empty() {
            return Err(SettingsError::EmptyCommand);
        }
        if name.contains(char::is_whitespace) || name.contains('/') {
            return Err(SettingsError::NotACommandName(name.to_owned()));
        }
        Ok(Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Le dossier qu'une entrée vise, sous **ses deux formes**.
///
/// « Quel dossier cette entrée vise-t-elle réellement ? » se répondait de trois façons, et
/// il existe deux écritures du même dossier : celle que l'utilisateur a tapée (`~/.claude`)
/// et celle qui touche le disque (`/Users/moi/.claude`). Quatre mécanismes dépendent de
/// leur accord — la vérification, la détection de doublon, la recherche du bloc de hooks, et
/// la mémoire du dernier dossier valide. **Si l'un comparait les formes brutes et un autre
/// les formes résolues, la bannière de doublon mentirait** : `~/.claude` et
/// `/Users/moi/.claude` ne se verraient pas.
///
/// Les deux formes voyagent donc ensemble, et l'**égalité porte sur la forme résolue** :
/// deux cibles sont la même cible quand elles désignent le même dossier, quoi que
/// l'utilisateur ait tapé. Il n'y a plus qu'une notion de « même dossier » dans la feature,
/// donc plus rien à faire diverger.
///
/// **[`Verifier::target`](super::Verifier::target) est le seul producteur de production**,
/// parce qu'il est le seul à connaître le profil de l'adaptateur — donc le seul à pouvoir
/// répondre « l'entrée, sinon le défaut de l'adaptateur ».
///
/// Sérialisé comme **la forme déclarée**, une chaîne : c'est ce que la fenêtre affiche dans
/// son champ de chemin, et le contrat sur le fil ne bouge pas d'un caractère.
#[derive(Debug, Clone)]
pub struct ConfigTarget {
    declared: String,
    resolved: PathBuf,
}

impl ConfigTarget {
    /// Le dossier tel qu'il est écrit, et le dossier qu'il désigne.
    ///
    /// `pub(super)` et non `pub` : la feature entière peut le lire, un seul endroit le
    /// fabrique. Voir [`Verifier::target`](super::Verifier::target).
    pub(super) fn resolving(declared: &str, home: Option<&Path>) -> Self {
        Self {
            declared: declared.to_owned(),
            resolved: expand_home(declared, home),
        }
    }

    /// Tel que l'entrée l'écrit — ce que la fenêtre montre et ce que la mémoire restaure.
    pub fn declared(&self) -> &str {
        &self.declared
    }

    /// Tel qu'il touche le disque — `~` étendu. C'est ce qu'on lit, et où l'on écrit.
    pub fn resolved(&self) -> &Path {
        &self.resolved
    }

    /// Une cible quelconque, **réservée aux tests**.
    ///
    /// Les tests qui vérifient la mémoire ou le doublon n'ont pas de `Verifier` sous la
    /// main. La porte est `#[cfg(test)]` pour que le code de production n'en dispose à aucun
    /// moment — même discipline que `Document::verbatim`.
    #[cfg(test)]
    pub fn at(declared: &str, resolved: &str) -> Self {
        Self {
            declared: declared.to_owned(),
            resolved: PathBuf::from(resolved),
        }
    }
}

/// Deux cibles sont la même quand elles désignent le même dossier.
///
/// **Sur la forme résolue, et sur elle seule.** C'est la règle du doublon, et c'est aussi
/// celle qui dit si une réinitialisation a réellement déplacé quelque chose : les deux
/// posent la même question, et une seule réponse évite qu'elles divergent.
impl PartialEq for ConfigTarget {
    fn eq(&self, other: &Self) -> bool {
        self.resolved == other.resolved
    }
}

impl Eq for ConfigTarget {}

// Le frontend reçoit le chemin **tel qu'il est écrit** : c'est ce qu'il affiche dans son
// champ, et c'est ce que l'utilisateur reconnaît. La forme résolue ne traverse pas la
// frontière — elle n'apprendrait rien à la fenêtre, et lui donnerait de quoi recomposer une
// règle qui n'est pas la sienne (ADR-0009).
impl serde::Serialize for ConfigTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.declared)
    }
}

/// Un champ facultatif : rendu vide, il est absent — pas vide.
///
/// La différence compte pour `config` : une chaîne vide se lirait « ce dossier-là », alors
/// que l'absence veut dire « celui de l'adaptateur », que l'adaptateur est seul à savoir.
/// La règle est **ici et nulle part ailleurs** : elle était recopiée dans la déclaration,
/// dans le changement de cible et dans la vérification d'une saisie, et un `~/.claude `
/// laissé tel quel par l'une des trois désignerait un dossier que rien ne trouve.
pub(super) fn optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_command_name_that_hides_a_path_when_it_is_parsed_then_it_is_refused() {
        // Given — **frontière de sécurité** : ce nom finit dans la résolution du `PATH`, seule
        // productrice du programme que le test 4 lancera. Un chemin, une ligne de commande ou
        // un vide feraient sortir Ash du `PATH` ou ne correspondraient à aucun processus.
        // La question se posait autrefois à l'entrée de `SystemCommands::locate` ; elle se
        // pose désormais ici, parce que le port ne prend plus qu'un `Command`
        let refused = [
            "/usr/local/bin/claude",
            "../claude",
            "claude --dangerously",
            "   ",
        ];

        // When
        let parsed = refused.map(Command::parse);

        // Then
        assert!(parsed.iter().all(Result::is_err), "{parsed:?}");
    }

    #[test]
    fn given_two_entries_that_write_the_same_folder_differently_when_they_are_compared_then_they_are_the_same_target(
    ) {
        // Given — c'est exactement le cas que le doublon existe pour attraper : deux entrées
        // écrites `~/.claude` et `/Users/ash/.claude` écriraient dans le même fichier, et
        // comparer les chaînes brutes laisserait la seconde poser un bloc par-dessus le premier
        let home = Path::new("/Users/ash");
        let written_with_a_tilde = ConfigTarget::resolving("~/.claude", Some(home));
        let written_in_full = ConfigTarget::resolving("/Users/ash/.claude", Some(home));

        // When
        let same = written_with_a_tilde == written_in_full;

        // Then
        assert!(same);
        assert_eq!(written_with_a_tilde.declared(), "~/.claude");
    }

    #[test]
    fn given_a_target_written_with_a_tilde_when_it_crosses_the_wire_then_it_is_the_string_the_user_typed(
    ) {
        // Given — la fenêtre affiche ce chemin dans son champ : lui envoyer la forme résolue
        // remplacerait sous ses yeux ce qu'il a tapé, et lui donnerait de quoi recomposer
        // une règle qui n'est pas la sienne
        let target = ConfigTarget::resolving("~/.claude", Some(Path::new("/Users/ash")));

        // When
        let json = serde_json::to_string(&target).expect("une cible se sérialise");

        // Then
        assert_eq!(json, "\"~/.claude\"");
    }
}
