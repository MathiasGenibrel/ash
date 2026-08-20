use std::path::{Path, PathBuf};

use crate::features::agents::adapter::{
    hook_mark, Adapter, ChildEvent, HookEntry, Instrumentation, RawEvent, SubagentSupport,
};
use crate::features::agents::state::AgentState;
use crate::features::agents::usage::{SessionUsage, UsageSupport, DEFAULT_CONTEXT_WINDOW};

/// La version du bloc que cet adaptateur compose.
///
/// Elle s'incrémente **dès que la forme du bloc change** — un hook ajouté, une invocation
/// réécrite, un champ renommé. C'est elle que la feature `hooks` inscrit dans le marqueur
/// (`ash block v1`), et c'est elle qui lui permet de reconnaître un bloc écrit par un Ash
/// plus ancien et de le réécrire, sans avoir à comparer deux textes ni à se demander si
/// l'utilisateur y a touché.
///
/// **Elle vaut 2 depuis le sixième hook.** La v1 installait cinq entrées ; `SubagentStop`
/// en fait une sixième (spec §6.5), donc un `settings.json` instrumenté par un Ash antérieur
/// ne porte pas ce qu'Ash écrirait aujourd'hui. C'est exactement ce que la version existe
/// pour dire : la feature `hooks` classe alors le fichier en `Superseded`, montre le diff de
/// l'entrée manquante, et réécrit sans rien demander — le parcours d'ADR-0007, sans cas
/// particulier à ajouter.
///
/// La spec §10 écrit `ash block v3` : ce `v3` est une illustration de la *forme* du
/// marqueur, rédigée avant la moindre ligne de code. Le nombre est le compteur réel, pas
/// celui de la spec.
const BLOCK_VERSION: u32 = 2;

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

