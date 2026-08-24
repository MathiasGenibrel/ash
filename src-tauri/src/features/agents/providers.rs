//! **Reconnaître un outil**, c'est-à-dire ce qu'ADR-0006 appelle « les agents sont
//! découverts, pas déclarés ».
//!
//! La feature possède la notion d'agent et le trait [`Adapter`](super::Adapter) : la table
//! des outils connus vit donc ici, et pas dans `pty`. Le registre de PTY **demande** — comme
//! il demande déjà l'état d'un onglet — et ne déduit rien lui-même.
//!
//! **Reconnaître est de la lecture** : aucun fichier n'est écrit, aucune autorisation macOS
//! n'est demandée, et rien n'est parcouru sur le disque. Ce module est une fonction pure de
//! ce que la sonde a vu ([ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md)).
//!
//! ## Pourquoi trois signaux, et dans cet ordre
//!
//! Une table qui reconnaîtrait les outils par leur seul **nom de commande** ne
//! correspondrait à rien pour l'installation la plus répandue : l'installateur officiel de
//! Claude Code pose un binaire dont le nom de fichier est le numéro de version, et
//! `~/.local/bin/claude` n'est qu'un lien vers lui. La sonde lit `proc_pidpath`, donc le
//! chemin réel — l'onglet affichait `2.1.234` aujourd'hui, `2.1.235` demain.
//!
//! | Signal | Ce qu'il reconnaît | Cas |
//! |---|---|---|
//! | Le chemin d'installation | `~/.local/share/claude/versions/*` | l'installateur officiel |
//! | Le nom de l'exécutable | `~/.kimi-code/bin/kimi` | les outils qui gardent leur nom |
//! | `argv[0]` | `claude` alors que l'exécutable est `node` | l'installation npm |
//!
//! Ils sont essayés du plus fiable au moins fiable, et le premier qui répond décide. Le
//! chemin passe **avant** le nom parce qu'il est le seul à survivre à une mise à jour de
//! version ; `argv[0]` vient en dernier parce qu'un processus choisit le sien, alors que
//! personne ne choisit son `proc_pidpath`.
//!
//! ## Ce que la reconnaissance ne fait pas
//!
//! Elle ne produit **aucun état d'agent** : elle dit qu'un onglet porte un outil connu, pas
//! ce que cet outil est en train de faire. `waiting` n'a jamais d'autre source qu'un hook
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)), et c'est précisément ce que
//! [`Instrumented`] sert à dire à l'écran quand la configuration de l'outil ne porte pas le
//! marqueur d'Ash.

use std::path::{Component, Path, PathBuf};

/// Ce que la sonde sait du programme qui tient l'avant-plan d'un onglet.
///
/// Trois faits, aucune conclusion — c'est la frontière entre `probe`, qui observe le
/// système, et cette feature, qui décide. La sonde ne porte aucune règle de provider :
/// c'est ce qui la garde testable sans lancer un seul processus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramIdentity {
    /// Le chemin **entier** de l'exécutable, tel que `proc_pidpath` l'a rendu.
    pub executable: PathBuf,
    /// Son dernier segment — ce que l'onglet affichait jusqu'ici.
    pub name: String,
    /// Le premier mot de sa ligne de commande, quand le système l'a dit.
    pub argv0: Option<String>,
}

impl ProgramIdentity {
    /// Le nom que `argv[0]` désigne, réduit à son dernier segment.
    ///
    /// Un `argv[0]` porte parfois un chemin entier — c'est ce que fait un shell qui lance un
    /// programme par son chemin absolu. Ce qui se compare à une commande reconnue est le
    /// **nom**, jamais le chemin (voir `settings::Command`).
    fn argv0_name(&self) -> Option<&str> {
        self.argv0
            .as_deref()
            .and_then(|argv0| Path::new(argv0).file_name()?.to_str())
    }
}

/// Un outil que cette version d'Ash sait reconnaître.
///
/// La table est **embarquée**, et ses chemins sont vérifiés un par un contre ce que la sonde
/// a vu : rien n'est parcouru sur le disque, et Ash ne cherche jamais un outil de lui-même.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    /// Le nom sous lequel l'outil se tape, et sous lequel sa ligne s'affiche.
    pub command: &'static str,
    /// L'identifiant de l'adaptateur qui le traduit ([`Adapter::id`](super::Adapter::id)).
    pub adapter: &'static str,
    /// Les chemins d'installation qui le désignent, quel que soit le nom du fichier.
    ///
    /// Chacun est une **suite de segments** cherchée dans le chemin de l'exécutable, et non
    /// un préfixe : le foyer de l'utilisateur n'a pas à être connu ici, et un outil installé
    /// sous un autre compte se reconnaît de la même façon.
    pub installed_at: &'static [&'static str],
}

