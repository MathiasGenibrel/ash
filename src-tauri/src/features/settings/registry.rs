use std::sync::{Arc, Mutex};

use super::error::SettingsError;
use super::ports::Launch;
use super::tool::{NewTool, ToolDeclaration};
use super::verification::{FirstPass, Verification, Verifier};

/// Les commandes déclarées, ce qu'elles ont prouvé, et les adaptateurs qu'on peut leur
/// donner.
///
/// **Le registre est en Rust, et pas dans un état de la webview**, pour la même raison que
/// les onglets et le thème : le frontend rend ce que le backend détient
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ici la raison est
/// même plus forte qu'ailleurs — ces déclarations sont ce qui fera reconnaître un agent
/// dans la sidebar (ADR-0006) et ce qui décidera où poser des hooks (ADR-0007) ; ni l'une
/// ni l'autre de ces lectures ne passe par la fenêtre de réglages.
///
/// **La vérification y vit aussi, et c'est ce qui garde la règle d'écriture unique.** Le
/// oui/non qui autorise les hooks est calculé par [`Verifier`] et transporté tel quel ; la
/// fenêtre l'annonce sans le rejouer.
///
/// **Rien n'est encore lu ni écrit dans `~/.ash/config.toml`.** L'écriture attend d'avoir
/// un déclencheur, et c'est cette vérification qui le devient : la persistance viendra avec
/// la tâche qui la porte, pas au milieu de celle qui la débloque.
pub struct ToolRegistry {
    verifier: Arc<Verifier>,
    tools: Mutex<Vec<ToolDeclaration>>,
}

/// Ce qu'il reste à lancer après un premier temps, et pour quelle entrée.
///
/// Il porte l'adaptateur et le chemin **tels qu'ils étaient au moment du premier temps** :
/// une commande peut mettre cinq secondes à répondre, et l'utilisateur peut avoir changé de
/// chemin entre-temps. Sans cette empreinte, le résultat d'une vérification périmée
/// écraserait celle qui la remplace.
#[derive(Debug, Clone)]
pub struct SecondPass {
    pub command: String,
    pub adapter: String,
    pub config: Option<String>,
    pub launch: Launch,
    /// L'entrée est-elle au registre ? Une saisie du formulaire d'ajout n'y est pas encore,
    /// et son résultat ne doit se poser sur aucune entrée existante.
    pub stored: bool,
}

/// Ce qu'une modification du registre rend : la liste entière, et ce qui reste à lancer.
pub struct Changed {
    pub tools: Vec<ToolDeclaration>,
    pub pending: Vec<SecondPass>,
}

impl ToolRegistry {
    pub fn new(verifier: Arc<Verifier>) -> Self {
        Self {
            verifier,
            tools: Mutex::new(Vec::new()),
        }
    }

    /// Les adaptateurs proposés par le formulaire d'ajout.
    pub fn adapters(&self) -> Vec<String> {
        self.verifier.adapters()
    }

    pub fn tools(&self) -> Result<Vec<ToolDeclaration>, SettingsError> {
        Ok(self.lock()?.clone())
    }

    /// Retient une saisie, ou dit pourquoi elle n'en est pas une, puis la vérifie.
    ///
    /// La vérification part **dans la foulée** et non sur demande : une entrée qui vient
    /// d'être ajoutée sans rien avoir prouvé afficherait `unverified` alors que rien
    /// n'attend l'utilisateur — et la maquette relance justement « à tout changement de
    /// chemin ou d'adaptateur ».
    pub fn declare(&self, draft: NewTool) -> Result<Changed, SettingsError> {
        let command = {
            let mut tools = self.lock()?;
            // Le verrou est tenu pendant le jugement **et** l'ajout : deux ajouts
            // concurrents de la même commande passeraient tous les deux si l'unicité était
            // vérifiée en dehors, et le registre porterait alors deux entrées homonymes.
            let declared = draft.declare(&self.adapters(), &tools)?;
            let command = declared.command.clone();
            tools.push(declared);
            command
        };
        self.verify(&command)
    }

