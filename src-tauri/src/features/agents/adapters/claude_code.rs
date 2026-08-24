use std::path::{Path, PathBuf};

use crate::features::agents::adapter::{
    hook_mark, Adapter, ChildEvent, HookEntry, Instrumentation, RawEvent, SessionEvent,
    SubagentSupport,
};
use crate::features::agents::state::AgentState;
use crate::features::agents::usage::{ModelSource, Turn, UsageSupport};

/// La version du bloc que cet adaptateur compose.
///
/// Elle s'incrémente **dès que la forme du bloc change** — un hook ajouté, une invocation
/// réécrite, un champ renommé. C'est elle que la feature `hooks` inscrit dans le marqueur
/// (`ash block v1`), et c'est elle qui lui permet de reconnaître un bloc écrit par un Ash
/// plus ancien et de le réécrire, sans avoir à comparer deux textes ni à se demander si
/// l'utilisateur y a touché.
///
/// **Elle vaut 3 depuis le septième hook.** La v1 installait cinq entrées ; `SubagentStop`
/// en a fait une sixième (spec §6.5), et `SessionStart` une septième — donc un
/// `settings.json` instrumenté par un Ash antérieur ne porte pas ce qu'Ash écrirait
/// aujourd'hui. C'est exactement ce que la version existe pour dire : la feature `hooks`
/// classe alors le fichier en `Superseded`, montre le diff de l'entrée manquante, et réécrit
/// sans rien demander — le parcours d'ADR-0007, sans cas particulier à ajouter.
///
/// La spec §10 écrit `ash block v3` : ce `v3` est une illustration de la *forme* du
/// marqueur, rédigée avant la moindre ligne de code. Que le compteur réel vaille aujourd'hui
/// le même nombre est une coïncidence — c'est le compteur qui fait foi, et lui seul.
const BLOCK_VERSION: u32 = 3;

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

/// Le septième hook, et le second qui n'annonce **aucun état**.
///
/// `SessionStart` part quand une session s'ouvre — au démarrage de `claude`, à sa reprise
/// (`--continue`, `--resume`), après un `/clear` ou un compactage. Il dit qu'une session
/// **existe** dans cet onglet, et rien de ce qu'elle fait : un agent qui vient d'ouvrir
/// n'a reçu aucun prompt, donc il ne travaille pas.
///
/// Il vaut pourtant les six autres, et pour deux raisons qui tiennent ensemble
/// (ADR-0007, précision du 2026-08-24) :
///
/// - il fait naître la machine à états de l'onglet, donc la **présence** vue par la sonde
///   cesse d'y répondre. Sans lui, `claude` à son invite se montre `working` avec un glyphe
///   qui tourne, alors qu'il attend un prompt ;
/// - son entrée standard porte un `transcript_path`, comme celle des six autres. C'est ce
///   qui donne sa jauge de contexte à une session **reprise** dès la première seconde, sans
///   attendre qu'un prompt soit envoyé.
///
/// **Il écrit un verbe qui n'est pas un état**, comme [`CHILD_HOOK`] : `session-start` ne
/// figure dans aucune table d'états, donc [`Adapter::interpret`] le refuse, et il n'a aucun
/// chemin vers l'état de l'onglet. Il ne se lit que par [`Adapter::session_event`].
const SESSION_HOOK: (&str, &str) = ("SessionStart", "session-start");

/// La variable d'environnement qui l'emporte sur toute la configuration de Claude Code.
const MODEL_VARIABLE: &str = "ANTHROPIC_MODEL";

/// La clé sous laquelle un `settings.json` de Claude Code nomme le modèle.
const MODEL_KEY: &str = "model";

/// Les deux fichiers du **dépôt**, du plus privé au plus partagé.
///
/// `settings.local.json` n'est pas versionné : c'est là que se posent les choix propres à une
/// machine, et il l'emporte donc sur le `settings.json` que l'équipe partage. L'ordre de ce
/// tableau *est* cette priorité.
const PROJECT_SETTINGS: [&str; 2] = [".claude/settings.local.json", ".claude/settings.json"];

/// Le fichier du **foyer**, consulté en dernier.
const HOME_SETTINGS: &str = ".claude/settings.json";

/// Le suffixe qui fait passer une session à un million de tokens.
///
/// Il se porte sur un alias court (`opus[1m]`) comme sur un identifiant complet
/// (`claude-opus-5[1m]`), et c'est **la seule chose** qui distingue les deux fenêtres :
/// le transcript, lui, écrit `claude-opus-5` dans les deux cas.
const LONG_CONTEXT_SUFFIX: &str = "[1m]";

/// La fenêtre d'une session portant [`LONG_CONTEXT_SUFFIX`].
const LONG_CONTEXT_WINDOW: u64 = 1_000_000;

/// Ce que le nom court ajoute quand la session tourne en un million de tokens.
///
/// `Opus 5 1M` plutôt que `Opus 5[1m]` : le suffixe est une syntaxe de fichier de réglages,
/// pas un mot à lire dans une barre d'état.
const LONG_CONTEXT_MARK: &str = "1M";