/// Les outils connus de cette version d'Ash.
///
/// **`generic` n'est pas un manque** : c'est l'adaptateur de l'outil dont on ne sait rien
/// ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)). Un outil qu'il traduit est
/// reconnu — sa ligne porte son nom — mais il ne pose aucun hook, donc il ne montrera jamais
/// `waiting` : c'est ce que [`Instrumented::Unsupported`] dit à l'écran, plutôt que de
/// laisser lire une panne.
pub const KNOWN_PROVIDERS: &[Provider] = &[
    Provider {
        command: "claude",
        adapter: "claude-code",
        // Le chemin de l'installateur officiel. Le binaire s'appelle `2.1.234` : c'est le
        // seul signal qui survive à une mise à jour de version.
        installed_at: &[".local/share/claude/versions"],
    },
    Provider {
        command: "codex",
        adapter: "generic",
        installed_at: &[],
    },
    Provider {
        command: "kimi",
        adapter: "generic",
        installed_at: &[".kimi-code/bin"],
    },
    Provider {
        command: "opencode",
        adapter: "generic",
        installed_at: &[],
    },
];

/// Un outil reconnu, avant qu'on sache ce que sa configuration porte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecognizedProvider {
    /// Le nom de l'outil — `claude`, et non `2.1.234`.
    pub command: String,
    pub adapter: String,
}

/// Ce que la configuration d'un outil reconnu porte, du point de vue d'Ash.
///
/// Ce n'est **pas** un état d'agent, et les deux ne doivent jamais se confondre : un outil
/// non instrumenté vit très bien, il montre `idle` et `working` comme avant. Ce que le
/// marqueur dit, c'est *pourquoi* il ne montrera jamais `waiting`
/// ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)) — sans quoi son absence se
/// lirait comme une panne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum Instrumented {
    /// La configuration de cet outil porte le marqueur `# ash:hook v`.
    Installed,
    /// Elle ne le porte pas — et son adaptateur saurait le poser.
    Missing,
    /// Aucun adaptateur de cette version ne sait instrumenter cet outil.
    Unsupported,
}

/// Un outil reconnu dans l'avant-plan d'un onglet, tel qu'il traverse la frontière.
///
/// Il voyage dans le `TabInfo` et **ne change pas d'une passe de sonde à l'autre** tant que
/// le même programme tient l'avant-plan : la fiche d'onglet est comparée entière pour
/// décider s'il faut émettre `ash://tab-changed`, donc un champ qui bougerait toutes les
/// 300 ms réveillerait la sidebar en permanence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RecognizedAgent {
    pub command: String,
    pub adapter: String,
    pub instrumented: Instrumented,
}

/// L'outil que ce programme est, ou rien.
///
/// `declared` est ce que l'utilisateur a déclaré dans la fenêtre de réglages — ou écrit à la
/// main dans `~/.ash/tools.json` (spec §9) —,
/// et il **l'emporte** sur la table embarquée : c'est lui qui décide de l'adaptateur et du
/// dossier de configuration, et c'est ce qui permet de reconnaître un outil qu'Ash ne connaît
/// pas — ou d'en corriger un qu'il connaît mal. Une entrée déclarée qui répète un outil de la
/// table ne le fait donc pas apparaître deux fois : il n'y a qu'une réponse.
///
/// La comparaison porte sur un **nom** des deux côtés — le nom de l'exécutable, puis
/// `argv[0]` —, jamais sur un chemin : c'est la règle que `settings::Command` porte déjà.
pub fn recognize(program: &ProgramIdentity, declared: &[Declared]) -> Option<RecognizedProvider> {
    // Une entrée déclarée reconnaît un outil que la table ignore : c'est le cas qu'ADR-0006
    // nomme — un outil lancé autrement, ou qu'Ash ne connaît pas.
    let by_hand = declared
        .iter()
        .find(|entry| names(program).any(|name| name == entry.command))
        .map(|entry| RecognizedProvider {
            command: entry.command.clone(),
            adapter: entry.adapter.clone(),
        });

    let found = by_hand.or_else(|| {
        // Le chemin d'abord : c'est le seul signal qu'une mise à jour de version ne casse pas.
        KNOWN_PROVIDERS
            .iter()
            .find(|provider| {
                provider
                    .installed_at
                    .iter()
                    .any(|marker| carries(&program.executable, marker))
            })
            .or_else(|| {
                KNOWN_PROVIDERS
                    .iter()
                    .find(|provider| names(program).any(|name| name == provider.command))
            })
            .map(|provider| RecognizedProvider {
                command: provider.command.to_owned(),
                adapter: provider.adapter.to_owned(),
            })
    })?;

    // La précédence se rejoue sur le **nom trouvé**, et pas seulement sur ce que la sonde a
    // vu : un binaire versionné ne s'appelle `claude` qu'une fois reconnu par son chemin, et
    // sans cette seconde passe la déclaration de l'utilisateur ne s'appliquerait jamais à
    // l'installation la plus courante. Il n'y a toujours qu'une réponse — l'outil n'apparaît
    // pas deux fois.
    let overridden = declared
        .iter()
        .find(|entry| entry.command == found.command)
        .map(|entry| entry.adapter.clone());

    Some(RecognizedProvider {
        adapter: overridden.unwrap_or(found.adapter),
        command: found.command,
    })
}

