use std::sync::{Arc, Mutex};

use super::error::SettingsError;
use super::hooks::{report, BlockAt, HookAction};
use super::ports::{HookBlocks, Launch};
use super::tool::{NewTool, ToolDeclaration};
use super::values::{optional, Command, ConfigTarget};
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
    /// Le seul chemin par lequel Ash écrit chez l'utilisateur, et il est derrière un trait :
    /// la feature ne connaît aucun adaptateur concret (ADR-0008).
    blocks: Arc<dyn HookBlocks>,
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
    pub command: Command,
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
    pub fn new(verifier: Arc<Verifier>, blocks: Arc<dyn HookBlocks>) -> Self {
        Self {
            verifier,
            blocks,
            tools: Mutex::new(Vec::new()),
        }
    }

    /// Les adaptateurs proposés par le formulaire d'ajout.
    pub fn adapters(&self) -> Vec<String> {
        self.verifier.adapters()
    }

    pub fn tools(&self) -> Result<Vec<ToolDeclaration>, SettingsError> {
        let stored = self.lock()?.clone();
        Ok(self.enrich(stored))
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
        command: &Command,
        adapter: &str,
        config: Option<&str>,
    ) -> Result<Changed, SettingsError> {
        {
            let mut tools = self.lock()?;
            let Some(tool) = tools.iter_mut().find(|tool| &tool.command == command) else {
                return Err(SettingsError::UnknownTool(command.clone()));
            };
            if !self.adapters().iter().any(|known| known == adapter) {
                return Err(SettingsError::UnknownAdapter(adapter.to_owned()));
            }
            let folder = optional(config);
            // L'entrée retombe à `unverified` **avant** que quoi que ce soit ne soit
            // relancé : entre la frappe et la réponse, ce que l'écran montrait décrivait
            // l'ancien chemin.
            let refreshed = tool
                .clone()
                .retargeted(adapter, folder)
                .verified_by(Verification::unverified(), None);
            *tool = refreshed;
        }
        self.verify(command)
    }

    /// Ramène une entrée à **son** dernier dossier valide (spec §9.1).
    ///
    /// Pas au défaut de son adaptateur : deux entrées partagent souvent un adaptateur, et y
    /// revenir les rendrait identiques — le doublon deviendrait la conséquence mécanique du
    /// geste au lieu d'un accident. Une entrée qui n'a jamais rien prouvé n'a donc rien à
    /// restaurer, et le dire vaut mieux que de la renvoyer quelque part au hasard.
    pub fn reset(&self, command: &Command) -> Result<Changed, SettingsError> {
        {
            let mut tools = self.lock()?;
            let Some(tool) = tools.iter_mut().find(|tool| &tool.command == command) else {
                return Err(SettingsError::UnknownTool(command.clone()));
            };
            let Some(memory) = tool.last_valid_config.clone() else {
                return Err(SettingsError::NothingToRestore(command.clone()));
            };
            let adapter = tool.adapter.clone();
            // Ce qu'on remplace est le dossier **désigné**, défaut de l'adaptateur compris :
            // la ligne `was` montre un chemin, et « rien » ne se barre pas d'un trait.
            let previous = self.verifier.target(&adapter, tool.config.as_deref());
            let mut restored = tool
                .clone()
                .retargeted(&adapter, Some(memory.declared().to_owned()))
                .verified_by(Verification::unverified(), None);
            // Rien à annuler quand rien n'a changé : offrir « annuler » ferait chercher ce
            // que le geste a fait. « Rien n'a changé » est la question du doublon, posée sur
            // la même valeur — deux écritures du même dossier ne sont pas un déplacement.
            restored.reset_from = previous.filter(|before| *before != memory);
            *tool = restored;
        }
        self.verify(command)
    }

    /// Annule la réinitialisation, tant qu'elle est la dernière chose qui s'est passée.
    ///
    /// C'est le `undo the reset` de la bannière de doublon : le geste qui vient de créer la
    /// collision est celui qu'on défait, et il reste **à portée** plutôt que d'obliger à
    /// retaper un chemin qu'on n'a plus sous les yeux.
    pub fn undo_reset(&self, command: &Command) -> Result<Changed, SettingsError> {
        {
            let mut tools = self.lock()?;
            let Some(tool) = tools.iter_mut().find(|tool| &tool.command == command) else {
                return Err(SettingsError::UnknownTool(command.clone()));
            };
            let Some(before) = tool.reset_from.clone() else {
                return Err(SettingsError::NothingToUndo(command.clone()));
            };
            let adapter = tool.adapter.clone();
            *tool = tool
                .clone()
                .retargeted(&adapter, Some(before.declared().to_owned()))
                .verified_by(Verification::unverified(), None);
        }
        self.verify(command)
    }

    /// Pose ou met à jour le bloc de hooks d'une entrée.
    ///
    /// **La garde est la ligne `hooks` elle-même**, et pas une seconde règle écrite ici :
    /// une entrée que la séquence n'autorise pas, un doublon, un bloc édité à la main ou un
    /// fichier qui porte déjà d'autres hooks ont tous éteint le bouton — et le backend
    /// refuse pour la même raison qu'il l'a éteint. Recopier la condition en ferait deux,
    /// dont une seule protège vraiment le fichier de l'utilisateur.
    pub fn install_hooks(&self, command: &Command) -> Result<Vec<ToolDeclaration>, SettingsError> {
        self.write_hooks(command, HookAction::Install)
    }

    /// Retire le bloc et ses marqueurs — le `remove` de l'état `installed`.
    pub fn remove_hooks(&self, command: &Command) -> Result<Vec<ToolDeclaration>, SettingsError> {
        self.write_hooks(command, HookAction::Remove)
    }

    fn write_hooks(
        &self,
        command: &Command,
        asked: HookAction,
    ) -> Result<Vec<ToolDeclaration>, SettingsError> {
        let tools = self.tools()?;
        let Some(tool) = tools.iter().find(|tool| &tool.command == command) else {
            return Err(SettingsError::UnknownTool(command.clone()));
        };
        let allowed = match asked {
            // `update` est une installation : le geste est le même, seul le mot change.
            HookAction::Install => {
                matches!(tool.hooks.action, HookAction::Install | HookAction::Update)
            }
            other => tool.hooks.action == other,
        };
        if !allowed || !tool.hooks.enabled {
            return Err(SettingsError::HooksRefused(tool.hooks.summary.clone()));
        }
        let Some(folder) = self.verifier.target(&tool.adapter, tool.config.as_deref()) else {
            return Err(SettingsError::NoConfigFolder(command.clone()));
        };

        match asked {
            HookAction::Remove => self.blocks.remove(&tool.adapter, &folder),
            _ => self.blocks.install(&tool.adapter, &folder),
        }
        .map_err(SettingsError::HooksRefused)?;

        self.tools()
    }

    /// Relance la séquence sur une entrée — le bouton `re-verify` d'une carte.
    pub fn verify(&self, command: &Command) -> Result<Changed, SettingsError> {
        let (stored, pending) = {
            let mut tools = self.lock()?;
            let Some(index) = tools.iter().position(|tool| &tool.command == command) else {
                return Err(SettingsError::UnknownTool(command.clone()));
            };
            let pending = verify_at(&self.verifier, &mut tools, index);
            (tools.clone(), pending.into_iter().collect())
        };
        Ok(Changed {
            tools: self.enrich(stored),
            pending,
        })
    }

    /// Relance la séquence sur **toute** la liste — le bouton `re-verify all`.
    ///
    /// Les premiers temps se font ici, l'un après l'autre : ils ne lisent qu'un dossier et
    /// le `PATH`, et les faire en parallèle coûterait plus en fils qu'ils ne prennent de
    /// temps. Ce sont les seconds temps qui partent en parallèle — et c'est là que
    /// [`super::permits`] les borne.
    pub fn verify_all(&self) -> Result<Changed, SettingsError> {
        let (stored, pending) = {
            let mut tools = self.lock()?;
            let mut pending = Vec::new();
            for index in 0..tools.len() {
                pending.extend(verify_at(&self.verifier, &mut tools, index));
            }
            (tools.clone(), pending)
        };
        Ok(Changed {
            tools: self.enrich(stored),
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
        let stored = {
            let mut tools = self.lock()?;
            let Some(tool) = tools.iter_mut().find(|tool| tool.command == next.command) else {
                return Ok(None);
            };
            if tool.adapter != next.adapter || tool.config != next.config {
                return Ok(None);
            }
            let declared = self.verifier.target(&tool.adapter, tool.config.as_deref());
            *tool = tool.clone().verified_by(verification, declared);
            tools.clone()
        };
        Ok(Some(self.enrich(stored)))
    }

    /// Vérifie une saisie qui n'est pas encore déclarée — le formulaire d'ajout.
    ///
    /// Elle ne touche pas le registre : le bouton `add` a besoin de savoir ce que les
    /// quatre tests disent **avant** qu'il n'y ait quoi que ce soit à ajouter.
    pub fn verify_draft(
        &self,
        draft: &NewTool,
    ) -> Result<(Verification, Option<SecondPass>), SettingsError> {
        let command = Command::parse(&draft.command)?;
        let config = optional(draft.config.as_deref());
        let first = self
            .verifier
            .first_pass(&command, draft.adapter.trim(), config.as_deref());
        let shown = first.shown().clone();
        let pending = match first {
            FirstPass::Pending { launch, .. } => Some(SecondPass {
                command,
                adapter: draft.adapter.trim().to_owned(),
                config,
                launch,
                stored: false,
            }),
            FirstPass::Settled(_) => None,
        };
        Ok((shown, pending))
    }

    /// Oublie une entrée — le `✕` de l'en-tête de carte.
    ///
    /// C'est aussi ce qui rend l'état vide atteignable autrement qu'au premier démarrage :
    /// une liste où l'on ajoute sans pouvoir revenir en arrière ferait d'une faute de
    /// frappe une entrée définitive.
    pub fn forget(&self, command: &Command) -> Result<Vec<ToolDeclaration>, SettingsError> {
        let stored = {
            let mut tools = self.lock()?;
            let before = tools.len();
            tools.retain(|tool| &tool.command != command);
            if tools.len() == before {
                return Err(SettingsError::UnknownTool(command.clone()));
            }
            tools.clone()
        };
        // La liste enrichie, et pas la liste stockée : oublier une entrée peut lever le
        // doublon d'une autre, et sa ligne `hooks` avec.
        Ok(self.enrich(stored))
    }

    /// Ce qu'une entrée ne peut pas savoir seule : ses homonymes de dossier, et l'état du
    /// fichier qu'elle vise.
    ///
    /// **Dérivé à chaque fois plutôt que retenu**, parce que les deux sources changent sans
    /// que l'entrée bouge : une autre entrée peut prendre son dossier, et l'utilisateur peut
    /// éditer son `settings.json` pendant que la fenêtre est ouverte. Un état de hooks mis
    /// en cache serait exactement le genre de vérité périmée sur laquelle on finirait par
    /// écrire.
    ///
    /// **Hors du verrou** : la lecture d'un fichier de configuration n'a pas à figer le
    /// registre pour les autres fils, dont ceux du second temps de la vérification.
    fn enrich(&self, mut tools: Vec<ToolDeclaration>) -> Vec<ToolDeclaration> {
        let aimed: Vec<(Command, Option<ConfigTarget>)> = tools
            .iter()
            .map(|tool| {
                (
                    tool.command.clone(),
                    self.verifier.target(&tool.adapter, tool.config.as_deref()),
                )
            })
            .collect();

        for (index, tool) in tools.iter_mut().enumerate() {
            let mine = aimed.get(index).and_then(|(_, dir)| dir.clone());
            let mut duplicates = Vec::new();
            // Qui tient le fichier : **la première entrée déclarée**. Il en faut une, et
            // celle-là ne dépend ni de l'ordre des clics ni du contenu du disque — donc
            // l'écran ne change pas d'avis d'un affichage à l'autre.
            let mut taken_by: Option<Command> = None;
            for (position, (name, dir)) in aimed.iter().enumerate() {
                if position == index || dir.is_none() || *dir != mine {
                    continue;
                }
                duplicates.push(name.clone());
                if position < index && taken_by.is_none() {
                    taken_by = Some(name.clone());
                }
            }

            let found: Option<BlockAt> = match (&mine, taken_by.is_none()) {
                (Some(folder), true) if tool.verification.allows_hooks => {
                    self.blocks.inspect(&tool.adapter, folder)
                }
                _ => None,
            };

            tool.hooks = report(&tool.verification, &tool.adapter, taken_by.as_ref(), found);
            tool.duplicates = duplicates;
        }
        tools
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
    let declared = verifier.target(&tool.adapter, tool.config.as_deref());
    *tool = tool.clone().verified_by(shown, declared);
    pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::hooks::Presence;
    use crate::features::settings::fakes::{FakeBlocks, FakeCommands, FakeFolders};
    use crate::features::settings::hooks::HookState;
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
        blocks: FakeBlocks,
    }

    impl RegistryBuilder {
        fn new() -> Self {
            Self {
                files: FakeFolders::new("/home"),
                commands: FakeCommands::new().answering(true),
                // `generic` n'instrumente rien, ici comme dans l'application : c'est ce qui
                // fait qu'une entrée sur cet adaptateur ne se voit jamais proposer `install`.
                blocks: FakeBlocks::new().without_hooks("generic"),
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

        fn carrying(mut self, config_dir: &str, presence: Presence) -> Self {
            self.blocks = self.blocks.at(config_dir, presence);
            self
        }

        fn build(self) -> ToolRegistry {
            self.assemble().0
        }

        /// Le registre **et** ce qu'il écrit : les tests qui comptent ici sont ceux qui
        /// affirment qu'aucun fichier n'a été touché.
        fn assemble(self) -> (ToolRegistry, Arc<FakeBlocks>) {
            let blocks = Arc::new(self.blocks);
            let registry = ToolRegistry::new(
                Arc::new(Verifier::new(
                    Arc::new(self.files),
                    Arc::new(self.commands),
                    profiles(),
                )),
                Arc::clone(&blocks) as Arc<dyn HookBlocks>,
            );
            (registry, blocks)
        }
    }

    /// Un nom de commande valide, tel que `NewTool::declare` en produit.
    fn named(command: &str) -> Command {
        Command::parse(command).unwrap_or_else(|why| panic!("{command} est un nom valide : {why}"))
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
            .retarget(&named("claude"), "generic", Some("/home/notes"))
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
            .retarget(&named("claude"), "claude-code", Some("/home/other"))
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
        let after = registry.forget(&named("claude")).unwrap();

        // Then
        assert_eq!(after, vec![]);
    }

    /// Deux entrées `claude-code` que tout sépare sauf l'adaptateur — le cas de la maquette.
    fn two_claude_accounts() -> RegistryBuilder {
        RegistryBuilder::new()
            .folder("/home/.claude", &["settings.json"])
            .folder("/home/.claude-perso", &["settings.json"])
            .in_path("claude", "/bin/claude")
            .in_path("claude-perso", "/bin/claude-perso")
    }

    #[test]
    fn given_two_entries_pointing_at_the_same_folder_when_the_list_is_read_then_both_rows_carry_the_flag(
    ) {
        // Given — « le doublon est signalé sur les deux lignes, pas seulement sur celle
        // qu'on vient de toucher » (spec §9.1). Ne marquer que la seconde ferait chercher
        // laquelle est l'autre
        let registry = two_claude_accounts().build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let tools = registry
            .declare(draft("claude-perso", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide")
            .tools;

        // When
        let flagged: Vec<(&str, Vec<&str>)> = tools
            .iter()
            .map(|tool| {
                (
                    tool.command.as_str(),
                    tool.duplicates.iter().map(Command::as_str).collect(),
                )
            })
            .collect();

        // Then
        assert_eq!(
            flagged,
            vec![
                ("claude", vec!["claude-perso"]),
                ("claude-perso", vec!["claude"]),
            ]
        );
    }

    #[test]
    fn given_two_entries_that_write_the_same_folder_differently_when_the_list_is_read_then_the_duplicate_still_shows(
    ) {
        // Given — `~/.claude` et `/home/.claude` sont le même dossier, donc le même fichier
        // de hooks. C'est le cas que la bannière de doublon existe pour attraper, et celui
        // qu'elle raterait si un lecteur comparait les chemins tels qu'ils sont écrits
        // pendant qu'un autre comparait les chemins résolus
        let registry = two_claude_accounts().build();
        registry
            .declare(draft("claude", "claude-code", Some("~/.claude")))
            .expect("la saisie est valide");

        // When
        let tools = registry
            .declare(draft("claude-perso", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide")
            .tools;

        // Then — signalé sur les deux lignes, et la seconde n'écrit pas par-dessus la première
        assert_eq!(tools[0].duplicates, vec![named("claude-perso")]);
        assert_eq!(tools[1].duplicates, vec![named("claude")]);
        assert_eq!(tools[1].hooks.state, HookState::Blocked);
    }

    #[test]
    fn given_two_entries_on_the_same_folder_when_the_second_looks_at_its_hooks_then_only_it_is_blocked(
    ) {
        // Given — le doublon n'invalide rien : la première garde ses hooks, la seconde ne
        // les écrit pas une deuxième fois dans le même fichier
        let registry = two_claude_accounts().build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let tools = registry
            .declare(draft("claude-perso", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide")
            .tools;

        // When
        let states: Vec<HookState> = tools.iter().map(|tool| tool.hooks.state).collect();

        // Then
        assert_eq!(states, vec![HookState::Missing, HookState::Blocked]);
        assert_eq!(
            tools[1].hooks.summary,
            "already written by claude in this file"
        );
    }

    #[test]
    fn given_an_entry_a_duplicate_blocks_when_its_hooks_are_asked_for_then_nothing_is_written() {
        // Given — la garde est en Rust, pas dans le bouton éteint : une fenêtre qui
        // appellerait quand même ne doit pas faire écrire Ash chez l'utilisateur
        let (registry, blocks) = two_claude_accounts().assemble();
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        registry
            .declare(draft("claude-perso", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");

        // When
        let refused = registry.install_hooks(&named("claude-perso"));

        // Then
        assert_eq!(
            refused.unwrap_err(),
            SettingsError::HooksRefused("already written by claude in this file".to_owned())
        );
        assert_eq!(blocks.written(), Vec::<String>::new());
    }

    #[test]
    fn given_an_entry_whose_folder_is_not_verified_when_its_hooks_are_asked_for_then_nothing_is_written(
    ) {
        // Given — « ash n'écrit dans aucun fichier tant qu'une entrée n'est pas vérifiée ».
        // C'est le garde-fou d'ADR-0007, et il vit du côté qui écrit
        let (registry, blocks) = RegistryBuilder::new()
            .folder("/home/notes", &["a.md"])
            .assemble();
        registry
            .declare(draft("claude", "claude-code", Some("/home/notes")))
            .expect("la saisie est valide");

        // When
        let refused = registry.install_hooks(&named("claude"));

        // Then
        assert!(matches!(refused, Err(SettingsError::HooksRefused(_))));
        assert_eq!(blocks.written(), Vec::<String>::new());
    }

    #[test]
    fn given_a_verified_entry_whose_block_is_missing_when_its_hooks_are_installed_then_they_land_in_its_own_folder(
    ) {
        // Given — `claude` et `claude-perso` sont deux dossiers, donc deux blocs (ADR-0007) :
        // ce qui est écrit doit l'être là où l'entrée pointe, pas au défaut de l'adaptateur
        let (registry, blocks) = two_claude_accounts().assemble();
        registry
            .declare(draft(
                "claude-perso",
                "claude-code",
                Some("/home/.claude-perso"),
            ))
            .expect("la saisie est valide");

        // When
        let after = registry
            .install_hooks(&named("claude-perso"))
            .expect("elle est vérifiée");

        // Then
        assert_eq!(
            blocks.written(),
            vec!["install claude-code /home/.claude-perso"]
        );
        assert_eq!(after[0].hooks.state, HookState::Missing);
    }

    #[test]
    fn given_a_block_someone_edited_by_hand_when_its_entry_is_shown_then_the_line_offers_the_diff_instead_of_a_write(
    ) {
        // Given — le conflit ne se déduit pas d'un souvenir : Ash relit le fichier, parce
        // que l'utilisateur a pu l'éditer entre deux ouvertures de la fenêtre
        let (registry, blocks) = two_claude_accounts()
            .carrying(
                "/home/.claude",
                Presence::HandEdited {
                    diff: "- ash\n+ moi".to_owned(),
                },
            )
            .assemble();

        // When
        let tools = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide")
            .tools;

        // Then
        assert_eq!(tools[0].hooks.state, HookState::Conflict);
        assert_eq!(tools[0].hooks.diff.as_deref(), Some("- ash\n+ moi"));
        assert!(registry.install_hooks(&named("claude")).is_err());
        assert_eq!(blocks.written(), Vec::<String>::new());
    }

    #[test]
    fn given_an_entry_that_never_passed_the_four_tests_when_it_is_reset_then_it_says_there_is_nothing_to_restore(
    ) {
        // Given — « réinitialiser ramène à la dernière valeur valide » (spec §9.1) : quand
        // il n'y en a pas, renvoyer l'entrée au défaut de l'adaptateur fabriquerait
        // justement le doublon que cette règle existe pour éviter
        let registry = RegistryBuilder::new()
            .folder("/home/notes", &["a.md"])
            .build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/notes")))
            .expect("la saisie est valide");

        // When
        let refused = registry.reset(&named("claude"));

        // Then
        assert_eq!(
            refused.err(),
            Some(SettingsError::NothingToRestore(named("claude")))
        );
    }

    #[test]
    fn given_an_entry_that_moved_away_from_a_folder_that_worked_when_it_is_reset_then_it_goes_back_to_that_folder(
    ) {
        // Given — et pas au défaut de l'adaptateur : `claude-perso` reviendrait alors sur
        // `~/.claude`, c'est-à-dire sur l'entrée d'à côté
        let registry = two_claude_accounts().build();
        let changed = registry
            .declare(draft(
                "claude-perso",
                "claude-code",
                Some("/home/.claude-perso"),
            ))
            .expect("la saisie est valide");
        // Le test 4 répond : c'est lui qui rend l'entrée `valid`, donc mémorisable.
        let pending = changed
            .pending
            .first()
            .cloned()
            .expect("le test 4 reste à lancer");
        let verification = registry.second_pass(&pending);
        registry
            .settle(&pending, verification)
            .expect("le registre répond");
        registry
            .retarget(&named("claude-perso"), "claude-code", Some("/home/notes"))
            .expect("l'entrée existe");

        // When
        let after = registry
            .reset(&named("claude-perso"))
            .expect("elle a été valide");

        // Then
        let tool = after.tools.first().expect("l'entrée est là");
        assert_eq!(tool.config.as_deref(), Some("/home/.claude-perso"));
        assert_eq!(
            tool.reset_from.as_ref().map(ConfigTarget::declared),
            Some("/home/notes")
        );
    }

    #[test]
    fn given_a_reset_that_landed_on_another_entrys_folder_when_it_is_undone_then_the_previous_folder_comes_back(
    ) {
        // Given — c'est le `undo the reset` de la bannière : le geste qui a créé la
        // collision est celui qu'on défait, et il reste à portée
        let registry = two_claude_accounts().build();
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let pending = changed
            .pending
            .first()
            .cloned()
            .expect("le test 4 reste à lancer");
        let verification = registry.second_pass(&pending);
        registry
            .settle(&pending, verification)
            .expect("le registre répond");
        registry
            .retarget(&named("claude"), "claude-code", Some("/home/.claude-perso"))
            .expect("l'entrée existe");
        registry.reset(&named("claude")).expect("elle a été valide");

        // When
        let after = registry
            .undo_reset(&named("claude"))
            .expect("elle vient d'être réinitialisée");

        // Then
        let tool = after.tools.first().expect("l'entrée est là");
        assert_eq!(tool.config.as_deref(), Some("/home/.claude-perso"));
        assert_eq!(tool.reset_from, None);
    }

    #[test]
    fn given_an_entry_the_user_retyped_after_a_reset_when_the_undo_is_asked_for_then_it_is_refused()
    {
        // Given — « annuler la réinitialisation » ne veut plus rien dire une fois le chemin
        // retapé : proposer le geste ferait revenir un dossier que personne ne demande plus
        let registry = two_claude_accounts().build();
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let pending = changed
            .pending
            .first()
            .cloned()
            .expect("le test 4 reste à lancer");
        let verification = registry.second_pass(&pending);
        registry
            .settle(&pending, verification)
            .expect("le registre répond");
        registry
            .retarget(&named("claude"), "claude-code", Some("/home/.claude-perso"))
            .expect("l'entrée existe");
        registry.reset(&named("claude")).expect("elle a été valide");

        // When
        registry
            .retarget(&named("claude"), "claude-code", Some("/home/notes"))
            .expect("l'entrée existe");
        let refused = registry.undo_reset(&named("claude"));

        // Then
        assert_eq!(
            refused.err(),
            Some(SettingsError::NothingToUndo(named("claude")))
        );
    }

    #[test]
    fn given_a_command_that_was_never_declared_when_it_is_forgotten_then_it_is_refused() {
        // Given — se taire ferait croire à une suppression qui n'a pas eu lieu
        let registry = RegistryBuilder::new().build();

        // When
        let forgotten = registry.forget(&named("codex"));

        // Then
        assert_eq!(
            forgotten.unwrap_err(),
            SettingsError::UnknownTool(named("codex"))
        );
    }
}
