use super::error::SettingsError;
use super::hooks::HooksReport;
use super::values::{optional, Command, ConfigTarget};
use super::verification::{Verification, VerificationState};

/// Une commande reconnue, telle que la spec §9 et
/// [ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md) la décrivent.
///
/// C'est l'entrée de `~/.ash/tools.json`, un pour un :
///
/// ```json
/// {
///   "command": "claude-perso",
///   "label": "Perso",
///   "adapter": "claude-code",
///   "config": "~/.claude-perso"
/// }
/// ```
///
/// Ce qui est écrit dans le fichier est la **déclaration seule**, plus le dernier dossier
/// valide ; ce qu'elle a prouvé, ses homonymes et l'état de ses hooks se relisent à chaque
/// session — voir [`super::persisted`].
///
/// **`command` est l'identité**, et il n'y a pas d'autre identifiant : c'est le `match` du
/// fichier, c'est-à-dire le nom de processus que la sonde compare. Poser un ulid à côté
/// donnerait deux clés pour une seule chose, et rien ne dirait laquelle fait foi le jour
/// où le fichier est édité à la main — ce que la spec §9 autorise explicitement.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ToolDeclaration {
    /// Le `match` : le nom du processus, tel qu'on le tape dans le shell.
    ///
    /// [`Command`] et non `String` : la règle qui en fait un nom de processus est vérifiée
    /// une fois, et le type la porte ensuite dans chaque signature qu'elle traverse.
    pub command: Command,
    /// Le libellé d'affichage — `Pro`, `Perso`. Absent est le cas courant.
    pub label: Option<String>,
    /// L'identifiant de l'adaptateur ([`Adapter::id`](crate::features::agents::Adapter::id)).
    pub adapter: String,
    /// Le dossier de configuration. `None` veut dire « celui de l'adaptateur », que
    /// l'adaptateur est seul à connaître — pas un dossier vide.
    pub config: Option<String>,
    /// L'entrée a-t-elle prouvé assez pour qu'on écrive chez l'utilisateur ?
    ///
    /// **Dérivé, jamais posé à la main** : c'est exactement
    /// [`Verification::allows_hooks`], recopié pour que les lecteurs qui ne veulent que ce
    /// oui/non — le compteur de l'en-tête — n'aient
    /// pas à traverser la structure entière. [`ToolDeclaration::verified_by`] est le seul
    /// endroit où les deux sont écrits, donc le seul endroit où ils pourraient diverger.
    pub verified: bool,

    /// Ce que les quatre tests de la spec §9.1 ont dit de cette entrée.
    ///
    /// Elle vit **avec** la déclaration et non à côté : une entrée dont le chemin change
    /// change de vérification au même instant, et deux tables séparées laisseraient un
    /// intervalle où l'écran montrerait le résultat de l'ancien chemin sous le nouveau.
    pub verification: Verification,

    /// Le dernier dossier qui a passé **les quatre** tests, et rien d'autre.
    ///
    /// C'est la mémoire que la spec §9.1 exige : « réinitialiser une entrée la ramène à sa
    /// dernière valeur valide, pas au défaut de son adaptateur ». La nuance décide du sens
    /// de tout l'écran — `claude` et `claude-perso` sont toutes deux en `claude-code`, donc
    /// revenir au défaut de l'adaptateur les rendrait identiques et ferait du doublon la
    /// conséquence mécanique du geste au lieu d'un accident.
    ///
    /// `None` veut dire « cette entrée n'a jamais été valide » : il n'y a alors rien à
    /// restaurer, et le bouton reste visible et éteint avec sa raison — la même règle que
    /// celle des hooks.
    ///
    /// C'est une [`ConfigTarget`] : la mémoire est un **dossier**, et le comparer à la cible
    /// du jour est la même question que celle du doublon. Elle se sérialise comme la forme
    /// déclarée — la chaîne que la fenêtre remet dans son champ.
    pub last_valid_config: Option<ConfigTarget>,

    /// Ce que la réinitialisation vient de remplacer, tant qu'on peut encore l'annuler.
    ///
    /// Il ne survit qu'à ce geste-là : toute autre modification de l'entrée l'efface, parce
    /// qu'« annuler la réinitialisation » ne veut plus rien dire une fois qu'on a retapé le
    /// chemin à la main. C'est ce qui distingue la ligne `was` — juste après un retour en
    /// arrière — de l'étiquette de doublon, qui existe dès que deux entrées collisionnent.
    pub reset_from: Option<ConfigTarget>,

    /// Les autres entrées qui visent le même dossier.
    ///
    /// **Dérivé de la liste entière**, donc recalculé à chaque fois qu'elle change : une
    /// entrée ne peut pas savoir seule qu'elle fait doublon. Le doublon est signalé sur
    /// **les deux** lignes (spec §9.1), pas seulement sur celle qu'on vient de toucher.
    pub duplicates: Vec<Command>,

    /// Où en est le bloc de hooks de cette entrée.
    ///
    /// Dérivé lui aussi : il compose ce que la vérification autorise, ce que les autres
    /// entrées ont déjà pris, et ce que le fichier de l'utilisateur porte. Le registre le
    /// repose à chaque fois qu'il rend la liste.
    pub hooks: HooksReport,
}

