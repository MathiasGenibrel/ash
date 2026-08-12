use std::path::{Path, PathBuf};

use crate::features::agents::adapter::{Adapter, Instrumentation, RawEvent, SubagentSupport};
use crate::features::agents::state::AgentState;

/// La version du bloc que cet adaptateur compose.
///
/// Elle s'incrémente **dès que la forme du bloc change** — un hook ajouté, une invocation
/// réécrite, un champ renommé. C'est elle que la feature `hooks` inscrit dans le marqueur
/// (`ash block v1`), et c'est elle qui lui permet de reconnaître un bloc écrit par un Ash
/// plus ancien et de le réécrire, sans avoir à comparer deux textes ni à se demander si
/// l'utilisateur y a touché.
///
/// **Elle vaut 1, et c'est la première.** La spec §10 écrit `ash block v3` : ce `v3` est
/// une illustration de la *forme* du marqueur, rédigée avant la moindre ligne de code, et
/// il n'a jamais eu de v1 ni de v2 pour le précéder. Le nombre est le compteur réel, pas
/// celui de la spec.
const BLOCK_VERSION: u32 = 1;

/// Les hooks de Claude Code qu'Ash installe, et l'état que chacun déclare.
///
/// C'est **la** table de traduction de cet adaptateur, et elle est lue dans les deux sens :
/// [`ClaudeCodeAdapter::instrumentation`] la parcourt pour composer le bloc, et
/// [`ClaudeCodeAdapter::interpret`] accepte exactement le vocabulaire qu'elle peut produire.
/// Une seule table, donc aucun moyen d'écrire dans le `settings.json` de l'utilisateur un
/// mot qu'Ash ne saura pas relire — c'est ce que prouve le test d'aller-retour.
///
/// La sémantique de chaque hook, telle qu'elle a été retenue :
///
/// | Hook | Quand il part | État |
/// |---|---|---|
/// | `UserPromptSubmit` | l'utilisateur vient d'envoyer un prompt | `working` |
/// | `PreToolUse` | juste avant un outil, donc **après** l'accord de l'utilisateur | `working` |
/// | `Notification` | Claude demande une permission, ou signale une saisie en attente | `waiting` |
/// | `Stop` | l'agent a fini son tour et rend le clavier | `waiting` |
/// | `SessionEnd` | la session se termine | `done` |
///
/// Les deux choix qui ne vont pas de soi :
///
/// - **`Stop` → `waiting`, et non `done`.** `Stop` ne termine rien : il clôt un *tour*, et
///   `claude` reste au premier plan, à son invite, à attendre. La spec §6.2 range `done` et
///   `error` parmi « les deux issues exclusives d'une même **terminaison** », toutes deux
///   suivies du retour à une ligne shell — ce qui serait faux d'un agent bien vivant. Et
///   c'est aussi le moment qui *mérite* d'interrompre l'utilisateur (spec §8), ce qui est
///   la définition même de `waiting`.
/// - **`UserPromptSubmit` ne figure pas dans l'énumération d'ADR-0007.** Sans lui, la
///   flèche « réponse » du diagramme §6.2 n'a de producteur que `PreToolUse` — donc un tour
///   qui ne lance aucun outil laisserait l'onglet en `waiting` pendant que l'agent travaille.
///   C'est un vrai hook de Claude Code, et il dit exactement « l'utilisateur a répondu ».
const HOOKS: [(&str, AgentState); 5] = [
    ("UserPromptSubmit", AgentState::Working),
    ("PreToolUse", AgentState::Working),
    ("Notification", AgentState::Waiting),
    ("Stop", AgentState::Waiting),
    ("SessionEnd", AgentState::Done),
];

/// Le seul hook qui exige un sélecteur d'outils, et la valeur qui les prend tous.
///
/// Claude Code filtre `PreToolUse` par `matcher` ; les hooks de cycle de vie n'en ont pas.
/// L'omettre là où il est attendu ferait un bloc que l'outil ignore en silence.
const MATCH_EVERY_TOOL: (&str, &str) = ("PreToolUse", "*");