    /// Change le dossier ou l'adaptateur d'une entrée, et la re-vérifie.
    ///
    /// Les deux ensemble parce qu'ils se relancent ensemble : appliquer la correction
    /// proposée par un état invalide change l'un **ou** l'autre, et la vérification qui
    /// suit est la même.
    pub fn retarget(
        &self,
        command: &str,
        adapter: &str,
        config: Option<&str>,
    ) -> Result<Changed, SettingsError> {
        {
            let mut tools = self.lock()?;
            let Some(tool) = tools.iter_mut().find(|tool| tool.command == command) else {
                return Err(SettingsError::UnknownTool(command.to_owned()));
            };
            if !self.adapters().iter().any(|known| known == adapter) {
                return Err(SettingsError::UnknownAdapter(adapter.to_owned()));
            }
            tool.adapter = adapter.to_owned();
            tool.config = config
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_owned);
            // L'entrée retombe à `unverified` **avant** que quoi que ce soit ne soit
            // relancé : entre la frappe et la réponse, ce que l'écran montrait décrivait
            // l'ancien chemin.
            let refreshed = tool.clone().verified_by(Verification::unverified());
            *tool = refreshed;
        }
        self.verify(command)
    }

    /// Relance la séquence sur une entrée — le bouton `re-verify` d'une carte.
    pub fn verify(&self, command: &str) -> Result<Changed, SettingsError> {
        let mut tools = self.lock()?;
        let Some(index) = tools.iter().position(|tool| tool.command == command) else {
            return Err(SettingsError::UnknownTool(command.to_owned()));
        };
        let pending = verify_at(&self.verifier, &mut tools, index);
        Ok(Changed {
            tools: tools.clone(),
            pending: pending.into_iter().collect(),
        })
    }

    /// Relance la séquence sur **toute** la liste — le bouton `re-verify all`.
    ///
    /// Les premiers temps se font ici, l'un après l'autre : ils ne lisent qu'un dossier et
    /// le `PATH`, et les faire en parallèle coûterait plus en fils qu'ils ne prennent de
    /// temps. Ce sont les seconds temps qui partent en parallèle — et c'est là que
    /// [`super::permits`] les borne.
    pub fn verify_all(&self) -> Result<Changed, SettingsError> {
        let mut tools = self.lock()?;
        let mut pending = Vec::new();
        for index in 0..tools.len() {
            pending.extend(verify_at(&self.verifier, &mut tools, index));
        }
        Ok(Changed {
            tools: tools.clone(),
            pending,
        })
    }

    /// Lance le second temps. **Ne prend aucun verrou** : la commande peut mettre des
    /// secondes à répondre, et le registre doit rester lisible pendant ce temps.
    pub fn second_pass(&self, next: &SecondPass) -> Verification {
        self.verifier.second_pass(&next.command, &next.launch)
    }

    /// Pose le résultat du second temps sur l'entrée, **si elle décrit toujours la même
    /// chose**.
    ///
    /// Un résultat périmé est jeté sans bruit : il ne décrit pas ce que l'écran montre, et
    /// l'afficher ferait clignoter une réponse à une question qu'on ne pose plus.
    pub fn settle(
        &self,
        next: &SecondPass,
        verification: Verification,
    ) -> Result<Option<Vec<ToolDeclaration>>, SettingsError> {
        if !next.stored {
            return Ok(None);
        }
        let mut tools = self.lock()?;
        let Some(tool) = tools.iter_mut().find(|tool| tool.command == next.command) else {
            return Ok(None);
        };
        if tool.adapter != next.adapter || tool.config != next.config {
            return Ok(None);
        }
        *tool = tool.clone().verified_by(verification);
        Ok(Some(tools.clone()))
    }

    /// Vérifie une saisie qui n'est pas encore déclarée — le formulaire d'ajout.
    ///
    /// Elle ne touche pas le registre : le bouton `add` a besoin de savoir ce que les
    /// quatre tests disent **avant** qu'il n'y ait quoi que ce soit à ajouter.
    pub fn verify_draft(&self, draft: &NewTool) -> (Verification, Option<SecondPass>) {
        let command = draft.command.trim();
        let config = draft
            .config
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty());
        let first = self
            .verifier
            .first_pass(command, draft.adapter.trim(), config);
        let shown = first.shown().clone();
        let pending = match first {
            FirstPass::Pending { launch, .. } => Some(SecondPass {
                command: command.to_owned(),
                adapter: draft.adapter.trim().to_owned(),
                config: config.map(str::to_owned),
                launch,
                stored: false,
            }),
            FirstPass::Settled(_) => None,
        };
        (shown, pending)
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