impl ToolDeclaration {
    /// Attache un résultat de vérification à une entrée, et **retient le dossier s'il a
    /// tout prouvé**.
    ///
    /// Le seul chemin par lequel [`ToolDeclaration::verified`] change de valeur, et le seul
    /// par lequel la mémoire du dossier se remplit. Elle ne retient que `valid` : une
    /// réserve dit que la commande ne lit pas ce dossier, et y ramener une entrée plus tard
    /// restaurerait quelque chose qui n'a jamais fonctionné.
    ///
    /// `declared` est ce que l'entrée désigne réellement, défaut de l'adaptateur compris —
    /// la mémoire est un **dossier**, pas la présence ou l'absence d'un champ.
    #[must_use]
    pub fn verified_by(
        mut self,
        verification: Verification,
        declared: Option<ConfigTarget>,
    ) -> Self {
        if verification.state == VerificationState::Valid {
            self.last_valid_config = declared;
        }
        self.verified = verification.allows_hooks;
        self.verification = verification;
        self
    }

    /// L'entrée avec la mémoire qu'un fichier lui rend.
    ///
    /// Le second chemin par lequel [`Self::last_valid_config`] se remplit, et le seul qui ne
    /// vienne pas d'une vérification : `~/.ash/tools.json` garde ce dossier parce que sans
    /// lui, « réinitialiser une entrée » ramènerait après un redémarrage au défaut de
    /// l'adaptateur — c'est-à-dire à l'entrée d'à côté (spec §9.1). Il ne dit pas que
    /// l'entrée est vérifiée : elle ne l'est pas, et elle le reste jusqu'à ce que les quatre
    /// tests soient relancés.
    #[must_use]
    pub fn remembering(mut self, folder: Option<ConfigTarget>) -> Self {
        self.last_valid_config = folder;
        self
    }

    /// L'entrée après une modification quelconque de sa cible.
    ///
    /// Elle oublie la réinitialisation : « annuler la réinitialisation » ne veut plus rien
    /// dire une fois que le chemin a été retapé.
    #[must_use]
    pub fn retargeted(mut self, adapter: &str, config: Option<String>) -> Self {
        self.adapter = adapter.to_owned();
        self.config = config;
        self.reset_from = None;
        self
    }
}

/// Ce que le formulaire d'ajout envoie — du texte brut, pas encore une déclaration.
///
/// Distinct de [`ToolDeclaration`] parce que les deux n'ont pas les mêmes invariants : ici
/// tout est possible, y compris des espaces autour et un adaptateur qui n'existe pas.
/// [`NewTool::declare`] est le seul passage de l'un à l'autre.
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NewTool {
    pub command: String,
    #[serde(default)]
    pub label: Option<String>,
    pub adapter: String,
    #[serde(default)]
    pub config: Option<String>,
}

