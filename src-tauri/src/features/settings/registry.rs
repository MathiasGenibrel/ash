use std::sync::Mutex;

use super::error::SettingsError;
use super::tool::{NewTool, ToolDeclaration};

/// Les commandes déclarées, et les adaptateurs qu'on peut leur donner.
///
/// **Le registre est en Rust, et pas dans un état de la webview**, pour la même raison que
/// les onglets et le thème : le frontend rend ce que le backend détient
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ici la raison est
/// même plus forte qu'ailleurs — ces déclarations sont ce qui fera reconnaître un agent
/// dans la sidebar (ADR-0006) et ce qui décidera où poser des hooks (ADR-0007) ; ni l'une
/// ni l'autre de ces lectures ne passe par la fenêtre de réglages.
///
/// **Rien n'est encore lu ni écrit dans `~/.ash/config.toml`.** Ce n'est pas un oubli :
/// une entrée n'y est écrite qu'une fois **vérifiée** (spec §9.1), et la vérification est
/// l'issue #15. Tant qu'elle n'existe pas, aucune entrée ne peut atteindre le fichier, et
/// un registre en mémoire dit exactement la vérité du produit — la persistance arrivera
/// avec ce qui la déclenche.
///
/// La liste des adaptateurs est **injectée** : ce sont ceux que la composition root
/// assemble, et la feature n'a pas à connaître leurs implémentations
/// ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)).
pub struct ToolRegistry {
    adapters: Vec<String>,
    tools: Mutex<Vec<ToolDeclaration>>,
}

impl ToolRegistry {
    pub fn new(adapters: Vec<String>) -> Self {
        Self {
            adapters,
            tools: Mutex::new(Vec::new()),
        }
    }

    /// Les adaptateurs proposés par le formulaire d'ajout.
    pub fn adapters(&self) -> &[String] {
        &self.adapters
    }

    pub fn tools(&self) -> Result<Vec<ToolDeclaration>, SettingsError> {
        Ok(self.lock()?.clone())
    }

    /// Retient une saisie, ou dit pourquoi elle n'en est pas une.
    ///
    /// Rend la liste entière plutôt que l'entrée ajoutée : le frontend redessine à partir
    /// de ce que le backend détient, il ne recompose pas sa propre liste à côté.
    pub fn declare(&self, draft: NewTool) -> Result<Vec<ToolDeclaration>, SettingsError> {
        let mut tools = self.lock()?;
        // Le verrou est tenu pendant le jugement **et** l'ajout : deux ajouts concurrents
        // de la même commande passeraient tous les deux si l'unicité était vérifiée en
        // dehors, et le registre porterait alors deux entrées homonymes.
        let declared = draft.declare(&self.adapters, &tools)?;
        tools.push(declared);
        Ok(tools.clone())
    }

    /// Oublie une entrée — le `✕` de l'en-tête de carte.
    ///
    /// C'est aussi ce qui rend l'état vide atteignable autrement qu'au premier démarrage :
    /// une liste où l'on ajoute sans pouvoir revenir en arrière ferait d'une faute de
    /// frappe une entrée définitive.
    pub fn forget(&self, command: &str) -> Result<Vec<ToolDeclaration>, SettingsError> {
        let mut tools = self.lock()?;
        let before = tools.len();
        tools.retain(|tool| tool.command != command);
        if tools.len() == before {
            return Err(SettingsError::UnknownTool(command.to_owned()));
        }
        Ok(tools.clone())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Vec<ToolDeclaration>>, SettingsError> {
        self.tools.lock().map_err(|_| SettingsError::Poisoned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ToolRegistry {
        ToolRegistry::new(vec!["generic".to_owned(), "claude-code".to_owned()])
    }

    fn draft(command: &str) -> NewTool {
        NewTool {
            command: command.to_owned(),
            label: None,
            adapter: "generic".to_owned(),
            config: None,
        }
    }

    #[test]
    fn given_an_empty_registry_when_a_command_is_declared_then_the_list_carries_it() {
        // Given — l'état vide de la maquette : `none` déclaré
        let registry = registry();
        assert_eq!(registry.tools().unwrap(), vec![]);

        // When
        let after = registry.declare(draft("claude")).unwrap();

        // Then
        assert_eq!(
            after
                .iter()
                .map(|tool| tool.command.as_str())
                .collect::<Vec<_>>(),
            vec!["claude"]
        );
    }

    #[test]
    fn given_a_declared_command_when_the_same_one_is_declared_again_then_the_list_is_unchanged() {
        // Given — un refus ne doit pas laisser le registre à moitié modifié
        let registry = registry();
        registry.declare(draft("claude")).unwrap();

        // When
        let refused = registry.declare(draft("claude"));

        // Then
        assert!(refused.is_err());
        assert_eq!(registry.tools().unwrap().len(), 1);
    }

    #[test]
    fn given_the_only_declared_command_when_it_is_forgotten_then_the_list_is_empty_again() {
        // Given — c'est ce qui rend l'état vide atteignable après un ajout
        let registry = registry();
        registry.declare(draft("claude")).unwrap();

        // When
        let after = registry.forget("claude").unwrap();

        // Then
        assert_eq!(after, vec![]);
    }

    #[test]
    fn given_a_command_that_was_never_declared_when_it_is_forgotten_then_it_is_refused() {
        // Given — se taire ferait croire à une suppression qui n'a pas eu lieu
        let registry = registry();

        // When
        let forgotten = registry.forget("codex");

        // Then
        assert_eq!(
            forgotten.unwrap_err(),
            SettingsError::UnknownTool("codex".to_owned())
        );
    }
}