/// Claude Code, première implémentation du trait
/// ([ADR-0008](../../../../../docs/adr/0008-abstraction-adapter.md)).
///
/// Elle est *une* implémentation, pas le cas normal : tout ce qui lui est propre — le nom
/// de ses hooks, la forme JSON de son `settings.json`, le sélecteur d'outils — reste dans
/// ce fichier, et n'en sort que sous la forme des cinq mots communs et d'une
/// [`Instrumentation`] que la feature `hooks` écrit sans la relire.
///
/// **Elle n'écrit rien.** Voir la documentation d'[`Instrumentation`] : l'écriture chez
/// l'utilisateur n'a qu'un propriétaire dans le code.
pub struct ClaudeCodeAdapter {
    /// Le chemin **absolu** d'`ash-event`, tel qu'il partira dans le `settings.json`.
    ///
    /// C'est un champ et non une constante parce que le bloc serait inutilisable sans lui :
    /// Claude Code exécute la commande du hook par un shell, dont le `PATH` n'a aucune
    /// raison de contenir le dossier d'une application `.app`. Écrire `ash-event` tout court
    /// obligerait à poser un shim ou à toucher au `PATH` de l'utilisateur — ce que la spec
    /// §10 exclut nommément (« pas de `.zshrc`, pas de `PATH`, pas de shim »). Un chemin
    /// absolu ne touche à rien et fonctionne dès la première session.
    event_binary: PathBuf,
}

impl ClaudeCodeAdapter {
    pub fn new(event_binary: PathBuf) -> Self {
        Self { event_binary }
    }

    /// `ash-event` tel qu'il est livré : à côté du binaire de l'application.
    ///
    /// C'est la composition root qui a le droit d'appeler ceci — la découverte du chemin de
    /// l'exécutable courant est un effet système, et l'adaptateur reste testable parce que
    /// le chemin lui est **donné** partout ailleurs.
    pub fn beside_the_app() -> Option<Self> {
        let executable = std::env::current_exe().ok()?;
        Some(Self::new(executable.parent()?.join("ash-event")))
    }

    /// La ligne de commande d'un hook, telle qu'un shell la lira.
    fn invocation(&self, state: AgentState) -> Option<String> {
        Some(format!(
            "{} {} --tab \"$ASH_TAB_ID\"",
            shell_quoted(&self.event_binary),
            declared_word(state)?
        ))
    }
}

impl Adapter for ClaudeCodeAdapter {
    fn id(&self) -> &str {
        "claude-code"
    }

    /// Le bloc à écrire dans `<config_dir>/settings.json`.
    ///
    /// Le dossier vient du paramètre et de nulle part ailleurs : `claude` et `claude-perso`
    /// sont deux dossiers, donc deux fichiers et deux blocs (ADR-0007). Un chemin en dur ici
    /// ferait écrire les deux comptes au même endroit — c'est l'invariant
    /// `InstrumentationIsPerConfigDir` de la suite contractuelle.
    ///
    /// Le texte est produit par `serde_json`, jamais par concaténation : le chemin
    /// d'`ash-event` finit dans une chaîne JSON, et c'est le sérialiseur qui doit décider
    /// comment y échapper un guillemet ou un antislash.
    fn instrumentation(&self, config_dir: &Path) -> Option<Instrumentation> {
        let mut hooks = serde_json::Map::new();
        for (hook, state) in HOOKS {
            hooks.insert(hook.to_owned(), self.entry_for(hook, state)?);
        }

        Some(Instrumentation {
            file: config_dir.join("settings.json"),
            block: as_object_entries(&serde_json::json!({ "hooks": hooks }))?,
            version: BLOCK_VERSION,
        })
    }