/// Le sixième hook, et le seul qui ne parle **pas** de l'onglet.
///
/// `SubagentStop` part quand une sous-tâche de l'outil `Task` se termine, et son entrée
/// standard porte l'`agent_id` de celle qui s'arrête — donc on sait *laquelle*, y compris
/// quand plusieurs tournent en parallèle (ADR-0007, amendement du 2026-08-13).
///
/// **Il écrit un verbe qui n'est pas un état**, et c'est toute la précaution : `subagent-stop`
/// ne figure dans aucune table d'états, donc [`Adapter::interpret`] le refuse comme il refuse
/// `Stop`, et il n'a aucun chemin vers l'état de l'onglet. Il ne se lit que par
/// [`Adapter::child_event`].
const CHILD_HOOK: (&str, &str) = ("SubagentStop", "subagent-stop");

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
    ///
    /// Elle finit par le **marqueur d'Ash**, en commentaire de shell : c'est à lui que la
    /// feature `hooks` reconnaît ses propres entrées au milieu de celles de l'utilisateur,
    /// et un `#` en fin de ligne ne change rien à ce que le shell exécute
    /// ([ADR-0007](../../../../../docs/adr/0007-etats-par-hooks.md), amendement du
    /// 2026-08-12).
    fn invocation(&self, word: &str) -> String {
        format!(
            "{} {} --tab \"$ASH_TAB_ID\" {}",
            shell_quoted(&self.event_binary),
            word,
            hook_mark(BLOCK_VERSION),
        )
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
        let mut entries = Vec::with_capacity(HOOKS.len() + 1);
        for (hook, state) in HOOKS {
            entries.push(HookEntry {
                // `settings.json` range les hooks de Claude Code sous `hooks`, une clé par
                // événement, et chaque clé porte un **tableau** de groupes. C'est ce chemin
                // que la feature `hooks` descend pour fusionner : elle n'a besoin de rien
                // d'autre, et surtout pas de connaître Claude Code.
                path: vec!["hooks".to_owned(), hook.to_owned()],
                item: self.item_for(hook, declared_word(state)?)?,
            });
        }

        // Le sixième, en dernier : il ne déclare pas un état, il nomme un enfant qui finit.
        let (child_hook, child_word) = CHILD_HOOK;
        entries.push(HookEntry {
            path: vec!["hooks".to_owned(), child_hook.to_owned()],
            item: self.item_for(child_hook, child_word)?,
        });

        Some(Instrumentation {
            file: config_dir.join("settings.json"),
            entries,
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

    /// Le verbe du sixième hook, et lui seul.
    ///
    /// Il ne passe **jamais** par [`Self::interpret`] : `subagent-stop` n'est pas un état, et
    /// un enfant qui finit ne rend pas `claude` disponible (ADR-0007, amendement du
    /// 2026-08-13). Les deux méthodes lisent le même mot brut et n'en tirent pas la même
    /// chose, ce qui est exactement le partage voulu.
    fn child_event(&self, raw: &RawEvent) -> Option<ChildEvent> {
        let (_, child_word) = CHILD_HOOK;
        (raw.kind() == child_word).then_some(ChildEvent::Ended)
    }

    /// Claude Code a des sous-tâches, et Ash les entend désormais.
    ///
    /// C'est cette tranche qui fait passer l'adaptateur de `None` à `Reported` : le bloc
    /// installe `SubagentStop`, donc la fin d'un enfant remonte, et le premier événement
    /// portant un `agent_id` inconnu révèle sa naissance. Déclarer `Reported` sans installer
    /// le hook aurait annoncé au cœur des lignes filles qui ne seraient jamais arrivées.
    fn subagents(&self) -> SubagentSupport {
        SubagentSupport::Reported
    }

    /// Claude Code tient un transcript, et c'est de là que vient la jauge.
    ///
    /// Rien n'est installé pour l'obtenir : le `stdin` de **chaque** hook porte déjà
    /// `transcript_path`, et le fichier existe que Ash le lise ou non. Déclarer la capacité
    /// ne coûte donc aucune écriture chez l'utilisateur, et n'incrémente pas
    /// [`BLOCK_VERSION`] — le bloc du `settings.json` est identique à celui d'avant.
    fn usage(&self) -> UsageSupport {
        UsageSupport::Transcript
    }

    /// Le dernier tour d'assistant de la queue, et ce qu'il dit de la place occupée.
    ///
    /// **La lecture part de la fin.** Le transcript est un journal : chaque tour y ajoute
    /// une ligne, et seule la dernière décrit l'état courant de la conversation. Remonter
    /// depuis la fin s'arrête donc au premier tour trouvé, au lieu de parcourir une
    /// conversation entière pour ne garder que son dernier élément.
    ///
    /// Les quatre compteurs s'**additionnent**, et c'est ce qui rend la mesure juste après un
    /// cache : Claude Code range les tokens déjà envoyés sous `cache_read_input_tokens` dès
    /// que le préfixe est mis en cache, si bien que `input_tokens` tombe à deux ou trois. Ne
    /// lire que `input_tokens` afficherait une conversation vide sur une session pleine.
    ///
    /// Une ligne qu'on ne sait pas lire est **sautée**, pas fatale : la queue commence au
    /// milieu du fichier, elle porte des `attachment` et des `user` qui n'ont pas d'usage, et
    /// un format qui gagne un champ ne doit pas éteindre la jauge.
    fn read_usage(&self, transcript_tail: &str) -> Option<SessionUsage> {
        let used = transcript_tail
            .lines()
            .rev()
            .find_map(|line| tokens_of(line.trim()))?;

        Some(SessionUsage {
            used_tokens: used,
            window_tokens: DEFAULT_CONTEXT_WINDOW,
        })
    }
}

/// Les tokens qu'une ligne de transcript déclare, si c'est un tour qui en déclare.
///
/// Séparée de [`Adapter::read_usage`] parce que c'est la seule moitié qui connaît la forme
/// du JSON de Claude Code — l'autre ne fait que choisir *quelle* ligne lire.
fn tokens_of(line: &str) -> Option<u64> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    let usage = entry.get("message")?.get("usage")?;

    // `as_u64().unwrap_or(0)` sur chacun, et pas un `?` : un compteur absent vaut zéro, et
    // exiger les quatre ferait perdre toute la mesure le jour où l'un d'eux disparaît.
    let counted = |field: &str| {
        usage
            .get(field)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    };

    let total = counted("input_tokens")
        + counted("cache_creation_input_tokens")
        + counted("cache_read_input_tokens")
        + counted("output_tokens");

    // Un objet `usage` présent mais entièrement vide ne mesure rien — le lire comme « zéro
    // token » ferait retomber la jauge à vide au milieu d'une conversation.
    (total > 0).then_some(total)
}

impl ClaudeCodeAdapter {
    /// Un groupe de la table `hooks` de Claude Code, dans sa forme attendue.
    ///
    /// **Sur une seule ligne, et compact.** C'est ce que la feature `hooks` insère dans le
    /// tableau de l'utilisateur, à côté des siens : une ligne se retire exactement comme
    /// elle a été posée, et ne réindente rien autour d'elle.
    fn item_for(&self, hook: &str, word: &str) -> Option<String> {
        let command = serde_json::json!({
            "type": "command",
            "command": self.invocation(word),
        });

        let mut group = serde_json::Map::new();
        let (filtered_hook, matcher) = MATCH_EVERY_TOOL;
        if hook == filtered_hook {
            group.insert("matcher".to_owned(), matcher.into());
        }
        group.insert("hooks".to_owned(), serde_json::Value::Array(vec![command]));

        serde_json::to_string(&serde_json::Value::Object(group)).ok()
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

    /// Tout ce que l'adaptateur fera écrire, en un seul texte — la façon la plus directe
    /// de vérifier qu'un mot y figure.
    fn written(adapter: &ClaudeCodeAdapter) -> String {
        adapter
            .instrumentation(Path::new("/home/someone/.claude"))
            .map(|instrumentation| {
                instrumentation
                    .entries
                    .iter()
                    .map(|entry| entry.item.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    /// Une queue de transcript telle que Claude Code l'écrit, réduite à trois lignes.
    ///
    /// Les valeurs sont celles d'un vrai fichier : un `input_tokens` à 2 parce que le
    /// préfixe est en cache, et l'essentiel de la conversation sous
    /// `cache_read_input_tokens`. C'est ce couple qui rend le test utile — une
    /// implémentation qui ne lirait que `input_tokens` passerait avec un chiffre absurde.
    const OWN_TRANSCRIPT: &str = concat!(
        r#"{"type":"user","message":{"role":"user","content":"vas-y"}}"#,
        "\n",
        r#"{"type":"attachment","attachment":{"type":"file"}}"#,
        "\n",
        r#"{"type":"assistant","message":{"model":"claude-opus-5","usage":{"input_tokens":2,"cache_creation_input_tokens":2196,"cache_read_input_tokens":143801,"output_tokens":274}}}"#,
        "\n",
    );

    /// Les événements que ces entrées feront réellement remonter — la forme canonique de la
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
        let report = check_adapter_contract(&adapter, &own_events(), Some(OWN_TRANSCRIPT));

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
        let written = written(&adapter);

        // When — les mots que la table peut écrire, relus tels qu'`ash-event` les postera
        let round_trip: Vec<Option<AgentState>> = HOOKS
            .iter()
            .map(|(_, state)| {
                let word = declared_word(*state).unwrap_or("");
                assert!(
                    written.contains(&format!(r#"ash-event' {word} --tab \"$ASH_TAB_ID\""#)),
                    "les entrées n'écrivent pas « {word} » :\n{written}"
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
        let line = hostile.invocation("waiting");

        // Then — tout le chemin tient dans une seule chaîne entre apostrophes, dont la
        // seule apostrophe présente est celle, échappée, du nom du dossier.
        assert_eq!(
            line,
            r#"'/Users/x/Ash'\''; rm -rf ~; '\''/ash-event' waiting --tab "$ASH_TAB_ID" # ash:hook v2"#
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
        let instrumentation = adapter
            .instrumentation(Path::new("/home/someone/.claude"))
            .unwrap_or_else(|| panic!("claude-code instrumente toujours"));

        // When — chaque entrée, relue là où son chemin dit qu'elle ira
        let mut hooks = serde_json::Map::new();
        for entry in &instrumentation.entries {
            let item: serde_json::Value = serde_json::from_str(&entry.item).unwrap_or_else(|why| {
                panic!("l'entrée n'est pas du JSON ({why}) : {}", entry.item)
            });
            assert_eq!(
                entry.path.first().map(String::as_str),
                Some("hooks"),
                "les hooks de Claude Code vivent sous `hooks`"
            );
            let event = entry.path.get(1).cloned().unwrap_or_default();
            hooks.insert(event, serde_json::Value::Array(vec![item]));
        }

        // Then
        for (hook, _) in HOOKS {
            assert!(
                hooks[hook][0]["hooks"][0]["type"] == "command",
                "le hook {hook} n'a pas la forme attendue : {hooks:#?}"
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

    #[test]
    fn given_a_transcript_whose_prefix_is_cached_when_the_adapter_reads_it_then_it_counts_the_cache_too(
    ) {
        // Given — le cas normal après le premier tour : `input_tokens` tombe à deux ou
        // trois, et la conversation entière vit sous `cache_read_input_tokens`.
        let adapter = adapter();

        // When
        let usage = adapter.read_usage(OWN_TRANSCRIPT).unwrap();

        // Then — 2 + 2196 + 143801 + 274. Ne lire qu'`input_tokens` afficherait une
        // conversation vide sur une session pleine aux trois quarts.
        assert_eq!(usage.used_tokens, 146_273);
        assert_eq!(usage.window_tokens, DEFAULT_CONTEXT_WINDOW);
    }

    #[test]
    fn given_two_assistant_turns_when_the_adapter_reads_them_then_the_last_one_wins() {
        // Given — le transcript est un journal : le tour précédent est toujours là, et il
        // décrit une conversation plus petite qu'elle ne l'est.
        let adapter = adapter();
        let earlier = r#"{"type":"assistant","message":{"usage":{"input_tokens":10}}}"#;
        let later = r#"{"type":"assistant","message":{"usage":{"input_tokens":900}}}"#;

        // When
        let usage = adapter
            .read_usage(&format!("{earlier}\n{later}\n"))
            .unwrap();

        // Then
        assert_eq!(usage.used_tokens, 900);
    }

    #[test]
    fn given_a_tail_without_a_single_assistant_turn_when_the_adapter_reads_it_then_it_measures_nothing(
    ) {
        // Given — une queue qui n'attrape que des messages d'utilisateur, ce qui arrive
        // quand le dernier tour a produit de gros résultats d'outil.
        let adapter = adapter();
        let tail = r#"{"type":"user","message":{"role":"user","content":"encore"}}"#;

        // When
        let usage = adapter.read_usage(tail);

        // Then — une absence de mesure, pas un zéro : l'onglet gardera ce qu'il savait.
        assert_eq!(usage, None);
    }

    #[test]
    fn given_a_usage_object_with_every_counter_at_zero_when_the_adapter_reads_it_then_it_measures_nothing(
    ) {
        // Given — un tour qui déclare `usage` sans rien dedans. Le lire comme « zéro token »
        // ferait retomber la jauge à vide au milieu d'une conversation.
        let adapter = adapter();
        let tail = r#"{"type":"assistant","message":{"usage":{"input_tokens":0}}}"#;

        // When
        let usage = adapter.read_usage(tail);

        // Then
        assert_eq!(usage, None);
    }
}
