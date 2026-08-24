//! La forme de `~/.ash/tools.json` : **ce qu'un outil déclaré emporte d'une session à la
//! suivante**, et rien d'autre.
//!
//! Distincte de [`ToolDeclaration`] pour la raison qui a fait naître
//! [`Persisted`](crate::features::sidebar) du côté de la colonne : une entrée en mémoire
//! porte ce qu'elle a prouvé, ses homonymes de dossier et l'état du fichier qu'elle vise —
//! trois faits **datés sur la machine**, que relire d'un fichier ferait mentir. Ce qui est
//! écrit est donc la déclaration seule, et le dernier dossier qui a fonctionné.
//!
//! **Ce qui n'y est pas, et pourquoi :** le résultat des quatre tests de la spec §9.1. Un
//! dossier peut avoir été renommé entre deux lancements, une commande avoir quitté le
//! `PATH`, un `settings.json` avoir été édité à la main — une vérification relue serait un
//! souvenir présenté comme un fait, exactement ce qu'ADR-0007 refuse pour l'état des hooks.
//! Une entrée relue repart *non vérifiée* et se revérifie comme une entrée saisie.
//!
//! **Ce qui y est en plus de la saisie :** `last_valid_config`. Sans lui, « réinitialiser
//! une entrée » ramènerait après un redémarrage au défaut de l'adaptateur — c'est-à-dire à
//! l'entrée d'à côté (spec §9.1). C'est une mémoire, pas une vérification : elle dit où
//! l'entrée a fonctionné, pas qu'elle fonctionne.

use serde::Deserialize;

use super::tool::{NewTool, ToolDeclaration};
use super::values::ConfigTarget;

/// Le contenu du fichier : les entrées déclarées, dans leur ordre.
///
/// Un objet et non un tableau nu, comme `state.json` et `theme.json` : la première clé qui
/// s'ajoutera un jour n'obligera pas à changer la forme du fichier de tout le monde.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PersistedTools {
    /// Les entrées, telles qu'elles ont été déclarées.
    ///
    /// Chaque élément est relu **pour lui-même** ([`readable_entries`]) : une entrée qu'une
    /// main a abîmée ne fait pas perdre celles d'à côté.
    #[serde(default, deserialize_with = "readable_entries")]
    pub tools: Vec<PersistedTool>,
}

/// Une entrée du fichier — le `[[command]]` de la spec §9, en JSON.
///
/// Les champs facultatifs sont **absents** quand ils sont vides plutôt qu'écrits `null` :
/// le fichier se relit à l'œil nu et s'édite à la main (spec §9), et une clé qui ne dit
/// rien y ferait chercher ce qu'elle veut dire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PersistedTool {
    /// Le `match` : le nom du processus. C'est l'identité, et il n'y en a pas d'autre.
    pub command: String,
    /// Le libellé d'affichage — `Pro`, `Perso`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// L'identifiant de l'adaptateur.
    pub adapter: String,
    /// Le dossier de configuration, tel qu'il a été **écrit** — `~` compris.
    ///
    /// La forme résolue ne s'écrit pas : elle dépend du `$HOME` du moment, et un fichier
    /// qui la porterait rendrait la déclaration fausse sur une autre machine, ou après un
    /// changement de nom de compte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// Le dernier dossier qui a passé les quatre tests, sous la même forme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_valid_config: Option<String>,
}

impl PersistedTools {
    /// Ce que ces entrées laisseront sur le disque.
    pub fn of(tools: &[ToolDeclaration]) -> Self {
        Self {
            tools: tools.iter().map(PersistedTool::of).collect(),
        }
    }
}

impl PersistedTool {
    /// La saisie que cette entrée redonne — le chemin de retour de [`Self::of`].
    ///
    /// **Les deux sens du fichier vivent ici**, champ pour champ. La clé qui s'ajoutera un
    /// jour se lit et s'écrit alors dans le même module, au lieu d'être écrite ici et
    /// relue dans le registre : la moitié qu'on oublie est celle qu'on ne voit pas en
    /// modifiant l'autre.
    ///
    /// Elle rend une [`NewTool`] et non une déclaration, et c'est le point : une entrée
    /// relue **n'est pas** plus digne de confiance qu'une saisie du formulaire — le
    /// fichier s'édite à la main (spec §9). Ce qui la juge est
    /// [`NewTool::restore`](super::tool::NewTool::restore), avec les mêmes règles.
    pub fn draft(&self) -> NewTool {
        NewTool {
            command: self.command.clone(),
            label: self.label.clone(),
            adapter: self.adapter.clone(),
            config: self.config.clone(),
        }
    }

    fn of(tool: &ToolDeclaration) -> Self {
        Self {
            command: tool.command.as_str().to_owned(),
            label: tool.label.clone(),
            adapter: tool.adapter.clone(),
            config: tool.config.clone(),
            last_valid_config: tool
                .last_valid_config
                .as_ref()
                .map(ConfigTarget::declared)
                .map(str::to_owned),
        }
    }
}

/// Les entrées qu'on sait relire, et elles seules.
///
/// Un `Vec<PersistedTool>` ordinaire est **tout ou rien** : une entrée à qui il manque son
/// `command` ferait échouer la lecture du tableau entier, donc perdre les cinq autres. Le
/// fichier s'édite à la main (spec §9) : la faute de frappe d'une ligne ne doit pas coûter
/// les déclarations d'à côté.
fn readable_entries<'de, D>(deserializer: D) -> Result<Vec<PersistedTool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let entries = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value(entry).ok())
        .collect())
}