    /// Le mot reçu sur le socket, ramené aux cinq du produit.
    ///
    /// Ce qui arrive ici n'est **pas** le nom du hook de Claude Code : la forme canonique de
    /// l'invocation est `ash-event <état> --tab <id>` (spec §6.3), donc la traduction
    /// `Stop → waiting` a déjà eu lieu — dans [`HOOKS`], au moment de composer le bloc. Ce
    /// que fait cette méthode est de garder la porte : elle n'accepte que le vocabulaire que
    /// le bloc peut produire, et refuse tout le reste plutôt que de deviner.
    ///
    /// `Stop`, `Notification` et les autres noms de hooks sont donc refusés, et c'est
    /// volontaire : les accepter ferait deux façons de dire la même chose, dont une qu'Ash
    /// n'écrit nulle part.
    fn interpret(&self, raw: &RawEvent) -> Option<AgentState> {
        // `idle` n'est jamais déclarable : c'est le mot de la sonde pour « aucun agent
        // ici », et un outil qui parle est la preuve du contraire.
        [
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Done,
            AgentState::Error,
        ]
        .into_iter()
        .find(|state| declared_word(*state) == Some(raw.kind()))
    }

    /// Claude Code a bien des sous-tâches — l'outil `Task`, et le hook `SubagentStop`.
    ///
    /// On répond pourtant `None`, qui se lit « n'en dit rien » : ce bloc n'installe pas
    /// `SubagentStop`, donc aucun événement de sous-tâche ne remontera. Déclarer `Reported`
    /// annoncerait au cœur des lignes filles qui n'arriveraient jamais. À reprendre avec la
    /// tranche des subagents (spec §6.5).
    fn subagents(&self) -> SubagentSupport {
        SubagentSupport::None
    }
}

impl ClaudeCodeAdapter {
    /// Une entrée de la table `hooks` de Claude Code, dans sa forme attendue.
    fn entry_for(&self, hook: &str, state: AgentState) -> Option<serde_json::Value> {
        let command = serde_json::json!({
            "type": "command",
            "command": self.invocation(state)?,
        });

        let mut group = serde_json::Map::new();
        let (filtered_hook, matcher) = MATCH_EVERY_TOOL;
        if hook == filtered_hook {
            group.insert("matcher".to_owned(), matcher.into());
        }
        group.insert("hooks".to_owned(), serde_json::Value::Array(vec![command]));

        Some(serde_json::Value::Array(vec![serde_json::Value::Object(
            group,
        )]))
    }
}

/// Le mot que le bloc écrit pour cet état, et celui qu'`ash-event` recevra.
///
/// `None` pour `idle`, qui n'est pas déclarable : aucun agent ne dit « je n'existe pas ».
fn declared_word(state: AgentState) -> Option<&'static str> {
    match state {
        AgentState::Working => Some("working"),
        AgentState::Waiting => Some("waiting"),
        AgentState::Done => Some("done"),
        AgentState::Error => Some("error"),
        AgentState::Idle => None,
    }
}