/// Le premier temps d'une entrée du registre, résultat posé sur place.
fn verify_at(
    verifier: &Verifier,
    tools: &mut [ToolDeclaration],
    index: usize,
) -> Option<SecondPass> {
    let tool = tools.get_mut(index)?;
    let first = verifier.first_pass(&tool.command, &tool.adapter, tool.config.as_deref());
    let shown = first.shown().clone();
    let pending = match first {
        FirstPass::Pending { launch, .. } => Some(SecondPass {
            command: tool.command.clone(),
            adapter: tool.adapter.clone(),
            config: tool.config.clone(),
            launch,
            stored: true,
        }),
        FirstPass::Settled(_) => None,
    };
    *tool = tool.clone().verified_by(shown);
    pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::settings::fakes::{FakeCommands, FakeFolders};
    use crate::features::settings::verification::{AdapterProfile, VerificationState};

    fn profiles() -> Vec<AdapterProfile> {
        vec![
            AdapterProfile {
                id: "claude-code".to_owned(),
                default_config: Some("/home/.claude".to_owned()),
                signature: vec!["settings.json".to_owned()],
                config_env: Some("CLAUDE_CONFIG_DIR".to_owned()),
                probe_args: vec!["--version".to_owned()],
            },
            AdapterProfile {
                id: "generic".to_owned(),
                default_config: None,
                signature: Vec::new(),
                config_env: None,
                probe_args: vec!["--version".to_owned()],
            },
        ]
    }

    /// Test Data Builder : un registre dont on ne décrit que le monde qui compte.
    struct RegistryBuilder {
        files: FakeFolders,
        commands: FakeCommands,
    }

    impl RegistryBuilder {
        fn new() -> Self {
            Self {
                files: FakeFolders::new("/home"),
                commands: FakeCommands::new().answering(true),
            }
        }

        fn folder(mut self, path: &str, entries: &[&str]) -> Self {
            self.files = self.files.folder(path, entries);
            self
        }

        fn in_path(mut self, command: &str, program: &str) -> Self {
            self.commands = self.commands.in_path(command, program);
            self
        }

        fn build(self) -> ToolRegistry {
            ToolRegistry::new(Arc::new(Verifier::new(
                Arc::new(self.files),
                Arc::new(self.commands),
                profiles(),
            )))
        }
    }

    fn draft(command: &str, adapter: &str, config: Option<&str>) -> NewTool {
        NewTool {
            command: command.to_owned(),
            label: None,
            adapter: adapter.to_owned(),
            config: config.map(str::to_owned),
        }
    }

    #[test]
    fn given_a_fresh_declaration_when_it_lands_in_the_registry_then_it_already_carries_what_the_tests_said(
    ) {
        // Given — la maquette relance « à tout changement de chemin ou d'adaptateur » ; un
        // ajout est le premier de ces changements
        let registry = RegistryBuilder::new()
            .folder("/home/.claude", &["settings.json"])
            .build();

        // When
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");

        // Then — le test 3 échoue (rien dans le `PATH`), donc une réserve, pas un `unverified`
        let tool = changed.tools.first().expect("l'entrée est là");
        assert_eq!(tool.verification.state, VerificationState::Caveat);
        assert!(tool.verified);
    }

    #[test]
    fn given_an_entry_pointing_at_a_folder_that_is_not_a_config_when_it_is_declared_then_it_is_kept_and_shown_invalid(
    ) {
        // Given — la maquette montre une entrée invalide **dans la liste** (`3e`) : Ash
        // n'empêche pas de déclarer, il refuse d'écrire. Confondre les deux ferait
        // disparaître l'entrée qu'on essaie justement de corriger
        let registry = RegistryBuilder::new()
            .folder("/home/notes", &["a.md"])
            .build();

        // When
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/notes")))
            .expect("la saisie est valide");

        // Then
        let tool = changed.tools.first().expect("l'entrée est là");
        assert_eq!(tool.verification.state, VerificationState::Invalid);
        assert!(!tool.verified);
    }

    #[test]
    fn given_an_invalid_entry_when_the_suggested_fix_retargets_it_then_it_is_verified_again_on_the_spot(
    ) {
        // Given — appliquer la correction proposée n'a de sens que si l'écran répond
        let registry = RegistryBuilder::new()
            .folder("/home/notes", &["a.md"])
            .build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/notes")))
            .expect("la saisie est valide");

        // When — `generic` ne signe rien : le même dossier lui convient
        let changed = registry
            .retarget("claude", "generic", Some("/home/notes"))
            .expect("l'entrée existe");

        // Then
        let tool = changed.tools.first().expect("l'entrée est là");
        assert_eq!(tool.verification.state, VerificationState::Caveat);
        assert_eq!(tool.adapter, "generic");
    }

    #[test]
    fn given_a_path_changed_while_the_command_was_still_answering_when_the_late_result_comes_back_then_it_is_dropped(
    ) {
        // Given — le test 4 peut mettre cinq secondes ; l'utilisateur, lui, tape. Un
        // résultat périmé posé sur l'entrée décrirait un chemin qui n'est plus à l'écran
        let registry = RegistryBuilder::new()
            .folder("/home/.claude", &["settings.json"])
            .folder("/home/other", &["settings.json"])
            .in_path("claude", "/bin/claude")
            .build();
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let stale = changed
            .pending
            .first()
            .cloned()
            .expect("le test 4 reste à lancer");
        registry
            .retarget("claude", "claude-code", Some("/home/other"))
            .expect("l'entrée existe");

        // When
        let settled = registry
            .settle(&stale, Verification::unverified())
            .expect("le registre répond");

        // Then
        assert!(settled.is_none());
    }

    #[test]
    fn given_two_entries_to_re_verify_when_the_whole_list_is_relaunched_then_each_one_that_needs_the_fourth_test_asks_for_it(
    ) {
        // Given — `re-verify all` relance la liste ; ce sont les seconds temps qui partent
        // en parallèle, et eux seuls ont un processus à lancer
        let registry = RegistryBuilder::new()
            .folder("/home/.claude", &["settings.json"])
            .in_path("claude", "/bin/claude")
            .build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        registry
            .declare(draft("kimi", "generic", Some("/home/.claude")))
            .expect("la saisie est valide");

        // When
        let changed = registry.verify_all().expect("le registre répond");

        // Then — `kimi` est sur `generic`, qui ne sait imposer aucun dossier : rien à lancer
        assert_eq!(changed.tools.len(), 2);
        assert_eq!(
            changed
                .pending
                .iter()
                .map(|next| next.command.as_str())
                .collect::<Vec<_>>(),
            vec!["claude"]
        );
    }

    #[test]
    fn given_a_command_already_declared_when_the_same_one_is_declared_again_then_the_list_is_unchanged(
    ) {
        // Given — un refus ne doit pas laisser le registre à moitié modifié
        let registry = RegistryBuilder::new().build();
        registry
            .declare(draft("claude", "generic", Some("/home/x")))
            .expect("la saisie est valide");

        // When
        let refused = registry.declare(draft("claude", "generic", Some("/home/x")));

        // Then
        assert!(refused.is_err());
        assert_eq!(registry.tools().unwrap().len(), 1);
    }

    #[test]
    fn given_the_only_declared_command_when_it_is_forgotten_then_the_list_is_empty_again() {
        // Given — c'est ce qui rend l'état vide atteignable après un ajout
        let registry = RegistryBuilder::new().build();
        registry
            .declare(draft("claude", "generic", Some("/home/x")))
            .expect("la saisie est valide");

        // When
        let after = registry.forget("claude").unwrap();

        // Then
        assert_eq!(after, vec![]);
    }

    #[test]
    fn given_a_command_that_was_never_declared_when_it_is_forgotten_then_it_is_refused() {
        // Given — se taire ferait croire à une suppression qui n'a pas eu lieu
        let registry = RegistryBuilder::new().build();

        // When
        let forgotten = registry.forget("codex");

        // Then
        assert_eq!(
            forgotten.unwrap_err(),
            SettingsError::UnknownTool("codex".to_owned())
        );
    }
}
