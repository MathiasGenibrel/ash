use super::error::SettingsError;
use super::verification::Verification;

/// Une commande reconnue, telle que la spec §9 et
/// [ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md) la décrivent.
///
/// C'est le `[[command]]` de `~/.ash/config.toml`, un pour un :
///
/// ```toml
/// [[command]]
/// match   = "claude-perso"
/// label   = "Perso"
/// adapter = "claude-code"
/// config  = "~/.claude-perso"
/// ```
///
/// **`command` est l'identité**, et il n'y a pas d'autre identifiant : c'est le `match` du
/// fichier, c'est-à-dire le nom de processus que la sonde compare. Poser un ulid à côté
/// donnerait deux clés pour une seule chose, et rien ne dirait laquelle fait foi le jour
/// où le fichier est édité à la main — ce que la spec §9 autorise explicitement.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDeclaration {
    /// Le `match` : le nom du processus, tel qu'on le tape dans le shell.
    pub command: String,
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
    /// oui/non — l'écriture dans `~/.ash/config.toml`, le compteur de l'en-tête — n'aient
    /// pas à traverser la structure entière. [`ToolDeclaration::verified_by`] est le seul
    /// endroit où les deux sont écrits, donc le seul endroit où ils pourraient diverger.
    pub verified: bool,

    /// Ce que les quatre tests de la spec §9.1 ont dit de cette entrée.
    ///
    /// Elle vit **avec** la déclaration et non à côté : une entrée dont le chemin change
    /// change de vérification au même instant, et deux tables séparées laisseraient un
    /// intervalle où l'écran montrerait le résultat de l'ancien chemin sous le nouveau.
    pub verification: Verification,
}

impl ToolDeclaration {
    /// Attache un résultat de vérification à une entrée.
    ///
    /// Le seul chemin par lequel [`ToolDeclaration::verified`] change de valeur.
    #[must_use]
    pub fn verified_by(mut self, verification: Verification) -> Self {
        self.verified = verification.allows_hooks;
        self.verification = verification;
        self
    }
}

/// Ce que le formulaire d'ajout envoie — du texte brut, pas encore une déclaration.
///
/// Distinct de [`ToolDeclaration`] parce que les deux n'ont pas les mêmes invariants : ici
/// tout est possible, y compris des espaces autour et un adaptateur qui n'existe pas.
/// [`NewTool::declare`] est le seul passage de l'un à l'autre.
#[derive(Debug, Clone, serde::Deserialize)]
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
        let command = self.command.trim();
        if command.is_empty() {
            return Err(SettingsError::EmptyCommand);
        }
        // Un `match` est comparé au nom du processus en avant-plan (ADR-0005/0006) : une
        // espace ou une barre oblique n'y apparaît jamais, donc une telle entrée ne
        // correspondrait à rien — et se lirait pourtant comme une entrée valide.
        if command.contains(char::is_whitespace) || command.contains('/') {
            return Err(SettingsError::NotACommandName(command.to_owned()));
        }
        if declared.iter().any(|tool| tool.command == command) {
            return Err(SettingsError::DuplicateCommand(command.to_owned()));
        }

        let adapter = self.adapter.trim();
        if !adapters.iter().any(|known| known == adapter) {
            return Err(SettingsError::UnknownAdapter(adapter.to_owned()));
        }

        Ok(ToolDeclaration {
            command: command.to_owned(),
            label: optional(self.label.as_deref()),
            adapter: adapter.to_owned(),
            config: optional(self.config.as_deref()),
            // Une entrée neuve n'a rien prouvé. C'est la seule valeur possible ici, et
            // c'est ce qui garantit qu'Ash n'écrit dans aucun fichier au moment de
            // l'ajout : c'est le registre qui lance ensuite la séquence, avec les ports
            // qu'elle exige — `declare` ne juge que la **saisie**.
            verified: false,
            verification: Verification::unverified(),
        })
    }
}

/// Un champ facultatif : rendu vide, il est absent — pas vide.
///
/// La différence compte pour `config` : une chaîne vide se lirait « ce dossier-là », alors
/// que l'absence veut dire « celui de l'adaptateur », que l'adaptateur est seul à savoir.
fn optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
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
            SettingsError::DuplicateCommand("claude".to_owned())
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
}