/// Une entrée déclarée à la main, réduite à ce que la reconnaissance regarde.
///
/// Le type vit ici plutôt que dans `settings` pour que cette fonction reste pure et
/// vérifiable seule : la feature qui tient les déclarations les traduit en ceci, et la règle
/// de précédence n'a qu'un seul endroit où se lire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub command: String,
    pub adapter: String,
}

/// Les noms sous lesquels un programme peut se reconnaître, du plus fiable au moins fiable.
fn names(program: &ProgramIdentity) -> impl Iterator<Item = &str> {
    std::iter::once(program.name.as_str()).chain(program.argv0_name())
}

/// Le chemin traverse-t-il cette suite de segments ?
///
/// Une comparaison de **segments** et non de texte : `contains` ferait correspondre
/// `/tmp/not.local/share/claude/versions-old/x`, et un chemin qui ressemble n'est pas un
/// chemin qui est.
fn carries(executable: &Path, marker: &str) -> bool {
    let wanted: Vec<&str> = Path::new(marker)
        .components()
        .filter_map(|part| match part {
            Component::Normal(segment) => segment.to_str(),
            _ => None,
        })
        .collect();
    if wanted.is_empty() {
        return false;
    }

    let segments: Vec<&str> = executable
        .components()
        .filter_map(|part| match part {
            Component::Normal(segment) => segment.to_str(),
            _ => None,
        })
        .collect();

    segments
        .windows(wanted.len())
        .any(|window| window == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : un programme en avant-plan, décrit par le seul signal du scénario.
    struct ProgramBuilder {
        executable: PathBuf,
        argv0: Option<String>,
    }

    impl ProgramBuilder {
        fn at(executable: &str) -> Self {
            Self {
                executable: PathBuf::from(executable),
                argv0: None,
            }
        }

        fn announcing(mut self, argv0: &str) -> Self {
            self.argv0 = Some(argv0.to_owned());
            self
        }

        fn build(self) -> ProgramIdentity {
            let name = self
                .executable
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            ProgramIdentity {
                executable: self.executable,
                name,
                argv0: self.argv0,
            }
        }
    }

    fn recognized(program: ProgramIdentity) -> Option<RecognizedProvider> {
        recognize(&program, &[])
    }

    #[test]
    fn given_claude_code_installed_by_its_official_installer_when_it_takes_the_foreground_then_the_tab_says_claude(
    ) {
        // Given — l'installateur pose un binaire dont le **nom de fichier est la version** ;
        // `~/.local/bin/claude` n'est qu'un lien. L'onglet affichait donc `2.1.234`
        let program = ProgramBuilder::at("/Users/ash/.local/share/claude/versions/2.1.234").build();

        // When
        let found = recognized(program);

        // Then
        assert_eq!(
            found,
            Some(RecognizedProvider {
                command: "claude".to_owned(),
                adapter: "claude-code".to_owned(),
            })
        );
    }

    #[test]
    fn given_the_same_installation_after_an_update_when_it_takes_the_foreground_then_it_is_still_recognized(
    ) {
        // Given — c'est **la** raison pour laquelle le chemin passe avant le nom : demain
        // le fichier s'appellera autrement, et une table de noms ne matcherait plus rien
        let updated = ProgramBuilder::at("/Users/ash/.local/share/claude/versions/2.1.999").build();

        // When
        let found = recognized(updated);

        // Then
        assert_eq!(
            found.map(|provider| provider.command),
            Some("claude".to_owned())
        );
    }

    #[test]
    fn given_a_tool_that_keeps_its_own_name_when_it_takes_the_foreground_then_its_executable_name_recognizes_it(
    ) {
        // Given — le deuxième signal : `kimi` s'installe ailleurs et garde son nom
        let program = ProgramBuilder::at("/Users/ash/.kimi-code/bin/kimi").build();

        // When
        let found = recognized(program);

        // Then
        assert_eq!(
            found,
            Some(RecognizedProvider {
                command: "kimi".to_owned(),
                // Aucun adaptateur dédié n'est embarqué : `generic` reconnaît sans
                // instrumenter, et c'est ce que l'écran devra dire (ADR-0008).
                adapter: "generic".to_owned(),
            })
        );
    }

    #[test]
    fn given_a_tool_installed_by_npm_when_it_takes_the_foreground_then_its_command_line_recognizes_it(
    ) {
        // Given — le troisième signal : l'exécutable est `node`, et seul `argv[0]` dit
        // quel outil tourne
        let program = ProgramBuilder::at("/opt/homebrew/bin/node")
            .announcing("claude")
            .build();

        // When
        let found = recognized(program);

        // Then
        assert_eq!(
            found.map(|provider| provider.command),
            Some("claude".to_owned())
        );
    }

    #[test]
    fn given_an_ordinary_program_in_the_foreground_when_it_is_examined_then_nothing_is_recognized()
    {
        // Given — la garantie qui compte autant que les trois signaux : un onglet ne devient
        // pas un agent parce qu'on y a lancé un éditeur (ADR-0006)
        let program = ProgramBuilder::at("/usr/bin/vim").build();

        // When
        let found = recognized(program);

        // Then
        assert_eq!(found, None);
    }

    #[test]
    fn given_a_binary_whose_name_looks_like_a_version_but_lives_elsewhere_when_it_is_examined_then_it_is_not_claude(
    ) {
        // Given — le marqueur est une **suite de segments**, pas un morceau de texte : un
        // chemin qui y ressemble ferait passer n'importe quel binaire pour un agent
        let lookalike =
            ProgramBuilder::at("/tmp/not.local/share/claude/versions-old/2.1.234").build();

        // When
        let found = recognized(lookalike);

        // Then
        assert_eq!(found, None);
    }

    #[test]
    fn given_an_entry_declared_by_hand_when_a_program_carries_its_name_then_the_declaration_wins() {
        // Given — la spec §9 autorise à écrire `~/.ash/tools.json` à la main, et ADR-0006
        // en fait la source qui l'emporte : c'est ainsi qu'on corrige un outil qu'Ash
        // connaît mal, ou qu'on en ajoute un qu'il ne connaît pas
        let program = ProgramBuilder::at("/Users/ash/.local/share/claude/versions/2.1.234").build();
        let declared = [Declared {
            command: "claude".to_owned(),
            adapter: "generic".to_owned(),
        }];

        // When
        let found = recognize(&program, &declared);

        // Then — une seule réponse, et c'est celle de l'utilisateur : l'outil n'apparaît pas
        // deux fois, une fois par la table et une fois par sa déclaration
        assert_eq!(
            found,
            Some(RecognizedProvider {
                command: "claude".to_owned(),
                adapter: "generic".to_owned(),
            })
        );
    }

    #[test]
    fn given_a_tool_ash_does_not_know_when_it_is_declared_by_hand_then_it_is_recognized_anyway() {
        // Given — le cas qu'ADR-0006 nomme explicitement : un outil lancé autrement, ou
        // qu'aucune table embarquée ne connaît. La configuration permet de l'ajouter
        let program = ProgramBuilder::at("/Users/ash/bin/aider").build();
        let declared = [Declared {
            command: "aider".to_owned(),
            adapter: "generic".to_owned(),
        }];

        // When
        let found = recognize(&program, &declared);

        // Then
        assert_eq!(
            found.map(|provider| provider.command),
            Some("aider".to_owned())
        );
    }

    #[test]
    fn given_a_declared_entry_when_a_program_announces_it_through_npm_then_it_is_recognized_too() {
        // Given — une déclaration à la main doit profiter des trois signaux, pas seulement du
        // premier : sinon elle ne servirait qu'aux outils déjà les plus faciles à voir
        let program = ProgramBuilder::at("/opt/homebrew/bin/node")
            .announcing("/opt/homebrew/lib/node_modules/.bin/aider")
            .build();
        let declared = [Declared {
            command: "aider".to_owned(),
            adapter: "generic".to_owned(),
        }];

        // When
        let found = recognize(&program, &declared);

        // Then — c'est le **nom** qui se compare, jamais le chemin
        assert_eq!(
            found.map(|provider| provider.command),
            Some("aider".to_owned())
        );
    }
}