impl NewTool {
    /// Transforme une saisie en déclaration, ou dit pourquoi elle n'en est pas une.
    ///
    /// `adapters` est la liste des adaptateurs embarqués, et `declared` l'état du registre :
    /// les deux viennent de l'appelant, parce qu'une saisie ne se juge que par rapport à ce
    /// qui existe déjà. C'est ici que se décide **ce qu'Ash accepte de retenir** ; le
    /// frontend a la même règle pour éteindre son bouton, mais il ne fait qu'annoncer
    /// celle-ci.
    pub fn declare(
        self,
        adapters: &[String],
        declared: &[ToolDeclaration],
    ) -> Result<ToolDeclaration, SettingsError> {
        let adapter = self.adapter.trim().to_owned();
        if !adapters.iter().any(|known| known == &adapter) {
            return Err(SettingsError::UnknownAdapter(adapter));
        }
        self.retained(&adapter, declared)
    }

    /// La même saisie, **relue d'un fichier** plutôt que tapée dans le formulaire.
    ///
    /// Les mêmes règles, sauf une : l'adaptateur n'a pas à être embarqué par cette
    /// compilation. Une entrée qui en nomme un que cette version d'Ash ne connaît pas est
    /// **gardée et montrée invalide**, avec la correction qui a une chance — c'est ce que
    /// [`first_pass`](super::verification::Verifier::first_pass) compose déjà pour ce cas
    /// précis, et c'est la conduite que la feature tient partout ailleurs : Ash n'empêche
    /// pas de déclarer, il refuse d'écrire. La faire disparaître ferait perdre sans un mot
    /// un chemin que l'utilisateur avait tapé, en revenant d'une version à la précédente ou
    /// en éditant le fichier à la main (spec §9).
    ///
    /// Ce qu'elle refuse est ce qui ne désigne rien : un nom qui n'est pas un nom de
    /// processus, une entrée sans adaptateur, et un doublon de commande — la clé est le
    /// `command`, et deux entrées homonymes laisseraient Ash sans savoir laquelle
    /// instrumenter.
    pub fn restore(self, declared: &[ToolDeclaration]) -> Result<ToolDeclaration, SettingsError> {
        let adapter = self.adapter.trim().to_owned();
        if adapter.is_empty() {
            return Err(SettingsError::UnknownAdapter(adapter));
        }
        self.retained(&adapter, declared)
    }