/// Un chemin, rendu inerte pour le shell qui exécutera la ligne du hook.
///
/// **C'est une frontière de sécurité, pas une coquetterie de formatage.** Ce chemin part
/// dans une ligne de commande que Claude Code fait exécuter par un shell, à chaque hook.
/// Un dossier d'application contenant une apostrophe, un `;` ou un `$` suffirait sinon à
/// faire exécuter autre chose qu'`ash-event` — et le fichier où ça se produirait est celui
/// de l'utilisateur, pas le nôtre.
///
/// Les guillemets simples neutralisent tout sauf eux-mêmes ; l'apostrophe se ferme, s'échappe
/// et se rouvre (`'\''`), qui est la seule forme portable.
fn shell_quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// Le corps d'un objet JSON — ses entrées, indentées de deux espaces, sans les accolades.
///
/// La feature `hooks` insère ce texte **tel quel** entre ses marqueurs, à l'intérieur de
/// l'objet racine du `settings.json`. Rendre l'objet complet obligerait `hooks` à le
/// rouvrir pour en retirer les accolades, donc à connaître le format de l'outil.
fn as_object_entries(value: &serde_json::Value) -> Option<String> {
    let pretty = serde_json::to_string_pretty(value).ok()?;
    let body = pretty.strip_prefix("{\n")?.strip_suffix("\n}")?;
    (!body.trim().is_empty()).then(|| body.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::contract::check_adapter_contract;

    /// L'adaptateur tel qu'un test le veut : un chemin d'`ash-event` connu, donc un bloc
    /// dont on peut citer le contenu exact.
    fn adapter() -> ClaudeCodeAdapter {
        ClaudeCodeAdapter::new(PathBuf::from(
            "/Applications/Ash.app/Contents/MacOS/ash-event",
        ))
    }

    /// Les événements que ce bloc fera réellement remonter — la forme canonique de la
    /// spec §6.3, pas les noms de hooks de Claude Code.
    fn own_events() -> Vec<RawEvent> {
        ["working", "waiting", "done", "error"]
            .into_iter()
            .map(RawEvent::new)
            .collect()
    }

    #[test]
    fn given_the_claude_code_adapter_when_it_is_run_through_the_adapter_contract_then_it_holds_every_invariant(
    ) {
        // Given — la deuxième implémentation à passer la suite que toute implémentation
        // doit passer, et la première qui instrumente réellement quelque chose.
        let adapter = adapter();

        // When
        let report = check_adapter_contract(&adapter, &own_events());

        // Then
        assert!(report.is_satisfied(), "violations :\n{report}");
    }

    #[test]
    fn given_the_block_it_writes_when_each_word_comes_back_from_the_socket_then_the_adapter_understands_it_all(
    ) {
        // Given — le bloc part dans un fichier, revient par un socket, et rien ne tient les
        // deux bouts à la compilation. Un mot écrit dans le `settings.json` que
        // `interpret` ne saurait pas relire serait un état perdu en silence, et
        // introuvable : il faudrait lancer un vrai agent pour s'en apercevoir.
        let adapter = adapter();
        let block = adapter
            .instrumentation(Path::new("/home/someone/.claude"))
            .map(|instrumentation| instrumentation.block)
            .unwrap_or_default();

        // When — les mots que la table peut écrire, relus tels qu'`ash-event` les postera
        let round_trip: Vec<Option<AgentState>> = HOOKS
            .iter()
            .map(|(_, state)| {
                let word = declared_word(*state).unwrap_or("");
                assert!(
                    block.contains(&format!(r#"ash-event' {word} --tab \"$ASH_TAB_ID\""#)),
                    "le bloc n'écrit pas « {word} » :\n{block}"
                );
                adapter.interpret(&RawEvent::new(word))
            })
            .collect();

        // Then
        let expected: Vec<Option<AgentState>> =
            HOOKS.iter().map(|(_, state)| Some(*state)).collect();
        assert_eq!(round_trip, expected);
    }

    #[test]
    fn given_a_hook_name_of_claude_code_when_it_arrives_on_the_socket_then_it_is_refused_rather_than_translated(
    ) {
        // Given — la traduction `Stop → waiting` a lieu dans le bloc, une fois pour toutes
        // (spec §6.3). L'accepter aussi ici ferait deux vocabulaires sur le fil, dont un
        // qu'Ash n'écrit nulle part et que personne ne testerait jamais.
        let adapter = adapter();
        let tool_words = ["Stop", "Notification", "PreToolUse", "SessionEnd", "idle"];

        // When
        let interpreted: Vec<_> = tool_words
            .iter()
            .map(|word| adapter.interpret(&RawEvent::new(*word)))
            .collect();

        // Then
        assert_eq!(interpreted, vec![None; tool_words.len()]);
    }

    #[test]
    fn given_two_claude_accounts_when_each_config_dir_is_instrumented_then_they_get_two_separate_files(
    ) {
        // Given — `claude` et `claude-perso`, le cas nommé par ADR-0007. C'est la seule
        // chose qui distingue les deux comptes : Ash n'a pas de notion de profil.
        let adapter = adapter();

        // When
        let pro = adapter.instrumentation(Path::new("/home/someone/.claude"));
        let perso = adapter.instrumentation(Path::new("/home/someone/.claude-perso"));

        // Then
        assert_eq!(
            pro.map(|instrumentation| instrumentation.file),
            Some(PathBuf::from("/home/someone/.claude/settings.json"))
        );
        assert_eq!(
            perso.map(|instrumentation| instrumentation.file),
            Some(PathBuf::from("/home/someone/.claude-perso/settings.json"))
        );
    }

    #[test]
    fn given_an_installation_path_carrying_shell_metacharacters_when_the_hook_line_is_written_then_nothing_escapes_the_quotes(
    ) {
        // Given — la ligne du bloc est exécutée par un shell à chaque hook. Un dossier
        // d'application nommé par l'utilisateur suffirait, sans les guillemets, à faire
        // lancer autre chose qu'`ash-event` depuis son propre `settings.json`.
        let hostile = ClaudeCodeAdapter::new(PathBuf::from("/Users/x/Ash'; rm -rf ~; '/ash-event"));

        // When
        let line = hostile.invocation(AgentState::Waiting).unwrap_or_default();

        // Then — tout le chemin tient dans une seule chaîne entre apostrophes, dont la
        // seule apostrophe présente est celle, échappée, du nom du dossier.
        assert_eq!(
            line,
            r#"'/Users/x/Ash'\''; rm -rf ~; '\''/ash-event' waiting --tab "$ASH_TAB_ID""#
        );
    }

    #[test]
    fn given_the_instrumented_block_when_it_is_read_as_json_then_claude_code_finds_its_five_hooks()
    {
        // Given — le bloc est écrit dans le fichier de l'utilisateur sans que personne ne
        // le relise ensuite. S'il n'était pas du JSON valide, ou s'il rangeait les hooks
        // ailleurs que là où l'outil les cherche, Claude Code l'ignorerait sans un mot et
        // aucun état ne remonterait jamais.
        let adapter = adapter();
        let block = adapter
            .instrumentation(Path::new("/home/someone/.claude"))
            .map(|instrumentation| instrumentation.block)
            .unwrap_or_default();

        // When — les accolades que `hooks` remettra autour des entrées
        let parsed: serde_json::Value =
            serde_json::from_str(&format!("{{\n{block}\n}}")).unwrap_or(serde_json::Value::Null);

        // Then
        let hooks = &parsed["hooks"];
        for (hook, _) in HOOKS {
            assert!(
                hooks[hook][0]["hooks"][0]["type"] == "command",
                "le hook {hook} n'a pas la forme attendue : {parsed:#}"
            );
        }
        assert_eq!(hooks["PreToolUse"][0]["matcher"], "*");
        assert!(
            hooks["Stop"][0].get("matcher").is_none(),
            "seul PreToolUse porte un sélecteur d'outils"
        );
    }

    #[test]
    fn given_the_same_config_dir_when_the_block_is_composed_twice_then_it_is_byte_for_byte_the_same(
    ) {
        // Given — `hooks` compare le bloc trouvé à celui qu'il écrirait pour décider s'il
        // faut réécrire le fichier de l'utilisateur. Un horodatage, un nonce ou un ordre de
        // clés instable ferait réécrire un `settings.json` à chaque démarrage d'Ash.
        let adapter = adapter();

        // When
        let first = adapter.instrumentation(Path::new("/home/someone/.claude"));
        let second = adapter.instrumentation(Path::new("/home/someone/.claude"));

        // Then
        assert_eq!(first, second);
    }
}