/// La fenêtre d'un modèle reconnu qui ne porte pas ce suffixe.
///
/// **C'est la valeur d'un modèle nommé, et plus un défaut universel.** C'est toute la
/// différence avec le `DEFAULT_CONTEXT_WINDOW` qu'elle remplace : elle ne s'applique qu'à un
/// identifiant qu'on a reconnu, et un identifiant inconnu n'y retombe pas.
const STANDARD_CONTEXT_WINDOW: u64 = 200_000;

/// Le nombre de chiffres au-delà duquel un segment de l'identifiant est une **date**.
///
/// `claude-haiku-4-5-20251001` porte sa version *et* son millésime, séparés de la même façon.
/// La version est faite de nombres courts (`4`, `5`, `8`) ; huit chiffres d'affilée ne sont
/// pas un numéro de version, et les écrire dans la barre donnerait `Haiku 4.5.20251001`.
const DATE_DIGITS: usize = 4;

/// Les familles de modèles dont Claude Code connaît la fenêtre.
///
/// Cherchées **dans** l'identifiant, jamais comparées à lui : les identifiants réels sont
/// datés (`claude-sonnet-4-5-20250929`), et une table d'égalités serait périmée à la
/// prochaine version — c'est le raisonnement de l'amendement du 2026-08-18 à ADR-0006, où le
/// nom du binaire de Claude Code est celui de sa version.
const KNOWN_FAMILIES: [&str; 3] = ["opus", "sonnet", "haiku"];

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
        let mut entries = Vec::with_capacity(HOOKS.len() + 2);
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

        // Le sixième : il ne déclare pas un état, il nomme un enfant qui finit.
        let (child_hook, child_word) = CHILD_HOOK;
        entries.push(HookEntry {
            path: vec!["hooks".to_owned(), child_hook.to_owned()],
            item: self.item_for(child_hook, child_word)?,
        });

        // Le septième, en dernier : il ne déclare pas un état non plus, il dit qu'une
        // session existe — et il porte son transcript, donc sa mesure.
        let (session_hook, session_word) = SESSION_HOOK;
        entries.push(HookEntry {
            path: vec!["hooks".to_owned(), session_hook.to_owned()],
            item: self.item_for(session_hook, session_word)?,
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

    /// Le verbe du septième hook, et lui seul.
    ///
    /// Il ne passe **jamais** par [`Self::interpret`] : une session qui s'ouvre n'est pas un
    /// état, et un agent qui vient de démarrer n'a rien en vol. C'est le même partage que
    /// pour [`Self::child_event`] — trois méthodes lisent le même mot brut, et aucune ne
    /// répond quand une autre a répondu.
    fn session_event(&self, raw: &RawEvent) -> Option<SessionEvent> {
        let (_, session_word) = SESSION_HOOK;
        (raw.kind() == session_word).then_some(SessionEvent::Opened)
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
    /// Les **trois compteurs d'entrée** s'additionnent, et c'est ce qui rend la mesure juste
    /// après un cache : Claude Code range les tokens déjà envoyés sous
    /// `cache_read_input_tokens` dès que le préfixe est mis en cache, si bien que
    /// `input_tokens` tombe à deux ou trois. Ne lire que `input_tokens` afficherait une
    /// conversation vide sur une session pleine.
    ///
    /// `output_tokens`, lui, est **dehors** : c'est la réponse à la requête qu'on mesure, pas
    /// ce que la requête occupait — voir [`turn_of`].
    ///
    /// Une ligne qu'on ne sait pas lire est **sautée**, pas fatale : la queue commence au
    /// milieu du fichier, elle porte des `attachment` et des `user` qui n'ont pas d'usage, et
    /// un format qui gagne un champ ne doit pas éteindre la jauge.
    ///
    /// **Le modèle sort du même objet que les tokens**, et c'est ce qui rend la lecture
    /// gratuite : `"model":"claude-opus-5"` est écrit à côté de l'`usage` du tour, donc le
    /// nommer ne coûte pas une ligne de plus, encore moins un fichier de plus. C'est aussi ce
    /// qui garantit qu'on ne rapporte jamais le modèle d'un tour avec la mesure d'un autre.
    fn read_turn(&self, transcript_tail: &str) -> Option<Turn> {
        transcript_tail
            .lines()
            .rev()
            .find_map(|line| turn_of(line.trim()))
    }

    /// L'identifiant du transcript, ramené aux deux ou trois mots de la barre.
    ///
    /// Le nom vient du **transcript** et le suffixe de la **configuration**, parce que c'est
    /// ainsi qu'ils sont écrits : `/model sonnet` fait changer le premier au tour suivant sans
    /// toucher au second, et `[1m]` ne figure que dans le second. Le suffixe ne se recopie
    /// toutefois pas de la configuration : il se **relit de la fenêtre**, par la porte qui
    /// vient déjà d'arbitrer l'accord des deux sources ([`Adapter::context_window`]). Un nom
    /// en `1M` à côté d'une jauge sans fenêtre serait la même contradiction écrite deux fois,
    /// et la seule façon qu'elle ne s'écrive jamais est qu'il n'y ait **qu'une** condition —
    /// pas deux qui se ressemblent.
    ///
    /// Un identifiant dont aucune famille connue ne ressort ne se nomme pas — pas plus qu'il
    /// ne se mesure ([`Self::context_window`]). C'est la même porte, et il n'y en a pas
    /// d'autre : `claude-fable-5` est un identifiant réel qu'Ash ne connaît pas, et le segment
    /// disparaît plutôt que d'inventer un mot.
    fn model_name(&self, ran: &str, configured: Option<&str>) -> Option<String> {
        let short = short_name(ran)?;

        // Le `1M` du nom **est** la fenêtre du million, dite en toutes lettres : il se lit
        // donc là où elle se décide, et nulle part ailleurs. Écrire `Sonnet 5 1M` parce qu'un
        // `opus[1m]` traîne dans un fichier de réglages annoncerait une fenêtre qu'on vient
        // justement de refuser de calculer — et c'est ce que l'accord des deux sources refuse
        // déjà, une seule fois, pour les deux tables.
        let long_context = self.context_window(Some(ran), configured) == Some(LONG_CONTEXT_WINDOW);

        Some(if long_context {
            format!("{short} {LONG_CONTEXT_MARK}")
        } else {
            short
        })
    }

    /// Les quatre endroits où Claude Code nomme son modèle, dans **son** ordre de priorité.
    ///
    /// C'est celui de l'outil, pas une préférence d'Ash : la variable d'environnement
    /// l'emporte sur tout, les réglages locaux du dépôt sur les réglages partagés du dépôt, et
    /// le dépôt sur le foyer. Un utilisateur qui a posé `"model": "sonnet"` dans le
    /// `.claude/settings.json` d'un projet a dit quelque chose de plus précis que son
    /// `~/.claude/settings.json`, et la jauge doit le suivre.
    ///
    /// Le `cwd` vient de la trame du hook, où il voyage comme **donnée** et jamais comme
    /// corrélation (`wire.rs`). Sans lui, les deux couches du dépôt n'ont pas de chemin, et
    /// seules la variable et le foyer restent — ce qui est une dégradation honnête, pas une
    /// supposition.
    fn model_sources(&self, cwd: Option<&Path>, home: Option<&Path>) -> Vec<ModelSource> {
        let mut sources = vec![ModelSource::variable(MODEL_VARIABLE)];

        if let Some(cwd) = cwd {
            for file in PROJECT_SETTINGS {
                sources.push(ModelSource::json_key(cwd.join(file), MODEL_KEY));
            }
        }
        if let Some(home) = home {
            sources.push(ModelSource::json_key(home.join(HOME_SETTINGS), MODEL_KEY));
        }

        sources
    }

    /// Ce que ces deux identifiants disent de la taille de la fenêtre — **et rien quand ils
    /// se contredisent**.
    ///
    /// Trois questions, dans cet ordre : la famille, l'accord, puis le suffixe.
    ///
    /// - La famille est cherchée **dans** l'identifiant plutôt que comparée à lui, pour la
    ///   raison qui a déjà tranché ADR-0006 : les identifiants réels sont datés
    ///   (`claude-sonnet-4-5-20250929`), et une table d'égalités serait périmée à la
    ///   prochaine version. Une famille inconnue des deux côtés vaut `None`.
    /// - **L'accord** est ce que cette porte est venue ajouter : le numérateur vient du
    ///   transcript et le dénominateur de la configuration, donc de deux sources qui peuvent
    ///   parler de deux modèles. Un `~/.claude/settings.json` qui annonce `opus[1m]` pendant
    ///   qu'un `claude-sonnet-5` tourne ne décrit pas la session qu'on mesure, et le
    ///   pourcentage serait alors calculé sur une fenêtre qui n'est pas la sienne — faux d'un
    ///   facteur cinq dans le cas observé. En désaccord, la fenêtre disparaît, et la mesure
    ///   reste ([`super::super::usage::SessionUsage::window_tokens`]).
    /// - Le suffixe `[1m]` est ce qui distingue une session d'un million de tokens, et il se
    ///   porte aussi bien sur un alias court (`opus[1m]`) que sur un identifiant complet
    ///   (`claude-opus-5[1m]`). Les deux formes existent réellement dans les fichiers des
    ///   utilisateurs, donc la reconnaissance porte sur le **suffixe**, jamais sur la liste
    ///   des identifiants qui pourraient le porter. Il ne vit que dans la configuration : le
    ///   transcript écrit `claude-opus-5` qu'on tourne en 200 k ou en 1 M.
    ///
    /// C'est aussi d'ici que sort le `1M` du nom court : [`Self::model_name`] ne relit pas la
    /// configuration, il regarde **la fenêtre que cette porte a rendue**. Le nom et le
    /// pourcentage ne peuvent donc pas se contredire, non parce qu'on y veille à deux endroits
    /// mais parce qu'il n'y a qu'un endroit.
    ///
    /// Un transcript qui ne nomme **rien** ne contredit rien : la configuration répond seule,
    /// comme avant cette porte. Tout le reste — `default`, un alias interne, un identifiant
    /// d'un autre fournisseur, une faute de frappe — vaut `None`. C'est la règle qui remplace
    /// `DEFAULT_CONTEXT_WINDOW`, et elle est le cœur de la correction : **rien de reconnu ne
    /// vaut rien**.
    fn context_window(&self, ran: Option<&str>, configured: Option<&str>) -> Option<u64> {
        let configured = identifier(configured?);
        family_of(&configured.named)?;

        if let Some(ran) = ran {
            let ran = identifier(ran);
            family_of(&ran.named)?;
            if !names_the_same_model(&ran.named, &configured.named) {
                return None;
            }
        }

        Some(if configured.long_context {
            LONG_CONTEXT_WINDOW
        } else {
            STANDARD_CONTEXT_WINDOW
        })
    }
}

/// Ce qu'un identifiant de modèle porte, lu **une seule fois**.
///
/// Le suffixe `[1m]` tranche deux questions — la taille de la fenêtre et le `1M` du nom court
/// — et il n'est lu qu'ici : le retirer et constater sa présence sont la même lecture, et deux
/// lectures séparées finiraient par diverger. La barre dirait alors `Opus 5` sur un
/// pourcentage calculé en 1 M.
struct Identifier {
    /// L'identifiant sans ce qui n'appartient pas à la famille : espaces, casse, suffixe.
    named: String,
    /// Le suffixe était là, donc la session tourne dans la fenêtre du million.
    long_context: bool,
}

/// L'identifiant tel qu'on le lit — voir [`Identifier`].
fn identifier(model: &str) -> Identifier {
    let model = model.trim().to_ascii_lowercase();
    match model.strip_suffix(LONG_CONTEXT_SUFFIX) {
        Some(named) => Identifier {
            named: named.to_owned(),
            long_context: true,
        },
        None => Identifier {
            named: model,
            long_context: false,
        },
    }
}

/// La famille connue que cet identifiant contient, et **ce qui la suit**.
///
/// **La porte des deux tables**, celle de la fenêtre ([`Adapter::context_window`]) et celle du
/// nom ([`short_name`]) : ce que Claude Code ne sait pas mesurer, il ne sait pas non plus le
/// nommer, et une seconde recherche sur la même liste finirait par répondre autrement — une
/// barre qui écrirait `Opus 5` à côté d'un pourcentage qu'elle refuse de calculer.
///
/// Ce qui suit la famille est rendu **découpé**, et non sous forme d'indice : l'appelant n'a
/// alors rien à recalculer, là où trois `at + famille.len()` recopiés à la main sont trois
/// occasions de se tromper d'une longueur — et de lire une version qui commencerait au milieu
/// d'un mot.
///
/// Cherchée **dans** l'identifiant, jamais comparée à lui — voir [`KNOWN_FAMILIES`].
fn family_of(named: &str) -> Option<(&'static str, &str)> {
    KNOWN_FAMILIES.iter().find_map(|known| {
        named
            .find(known)
            .map(|at| (*known, &named[at + known.len()..]))
    })
}

/// L'identifiant ramené à sa famille et à sa version — `claude-opus-5` → `Opus 5`.
///
/// **La famille est la porte, la version n'est qu'une transcription.** C'est ce qui distingue
/// ce nom d'une devinette : rien ne sort d'ici sans qu'une famille connue ait été reconnue,
/// exactement comme rien ne sort de [`Adapter::context_window`] sans elle. Les chiffres qui
/// suivent sont recopiés, pas interprétés — ce sont ceux que l'identifiant porte.
///
/// La version s'arrête au premier segment qui n'est pas un nombre court : `4`, puis `5`, puis
/// le millésime `20251001` qu'aucune barre d'état n'a à montrer (voir [`DATE_DIGITS`]). Un
/// identifiant sans version — l'alias `opus`, que Claude Code accepte — rend la famille seule.
fn short_name(model: &str) -> Option<String> {
    let model = identifier(model).named;
    let (family, after) = family_of(&model)?;

    let version = version_after(after);
    let family = capitalized(family);
    Some(if version.is_empty() {
        family
    } else {
        format!("{family} {version}")
    })
}

/// Les deux identifiants nomment-ils le **même** modèle ?
///
/// La question que la fenêtre pose avant de se laisser calculer, et elle appartient à
/// l'adaptateur : c'est Claude Code, et lui seul, qui sait qu'`opus[1m]` et `claude-opus-5`
/// sont deux écritures d'une même chose, et que `sonnet` et `claude-opus-5` n'en sont pas.
///
/// Deux comparaisons, et **rien au-delà de ce qui est écrit** :
///
/// - les familles doivent être la même — un `sonnet` configuré ne décrit pas un opus qui
///   tourne, et c'est le désaccord le plus courant (`/model` changé en cours de session) ;
/// - les versions doivent s'accorder **quand les deux en portent une** : `claude-opus-5[1m]`
///   configuré contre un `claude-opus-4-7` qui tourne est un désaccord, et il est réel — les
///   sessions de revue de sécurité tournent sur un modèle que la configuration n'annonce pas.
///
/// Un alias de configuration sans version (`opus[1m]`) ne peut **pas** être confronté à la
/// version qui a tourné : Claude Code le résout vers le dernier modèle de la famille, et Ash
/// ne sait pas lequel c'est. Il accorde alors sur la famille seule, et c'est une limite
/// documentée — la même nature que le `/model` tapé en cours de session.
fn names_the_same_model(ran: &str, configured: &str) -> bool {
    let (Some((ran_family, after_ran)), Some((configured_family, after_configured))) =
        (family_of(ran), family_of(configured))
    else {
        return false;
    };

    if ran_family != configured_family {
        return false;
    }

    let configured_version = version_after(after_configured);
    configured_version.is_empty() || configured_version == version_after(after_ran)
}

/// La version qu'un identifiant écrit après sa famille — le `-4-7` de `claude-opus-4-7` → `4.7`.
///
/// Elle s'arrête au premier segment qui n'est pas un nombre court : `4`, puis `5`, puis le
/// millésime `20251001` qu'aucune barre d'état n'a à montrer (voir [`DATE_DIGITS`]). Vide
/// quand l'identifiant n'en porte pas — l'alias `opus`, que Claude Code accepte.
///
/// **Une seule lecture pour les deux questions** qui s'en servent, le nom court et l'accord
/// des deux sources : deux extractions divergeraient, et la barre finirait par nommer un
/// modèle dont elle a refusé la fenêtre.
fn version_after(after_family: &str) -> String {
    after_family
        .split('-')
        .filter(|part| !part.is_empty())
        .take_while(|part| is_version_part(part))
        .collect::<Vec<_>>()
        .join(".")
}

/// Un nombre assez court pour être un numéro de version, et non un millésime.
fn is_version_part(part: &str) -> bool {
    part.len() < DATE_DIGITS && part.chars().all(|digit| digit.is_ascii_digit())
}

/// `opus` → `Opus`. Les familles sont ascii et minuscules par construction.
fn capitalized(word: &str) -> String {
    let mut letters = word.chars();
    match letters.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + letters.as_str(),
        None => String::new(),
    }
}

/// Ce qu'une ligne de transcript déclare, si c'est un tour qui déclare quelque chose.
///
/// Séparée de [`Adapter::read_turn`] parce que c'est la seule moitié qui connaît la forme
/// du JSON de Claude Code — l'autre ne fait que choisir *quelle* ligne lire.
fn turn_of(line: &str) -> Option<Turn> {
    let entry: serde_json::Value = serde_json::from_str(line).ok()?;
    let message = entry.get("message")?;
    let usage = message.get("usage")?;

    // `as_u64().unwrap_or(0)` sur chacun, et pas un `?` : un compteur absent vaut zéro, et
    // exiger les quatre ferait perdre toute la mesure le jour où l'un d'eux disparaît.
    let counted = |field: &str| {
        usage
            .get(field)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    };

    // **Les trois compteurs d'entrée, et pas le quatrième.** Ce que `/context` affiche est la
    // taille du *prompt* de la dernière requête — ce que la conversation occupe au moment où
    // le modèle répond. `output_tokens` est la réponse à cette requête : elle n'entrera dans
    // la fenêtre qu'à la requête suivante, où elle sera comptée par les trois autres. L'y
    // ajouter ici avance donc d'un tour sur `/context`, de 69 à 4 899 tokens sur les
    // transcripts réels — jusqu'à 2,4 points de pourcentage sur une fenêtre de 200 k.
    let total = counted("input_tokens")
        + counted("cache_creation_input_tokens")
        + counted("cache_read_input_tokens");

    // Un objet `usage` présent mais entièrement vide ne mesure rien — le lire comme « zéro
    // token » ferait retomber la jauge à vide au milieu d'une conversation. C'est l'`usage`
    // qui fait le tour, et non le modèle : une ligne qui nomme un modèle sans rien mesurer
    // n'est pas un tour d'assistant.
    (total > 0).then(|| Turn {
        used_tokens: total,
        // `message.model`, et non une clé de premier niveau : c'est l'objet du message qui
        // porte le modèle, à côté de son `usage`. Une chaîne vide ne nomme rien.
        model: message
            .get("model")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_owned),
    })
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
    fn given_the_seventh_hook_when_its_verb_comes_back_from_the_socket_then_it_opens_a_session_and_declares_nothing(
    ) {
        // Given — le bloc installe `SessionStart`, dont la commande écrit `session-start`.
        // Ce verbe doit faire exactement une chose : dire qu'une session existe. Le traduire
        // aussi en état remettrait un agent qui attend un prompt en `working`, ce que la
        // précision du 2026-08-24 à ADR-0007 écarte.
        let adapter = adapter();
        let written = written(&adapter);
        let raw = RawEvent::new("session-start");

        // When
        let opened = adapter.session_event(&raw);
        let declared = adapter.interpret(&raw);
        let child = adapter.child_event(&raw);

        // Then — et le verbe part bien dans le fichier de l'utilisateur : sans l'entrée,
        // l'adaptateur saurait relire un mot que rien n'enverrait jamais.
        assert!(
            written.contains(r#"ash-event' session-start --tab \"$ASH_TAB_ID\""#),
            "les entrées n'écrivent pas « session-start » :\n{written}"
        );
        assert_eq!(opened, Some(SessionEvent::Opened));
        assert_eq!(declared, None);
        assert_eq!(child, None);
    }

    #[test]
    fn given_the_instrumented_block_when_it_is_read_as_json_then_session_start_is_one_of_its_events(
    ) {
        // Given — c'est Claude Code qui lit ce fichier, et il ne déclenche que les
        // événements qu'il y trouve. Une entrée rangée sous un autre nom serait ignorée sans
        // un mot, et l'onglet d'un agent qui vient d'ouvrir resterait `working`.
        let adapter = adapter();

        // When
        let events: Vec<String> = adapter
            .instrumentation(Path::new("/home/someone/.claude"))
            .map(|instrumentation| {
                instrumentation
                    .entries
                    .iter()
                    .filter_map(|entry| entry.path.get(1).cloned())
                    .collect()
            })
            .unwrap_or_default();

        // Then — les sept, dans l'ordre où l'adaptateur les pose
        assert_eq!(
            events,
            vec![
                "UserPromptSubmit",
                "PreToolUse",
                "Notification",
                "Stop",
                "SessionEnd",
                "SubagentStop",
                "SessionStart",
            ]
        );
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
            r#"'/Users/x/Ash'\''; rm -rf ~; '\''/ash-event' waiting --tab "$ASH_TAB_ID" # ash:hook v3"#
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
        let used = adapter
            .read_turn(OWN_TRANSCRIPT)
            .map(|turn| turn.used_tokens);

        // Then — 2 + 2196 + 143801, et **pas** les 274 d'`output_tokens` : ce que `/context`
        // affiche est la taille du prompt de la requête, pas celle de la réponse. Ne lire
        // qu'`input_tokens` afficherait, à l'inverse, une conversation vide sur une session
        // pleine aux trois quarts.
        assert_eq!(used, Some(145_999));
    }

    #[test]
    fn given_two_assistant_turns_when_the_adapter_reads_them_then_the_last_one_wins() {
        // Given — le transcript est un journal : le tour précédent est toujours là, et il
        // décrit une conversation plus petite qu'elle ne l'est.
        let adapter = adapter();
        let earlier = r#"{"type":"assistant","message":{"usage":{"input_tokens":10}}}"#;
        let later = r#"{"type":"assistant","message":{"usage":{"input_tokens":900}}}"#;

        // When
        let used = adapter
            .read_turn(&format!("{earlier}\n{later}\n"))
            .map(|turn| turn.used_tokens);

        // Then
        assert_eq!(used, Some(900));
    }

    #[test]
    fn given_a_tail_without_a_single_assistant_turn_when_the_adapter_reads_it_then_it_measures_nothing(
    ) {
        // Given — une queue qui n'attrape que des messages d'utilisateur, ce qui arrive
        // quand le dernier tour a produit de gros résultats d'outil.
        let adapter = adapter();
        let tail = r#"{"type":"user","message":{"role":"user","content":"encore"}}"#;

        // When
        let read = adapter.read_turn(tail);

        // Then — une absence de mesure, pas un zéro : l'onglet gardera ce qu'il savait.
        assert_eq!(read, None);
    }

    #[test]
    fn given_a_usage_object_with_every_counter_at_zero_when_the_adapter_reads_it_then_it_measures_nothing(
    ) {
        // Given — un tour qui déclare `usage` sans rien dedans. Le lire comme « zéro token »
        // ferait retomber la jauge à vide au milieu d'une conversation.
        let adapter = adapter();
        let tail = r#"{"type":"assistant","message":{"usage":{"input_tokens":0}}}"#;

        // When
        let read = adapter.read_turn(tail);

        // Then
        assert_eq!(read, None);
    }

    #[test]
    fn given_a_model_carrying_the_long_context_suffix_when_its_window_is_asked_then_it_is_a_million(
    ) {
        // Given — les deux formes qui existent réellement dans les fichiers des utilisateurs :
        // l'alias court, et l'identifiant complet. Le transcript, lui, écrit `claude-opus-5`
        // dans les deux cas — c'est bien la configuration, et elle seule, qui distingue. Ici
        // il ne nomme rien du tout : une ligne d'usage sans `model`, qui ne contredit rien.
        let adapter = adapter();
        let declared = ["opus[1m]", "claude-opus-5[1m]", "sonnet[1m]", "OPUS[1M]"];

        // When
        let windows: Vec<Option<u64>> = declared
            .iter()
            .map(|model| adapter.context_window(None, Some(model)))
            .collect();

        // Then
        assert_eq!(windows, vec![Some(1_000_000); 4]);
    }

    #[test]
    fn given_a_recognized_model_without_the_suffix_when_its_window_is_asked_then_it_is_two_hundred_thousand(
    ) {
        // Given — les identifiants réels sont **datés**, donc la famille est cherchée dans
        // l'identifiant et non comparée à lui : une table d'égalités serait périmée à la
        // prochaine version, exactement comme la table de noms de binaires d'ADR-0006.
        let adapter = adapter();
        let declared = ["opus", "sonnet", "haiku", "claude-sonnet-4-5-20250929"];

        // When
        let windows: Vec<Option<u64>> = declared
            .iter()
            .map(|model| adapter.context_window(None, Some(model)))
            .collect();

        // Then
        assert_eq!(windows, vec![Some(200_000); 4]);
    }

    #[test]
    fn given_a_configuration_alias_and_the_full_identifier_it_resolved_to_when_the_window_is_asked_then_they_agree(
    ) {
        // Given — le cas de tous les jours : `~/.claude/settings.json` porte `opus[1m]`, et le
        // transcript écrit l'identifiant complet vers lequel l'alias s'est résolu. Les deux
        // écritures nomment la même chose, et seule la configuration porte le suffixe.
        let adapter = adapter();

        // When
        let window = adapter.context_window(Some("claude-opus-5"), Some("opus[1m]"));

        // Then
        assert_eq!(window, Some(1_000_000));
    }

    #[test]
    fn given_a_configuration_naming_another_family_than_the_one_that_ran_when_the_window_is_asked_then_there_is_none(
    ) {
        // Given — la configuration annonce un million de tokens d'opus, et c'est un sonnet qui
        // a tourné : un `/model` changé en cours de session, ou une session que la
        // configuration ne décrit pas. Le numérateur vient de ce transcript-là, et le
        // dénominateur d'un modèle qui n'y est pour rien — cinq fois trop grand.
        let adapter = adapter();

        // When
        let window = adapter.context_window(Some("claude-sonnet-5"), Some("opus[1m]"));

        // Then — pas de fenêtre plutôt qu'une fenêtre d'emprunt.
        assert_eq!(window, None);
    }

    #[test]
    fn given_a_configuration_naming_another_version_than_the_one_that_ran_when_the_window_is_asked_then_there_is_none(
    ) {
        // Given — la configuration est précise (`claude-opus-5[1m]`), et un autre opus a
        // tourné. C'est le cas réel des sessions de revue de sécurité, qui choisissent leur
        // modèle sans passer par la configuration de l'utilisateur.
        let adapter = adapter();

        // When
        let window = adapter.context_window(Some("claude-opus-4-7"), Some("claude-opus-5[1m]"));

        // Then
        assert_eq!(window, None);
    }

    #[test]
    fn given_a_transcript_naming_a_family_the_adapter_does_not_know_when_the_window_is_asked_then_there_is_none(
    ) {
        // Given — `claude-fable-5` est un identifiant qui existe vraiment dans des transcripts,
        // et qu'aucune table ne reconnaît. La configuration, elle, est parfaitement lisible :
        // c'est exactement la situation où poser sa fenêtre serait la poser sur autre chose.
        let adapter = adapter();

        // When
        let window = adapter.context_window(Some("claude-fable-5"), Some("opus[1m]"));

        // Then
        assert_eq!(window, None);
    }

    #[test]
    fn given_the_four_places_claude_code_names_its_model_when_they_are_listed_then_the_repository_comes_before_the_home(
    ) {
        // Given — l'ordre **est** la règle, et il est celui de l'outil : la variable, puis les
        // réglages locaux du dépôt, puis ses réglages partagés, puis le foyer.
        let adapter = adapter();

        // When
        let sources =
            adapter.model_sources(Some(Path::new("/dev/ash")), Some(Path::new("/Users/x")));

        // Then
        assert_eq!(
            sources,
            vec![
                ModelSource::variable("ANTHROPIC_MODEL"),
                ModelSource::json_key("/dev/ash/.claude/settings.local.json", "model"),
                ModelSource::json_key("/dev/ash/.claude/settings.json", "model"),
                ModelSource::json_key("/Users/x/.claude/settings.json", "model"),
            ]
        );
    }

    #[test]
    fn given_a_turn_naming_its_model_when_the_adapter_reads_it_then_the_name_and_the_tokens_come_from_the_same_line(
    ) {
        // Given — un tour d'assistant réel : `"model"` et `"usage"` sont écrits côte à côte,
        // dans le même objet `message`. C'est ce voisinage qui rend le nom gratuit — le
        // chercher ailleurs demanderait une seconde lecture.
        let adapter = adapter();

        // When
        let read = adapter.read_turn(OWN_TRANSCRIPT);

        // Then
        assert_eq!(
            read,
            Some(Turn {
                used_tokens: 145_999,
                model: Some("claude-opus-5".to_owned()),
            })
        );
    }

    #[test]
    fn given_a_session_whose_model_changed_mid_way_when_the_adapter_reads_the_tail_then_the_last_turn_names_it(
    ) {
        // Given — l'utilisateur a tapé `/model` et choisi Sonnet, puis l'agent a produit un
        // tour. Le tour précédent, lui, est toujours dans le transcript et nomme encore Opus :
        // c'est très exactement le piège d'une lecture qui partirait du début.
        let adapter = adapter();
        let before = r#"{"type":"assistant","message":{"model":"claude-opus-5","usage":{"input_tokens":10}}}"#;
        let after = r#"{"type":"assistant","message":{"model":"claude-sonnet-5","usage":{"input_tokens":900}}}"#;

        // When
        let read = adapter.read_turn(&format!("{before}\n{after}\n"));

        // Then — le modèle suit la mesure, parce que les deux viennent du même tour.
        assert_eq!(
            read,
            Some(Turn {
                used_tokens: 900,
                model: Some("claude-sonnet-5".to_owned()),
            })
        );
    }

    #[test]
    fn given_a_transcript_model_and_a_configuration_carrying_the_long_context_suffix_when_it_is_named_then_it_reads_opus_five_one_m(
    ) {
        // Given — les deux sources, et chacune ce qu'elle est seule à savoir : le transcript
        // dit **ce qui a tourné**, la configuration dit **dans quelle fenêtre**. Aucune des
        // deux ne pourrait répondre seule.
        let adapter = adapter();

        // When
        let name = adapter.model_name("claude-opus-5", Some("opus[1m]"));

        // Then
        assert_eq!(name.as_deref(), Some("Opus 5 1M"));
    }

    #[test]
    fn given_no_long_context_suffix_in_the_configuration_when_the_model_is_named_then_it_reads_opus_five(
    ) {
        // Given — la même session, sur une configuration ordinaire : le transcript écrit le
        // même `claude-opus-5` dans les deux cas, et rien d'autre ne distingue les deux
        // fenêtres.
        let adapter = adapter();

        // When
        let named: Vec<Option<String>> = [None, Some("opus"), Some("claude-opus-5")]
            .into_iter()
            .map(|configured| adapter.model_name("claude-opus-5", configured))
            .collect();

        // Then
        assert_eq!(named, vec![Some("Opus 5".to_owned()); 3]);
    }

    #[test]
    fn given_a_configuration_that_does_not_name_the_model_that_ran_when_it_is_named_then_the_one_million_mark_is_not_borrowed(
    ) {
        // Given — un `opus[1m]` dans les réglages, un sonnet dans le transcript. Le suffixe
        // décrit la fenêtre d'une session qui n'est pas celle-ci, et c'est la fenêtre que la
        // jauge vient de refuser de calculer : l'écrire dans le nom la ferait rentrer par la
        // porte de derrière, en toutes lettres.
        let adapter = adapter();

        // When
        let name = adapter.model_name("claude-sonnet-5", Some("opus[1m]"));

        // Then
        assert_eq!(name.as_deref(), Some("Sonnet 5"));
    }

    #[test]
    fn given_a_dated_identifier_when_it_is_named_then_the_date_is_not_read_as_a_version() {
        // Given — les identifiants réels portent parfois leur millésime après leur version, et
        // avec le même séparateur. Le recopier donnerait `Haiku 4.5.20251001` dans une barre
        // de 12 px.
        let adapter = adapter();

        // When
        let named: Vec<Option<String>> = ["claude-haiku-4-5-20251001", "claude-opus-4-8", "opus"]
            .into_iter()
            .map(|ran| adapter.model_name(ran, None))
            .collect();

        // Then — la version est recopiée, le millésime écarté, et un alias sans version rend
        // sa famille seule.
        assert_eq!(
            named,
            vec![
                Some("Haiku 4.5".to_owned()),
                Some("Opus 4.8".to_owned()),
                Some("Opus".to_owned()),
            ]
        );
    }

    #[test]
    fn given_an_identifier_of_an_unknown_family_when_it_is_named_then_nothing_is_named() {
        // Given — un identifiant parfaitement réel dont Ash ne connaît pas la famille. C'est
        // la même porte que pour la fenêtre : reconnaître, ou se taire.
        let adapter = adapter();

        // When
        let named: Vec<Option<String>> = ["claude-fable-5", "gpt-5", "default", ""]
            .into_iter()
            .map(|ran| adapter.model_name(ran, Some("opus[1m]")))
            .collect();

        // Then — et surtout pas `Fable 5` : la barre annoncerait un modèle dont Ash ignore
        // tout, à commencer par la fenêtre qu'elle mesure juste à côté.
        assert_eq!(named, vec![None; 4]);
    }
}