    /// Le corps commun des deux : ce qui vaut pour une saisie vaut pour une relecture.
    fn retained(
        self,
        adapter: &str,
        declared: &[ToolDeclaration],
    ) -> Result<ToolDeclaration, SettingsError> {
        // Un `match` est comparé au nom du processus en avant-plan (ADR-0005/0006) : la
        // règle vit dans [`Command`], donc elle est vérifiée ici **et portée ensuite** par
        // tout ce que ce nom traverse.
        let command = Command::parse(&self.command)?;
        if declared.iter().any(|tool| tool.command == command) {
            return Err(SettingsError::DuplicateCommand(command));
        }

        Ok(ToolDeclaration {
            command,
            label: optional(self.label.as_deref()),
            adapter: adapter.to_owned(),
            config: optional(self.config.as_deref()),
            // Une entrée neuve n'a rien prouvé. C'est la seule valeur possible ici, et
            // c'est ce qui garantit qu'Ash n'écrit dans aucun fichier au moment de
            // l'ajout : c'est le registre qui lance ensuite la séquence, avec les ports
            // qu'elle exige — `declare` ne juge que la **saisie**.
            verified: false,
            verification: Verification::unverified(),
            // Rien n'a jamais été valide, personne n'a rien réinitialisé, et ce que les
            // autres entrées font n'est pas de la compétence d'une saisie. Le registre pose
            // les trois dernières valeurs dès qu'il rend la liste.
            last_valid_config: None,
            reset_from: None,
            duplicates: Vec::new(),
            hooks: HooksReport::until_verified(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : une saisie de formulaire valide, dont on ne surcharge que ce
    /// que le scénario regarde.
    struct DraftBuilder {
        command: String,
        label: Option<String>,
        adapter: String,
        config: Option<String>,
    }

    impl DraftBuilder {
        fn new() -> Self {
            Self {
                command: "claude".to_owned(),
                label: None,
                adapter: "generic".to_owned(),
                config: None,
            }
        }

        fn command(mut self, command: &str) -> Self {
            self.command = command.to_owned();
            self
        }

        fn label(mut self, label: &str) -> Self {
            self.label = Some(label.to_owned());
            self
        }

        fn adapter(mut self, adapter: &str) -> Self {
            self.adapter = adapter.to_owned();
            self
        }

        fn config(mut self, config: &str) -> Self {
            self.config = Some(config.to_owned());
            self
        }

        fn build(self) -> NewTool {
            NewTool {
                command: self.command,
                label: self.label,
                adapter: self.adapter,
                config: self.config,
            }
        }
    }

    fn adapters() -> Vec<String> {
        vec!["generic".to_owned(), "claude-code".to_owned()]
    }

    #[test]
    fn given_a_filled_in_form_when_it_is_declared_then_the_entry_is_not_verified_yet() {
        // Given — la pastille « modifié, non enregistré » de la maquette dit exactement
        // ça : une entrée neuve n'a passé aucun des quatre tests de la spec §9.1
        let draft = DraftBuilder::new().command("claude-perso").build();

        // When
        let declared = draft.declare(&adapters(), &[]);

        // Then — et c'est ce qui garantit qu'Ash n'a rien écrit chez l'utilisateur
        assert_eq!(declared.map(|tool| tool.verified), Ok(false));
    }

    #[test]
    fn given_a_blank_optional_field_when_the_entry_is_declared_then_the_field_is_absent_not_empty()
    {
        // Given — un `config` vide se lirait « ce dossier-là » ; l'absence veut dire
        // « le défaut de l'adaptateur », qui n'est pas la même chose
        let draft = DraftBuilder::new().label("   ").config("").build();

        // When
        let declared = draft.declare(&adapters(), &[]);

        // Then
        let tool = declared.expect("la saisie est par ailleurs valide");
        assert_eq!((tool.label, tool.config), (None, None));
    }

    #[test]
    fn given_a_display_label_when_the_entry_is_declared_then_it_rides_along_with_the_command() {
        // Given — `label = "Perso"` de la spec §9 : un libellé d'affichage, jamais la clé
        let draft = DraftBuilder::new()
            .command("claude-perso")
            .label(" Perso ")
            .build();

        // When
        let declared = draft.declare(&adapters(), &[]);

        // Then
        let tool = declared.expect("la saisie est valide");
        assert_eq!(
            (tool.command.as_str(), tool.label.as_deref()),
            ("claude-perso", Some("Perso"))
        );
    }

    #[test]
    fn given_an_entry_that_proved_a_folder_when_a_later_check_only_gives_a_caveat_then_the_memory_keeps_the_folder_that_worked(
    ) {
        // Given — la mémoire est ce que « réinitialiser » restaure (spec §9.1). Si une
        // réserve l'écrasait, le geste ramènerait l'entrée sur un dossier dont on vient
        // justement d'apprendre que la commande ne le lit pas
        let entry = DraftBuilder::new()
            .command("claude-perso")
            .adapter("claude-code")
            .config("~/.claude-perso")
            .build()
            .declare(&adapters(), &[])
            .expect("la saisie est valide");
        let mut passed = Verification::unverified();
        passed.state = VerificationState::Valid;
        let mut reserved = Verification::unverified();
        reserved.state = VerificationState::Caveat;

        // When
        let remembered = entry
            .verified_by(
                passed,
                Some(ConfigTarget::at("~/.claude-perso", "/h/.claude-perso")),
            )
            .verified_by(
                reserved,
                Some(ConfigTarget::at("~/dev/notes", "/h/dev/notes")),
            );

        // Then
        assert_eq!(
            remembered
                .last_valid_config
                .as_ref()
                .map(ConfigTarget::declared),
            Some("~/.claude-perso")
        );
    }

    #[test]
    fn given_a_command_already_declared_when_a_second_entry_repeats_it_then_it_is_refused() {
        // Given — `match` est la clé : deux entrées homonymes désigneraient le même
        // processus, et Ash ne saurait laquelle instrumenter
        let existing = DraftBuilder::new()
            .command("claude")
            .build()
            .declare(&adapters(), &[])
            .expect("la première saisie est valide");

        // When
        let second = DraftBuilder::new()
            .command("claude")
            .build()
            .declare(&adapters(), &[existing]);

        // Then
        assert_eq!(
            second.unwrap_err(),
            SettingsError::DuplicateCommand(Command::parse("claude").expect("un nom valide"))
        );
    }

    #[test]
    fn given_a_command_that_carries_a_path_when_it_is_declared_then_it_is_not_a_command_name() {
        // Given — la sonde compare un **nom de processus** (ADR-0005/0006) : une entrée
        // portant un chemin ne correspondrait jamais, tout en se lisant comme valide
        let draft = DraftBuilder::new().command("/usr/local/bin/claude").build();

        // When
        let declared = draft.declare(&adapters(), &[]);

        // Then
        assert_eq!(
            declared.unwrap_err(),
            SettingsError::NotACommandName("/usr/local/bin/claude".to_owned())
        );
    }

    #[test]
    fn given_an_adapter_this_build_does_not_embed_when_it_is_declared_then_it_is_refused() {
        // Given — les adaptateurs sont ceux qu'ADR-0008 embarque, pas un texte libre :
        // retenir un nom inconnu produirait une entrée que rien ne peut jamais traduire
        let draft = DraftBuilder::new().adapter("kimi-code").build();

        // When
        let declared = draft.declare(&adapters(), &[]);

        // Then
        assert_eq!(
            declared.unwrap_err(),
            SettingsError::UnknownAdapter("kimi-code".to_owned())
        );
    }

    #[test]
    fn given_an_entry_whose_name_and_folders_carry_types_when_it_crosses_the_wire_then_they_are_still_plain_strings(
    ) {
        // Given — les quatre champs que les valeurs ont typés. Un newtype sérialise
        // autrement si l'on n'y prend pas garde : `{"declared":…,"resolved":…}` au lieu de
        // `"~/.claude"` ferait mentir le contrat de `src/features/settings/contract.ts`,
        // et cette régression-là ne se voit qu'à l'exécution dans la webview
        let mut passed = Verification::unverified();
        passed.state = VerificationState::Valid;
        let mut tool = DraftBuilder::new()
            .command("claude-perso")
            .adapter("claude-code")
            .config("~/.claude-perso")
            .build()
            .declare(&adapters(), &[])
            .expect("la saisie est valide")
            .verified_by(
                passed,
                Some(ConfigTarget::at("~/.claude-perso", "/h/.claude-perso")),
            );
        tool.reset_from = Some(ConfigTarget::at("~/.claude", "/h/.claude"));
        tool.duplicates = vec![Command::parse("claude").expect("un nom valide")];

        // When
        let json = serde_json::to_value(&tool).expect("une déclaration se sérialise");

        // Then
        assert_eq!(json["command"], serde_json::json!("claude-perso"));
        assert_eq!(
            json["lastValidConfig"],
            serde_json::json!("~/.claude-perso")
        );
        assert_eq!(json["resetFrom"], serde_json::json!("~/.claude"));
        assert_eq!(json["duplicates"], serde_json::json!(["claude"]));
    }
}
