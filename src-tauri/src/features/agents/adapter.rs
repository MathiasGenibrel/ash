use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::state::AgentState;
use super::usage::{ModelSource, UsageSupport};

/// Le marqueur qu'Ash pose dans **chacune** de ses entrées, suivi de sa version.
///
/// C'est ce qui a remplacé le bloc délimité `ash:begin`/`ash:end`
/// ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), amendement du 2026-08-12) :
/// des marqueurs par entrée cohabitent ligne à ligne avec les hooks de l'utilisateur, là où
/// une région du fichier ne le pouvait pas.
///
/// **Il vit dans une ligne de commande, et c'est la forme la moins risquée qu'on ait
/// trouvée.** Une clé JSON inconnue posée au milieu d'un objet de hooks serait à la merci
/// d'un schéma strict : au pire, l'outil refuserait tout le fichier, et l'utilisateur
/// perdrait ses réglages à cause d'Ash. Un commentaire de shell en fin de commande est
/// inerte pour le shell qui l'exécute — la commande d'un hook est déjà une ligne de shell,
/// puisqu'elle contient `"$ASH_TAB_ID"` — et invisible pour tout schéma JSON. L'asymétrie
/// des deux risques tranche toute seule.
pub const HOOK_MARK: &str = "# ash:hook v";

/// Le marqueur d'une version donnée, tel qu'il s'écrit dans le fichier.
pub fn hook_mark(version: u32) -> String {
    format!("{HOOK_MARK}{version}")
}

/// Une entrée qu'Ash veut voir dans la configuration d'un outil, et où.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEntry {
    /// La chaîne de clés d'objet qui mène au **tableau** où l'entrée doit vivre —
    /// `["hooks", "Stop"]` pour Claude Code.
    ///
    /// C'est ce qui permet à la feature `hooks` de fusionner sans connaître un seul outil :
    /// elle sait descendre une chaîne de clés, créer ce qui manque, et insérer dans ce qui
    /// existe. Le nom des événements, lui, ne sort pas de l'adaptateur.
    pub path: Vec<String>,

    /// L'objet à poser dans ce tableau, sérialisé **sur une ligne**, marqueur compris.
    ///
    /// Une ligne parce que c'est ce qui rend le retrait exact : la plage retirée est
    /// exactement celle qui avait été insérée, sans reste ni ligne vide.
    pub item: String,
}

/// Ce qu'Ash doit écrire dans la configuration d'un outil pour qu'il parle.
///
/// L'adaptateur décrit **quoi** écrire ; il n'écrit rien. Le marqueur par entrée, la
/// sauvegarde `.bak`, la fusion qui ne perd aucun hook de l'utilisateur et le retrait qui ne
/// laisse rien sont une règle transverse qui n'a qu'un propriétaire dans le code — la
/// feature `hooks` ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)). Un adaptateur
/// qui ouvrirait lui-même le fichier de l'utilisateur ferait exister une deuxième façon
/// d'écrire chez lui, donc une deuxième façon de se tromper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instrumentation {
    /// Le fichier à instrumenter, **sous le dossier de configuration** qu'on a donné.
    ///
    /// C'est un chemin et non un nom parce que les outils ne rangent pas leur
    /// configuration au même endroit : `settings.json` à la racine pour l'un, un
    /// sous-dossier pour un autre. La contrainte « sous `config_dir` » est vérifiée par la
    /// suite contractuelle : Ash écrit chez l'utilisateur, et la cible ne se négocie pas.
    pub file: PathBuf,

    /// Les entrées à poser, dans l'ordre où l'adaptateur les veut.
    pub entries: Vec<HookEntry>,

    /// La version des entrées, incrémentée dès que leur contenu change de forme.
    ///
    /// C'est elle qui permet de reconnaître une entrée écrite par une version antérieure
    /// d'Ash et de la réécrire, au lieu de la prendre pour une édition de l'utilisateur.
    pub version: u32,
}

/// Un événement brut, dans le vocabulaire de l'outil qui l'a émis.
///
/// C'est la frontière d'entrée d'un adaptateur, et la seule chose qu'il a le droit de
/// regarder. Elle ne porte **pas** l'onglet concerné : le routage — `ASH_TAB_ID`, l'ordre
/// d'arrivée, l'horloge — appartient au transport et à la machine à états. Un adaptateur
/// qui verrait l'onglet pourrait décider à leur place.
///
/// `kind` est le nom que l'outil donne à son événement (`Stop`, `Notification`,
/// `PreToolUse`…) : c'est l'endroit exact où les outils divergent, et donc la raison d'être
/// d'[`Adapter::interpret`]. `fields` porte le reste, non typé, parce qu'on ne sait pas
/// aujourd'hui ce que `codex`, `kimi` ou `opencode` enverront — et parce qu'un champ
/// inconnu doit pouvoir traverser le transport sans qu'Ash ait à le connaître.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEvent {
    kind: String,
    fields: BTreeMap<String, String>,
}

impl RawEvent {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            fields: BTreeMap::new(),
        }
    }

    /// Ajoute un champ — aussi le builder des tests, pour n'avoir qu'une façon d'en fabriquer.
    #[must_use]
    pub fn with_field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(name.into(), value.into());
        self
    }

    /// Le nom que l'outil donne à cet événement.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}

/// Ce qu'un événement dit du **cycle de vie d'un enfant**, et rien de l'onglet.
///
/// C'est la « méthode distincte » qu'exige l'amendement du 2026-08-13 à
/// [ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md) : le cycle de vie des
/// sous-agents ne passe **pas** par [`Adapter::interpret`], parce qu'un sous-agent qui finit
/// ne rend pas l'outil disponible. Le traduire en état d'onglet serait exactement la
/// déduction que l'ADR refuse, et la suite contractuelle vérifie qu'aucune implémentation ne
/// le fait ([`super::contract`]).
///
/// **Une seule variante, et c'est un constat, pas un oubli.** Aucun outil n'annonce le
/// *démarrage* d'un sous-agent : Claude Code n'a pas de `SubagentStart`. La naissance se lit
/// donc au premier événement portant un `agent_id` encore inconnu, ce qui est une affaire de
/// transport et non de vocabulaire — l'adaptateur n'aurait rien à en dire. Ce qu'il est seul
/// à savoir, c'est quel verbe signifie « cet enfant-là s'arrête ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildEvent {
    /// L'enfant nommé par la trame vient de terminer.
    Ended,
}

/// Ce qu'un événement dit de la **session** de l'outil, et rien de ce qu'elle fait.
///
/// La troisième lecture du même mot brut, à côté d'[`Adapter::interpret`] et
/// d'[`Adapter::child_event`], et elle existe pour la même raison que la deuxième : une
/// session qui s'ouvre n'est pas un état de travail. Un outil qui vient de démarrer n'est
/// **rien** en train de faire — il attend un prompt — et le traduire en `working` serait la
/// déduction qu'ADR-0007 refuse, cette fois par le vocabulaire plutôt que par la sortie du
/// PTY (précision du 2026-08-24).
///
/// Ce qu'elle apporte au cœur est pourtant décisif : tant qu'aucun événement n'est arrivé
/// d'un onglet, c'est la **présence** vue par la sonde qui y répond, donc `claude` à son
/// invite s'y montre `working`. Un verbe qui dit « une session existe » fait naître la
/// machine à états de l'onglet sans rien y déclarer, et la présence cesse d'y parler.
///
/// **Une seule variante**, et c'est un choix : la fin d'une session, elle, est un état —
/// `SessionEnd` se traduit en `done` par [`Adapter::interpret`], et n'a rien à faire ici.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    /// Une session de l'outil vient de s'ouvrir dans cet onglet — démarrage ou reprise.
    Opened,
}

/// Un outil expose-t-il des sous-tâches ?
///
/// La notion n'existe pas partout, et c'est la seule chose que le cœur a besoin de savoir
/// pour décider s'il peut afficher des lignes filles
/// ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md), spec §6.5). La hiérarchie
/// elle-même n'est pas modélisée ici : déclarer la capacité et la décrire sont deux
/// tâches, et seule la première est nécessaire pour que la sidebar ne suppose rien.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentSupport {
    /// L'outil n'a pas de sous-tâches, ou n'en dit rien.
    None,
    /// L'outil rapporte ses sous-tâches dans ses événements.
    Reported,
}

/// L'intégration d'un outil de code, derrière un trait
/// ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)).
///
/// Les méthodes correspondent aux endroits où `claude`, `codex`, `kimi` et `opencode`
/// divergent réellement : leur nom, le mécanisme et l'emplacement de leur instrumentation,
/// leur vocabulaire d'états, le verbe par lequel un **enfant** annonce sa fin, l'existence de
/// sous-tâches, et ce qu'ils savent dire de la place qu'ils consomment. Rien d'autre ne
/// franchit la frontière : le cœur ne connaît que [`AgentState`], et un adaptateur n'a aucun
/// moyen de lui faire connaître un sixième mot.
///
/// **Deux des capacités sont optionnelles, et elles se déclarent avant de se décrire** —
/// [`Adapter::subagents`] et [`Adapter::usage`]. C'est ce qui permet au cœur de ne rien
/// afficher plutôt que d'afficher un vide : un outil qui n'a pas de sous-tâches n'a pas de
/// lignes filles, et un outil qui ne tient pas de transcript n'a pas de jauge — pas une
/// jauge à zéro.
///
/// **Le vocabulaire des états et celui des enfants sont deux méthodes**, et pas une avec un
/// cas de plus : c'est la forme que l'amendement du 2026-08-13 à ADR-0007 exige, pour
/// qu'un événement d'enfant n'ait aucun chemin vers l'état de l'onglet.
///
/// `Send + Sync` parce que les adaptateurs sont partagés entre le fil qui reçoit les
/// événements et celui qui sonde les onglets.
pub trait Adapter: Send + Sync {
    /// L'identifiant stable de l'outil — `claude-code`, `generic`.
    ///
    /// Il sert de clé : la configuration reconnue d'ADR-0006 le désigne, et l'attribution
    /// d'un commit le retient ([ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md)).
    /// Le changer rétroactivement détacherait des agents déjà journalisés.
    fn id(&self) -> &str;

    /// Ce que l'adaptateur doit écrire dans la configuration de l'outil.
    ///
    /// `None` quand l'outil n'expose aucun point d'instrumentation — et c'est alors, à soi
    /// seul, la déclaration qu'aucun état ne viendra de ses hooks.
    ///
    /// Le dossier est passé en paramètre parce qu'un même outil peut en avoir plusieurs :
    /// deux comptes Claude, c'est deux `config_dir` et deux blocs à écrire (ADR-0007).
    fn instrumentation(&self, config_dir: &Path) -> Option<Instrumentation>;

    /// Traduit un événement brut en état Ash, ou en rien.
    ///
    /// `None` veut dire « cet événement ne dit rien de l'état » — un événement inconnu, ou
    /// connu mais sans conséquence. C'est le cas normal, pas une erreur : deviner serait
    /// exactement ce qu'ADR-0007 refuse.
    ///
    /// L'événement est emprunté : l'appelant en garde la propriété parce que c'est lui qui
    /// détient le routage et l'horodatage qui l'accompagnent.
    fn interpret(&self, raw: &RawEvent) -> Option<AgentState>;

    /// Ce que cet événement dit d'un **enfant**, ou rien.
    ///
    /// Distincte d'[`Self::interpret`] parce qu'elle répond à une autre question, et que
    /// confondre les deux est précisément ce que l'amendement du 2026-08-13 à ADR-0007
    /// interdit : un `SubagentStop` ne rend pas l'onglet disponible. Un adaptateur qui
    /// répond ici doit donc répondre `None` là — c'est un invariant du contrat.
    ///
    /// `None` est la réponse normale, y compris chez un outil qui a des sous-tâches : la
    /// grande majorité des événements ne parlent que de l'agent principal.
    fn child_event(&self, raw: &RawEvent) -> Option<ChildEvent>;

    /// Ce que cet événement dit de la **session**, ou rien.
    ///
    /// La troisième porte du trait, distincte des deux autres pour la raison qui les a déjà
    /// séparées : les trois lisent le même mot brut et n'en tirent pas la même chose, et un
    /// mot ne peut pas passer par deux portes à la fois — la suite contractuelle le vérifie.
    ///
    /// `None` est la réponse normale, y compris chez un outil qui annonce ses sessions : la
    /// quasi-totalité des événements parlent de ce que l'agent fait, pas de son ouverture.
    /// Un adaptateur sans instrumentation répond toujours `None` — il n'a fait installer
    /// aucun hook, donc rien ne peut lui parvenir.
    fn session_event(&self, raw: &RawEvent) -> Option<SessionEvent>;

    /// L'outil expose-t-il des sous-tâches ?
    fn subagents(&self) -> SubagentSupport;

    /// L'outil dit-il la place qu'il consomme dans sa fenêtre de contexte ?
    ///
    /// La quatrième capacité optionnelle du trait, et la deuxième à se déclarer avant d'être
    /// décrite — comme [`Self::subagents`], et pour la même raison : le cœur a besoin de
    /// savoir s'il **peut** afficher une jauge avant de savoir ce qu'elle vaudrait. Un outil
    /// qui répond [`UsageSupport::None`] n'a pas d'usage du tout, et rien dans la barre
    /// n'ira suggérer qu'il en manque un.
    fn usage(&self) -> UsageSupport;

    /// Ce que la fin d'un transcript dit de la place consommée, ou rien.
    ///
    /// **L'adaptateur interprète, il ne lit pas le disque.** C'est le même partage que pour
    /// [`Self::instrumentation`], qui décrit ce qu'il faut écrire sans jamais écrire : le
    /// format d'un transcript est ce que seul l'outil connaît, et l'ouverture d'un fichier
    /// est un effet système que la feature possède ([`super::usage::Transcripts`]). Un
    /// adaptateur qui ouvrirait lui-même le fichier ferait exister une deuxième façon de
    /// lire chez l'utilisateur, donc une deuxième façon de se tromper.
    ///
    /// Ce qu'il reçoit est une **queue**, pas le fichier : elle peut commencer n'importe où,
    /// et une implémentation doit donc tolérer des lignes qu'elle ne comprend pas plutôt que
    /// de s'arrêter à la première.
    ///
    /// `None` est la réponse normale — une queue sans tour d'assistant, un format qui a
    /// changé, un fichier encore vide. Ce n'est pas une erreur : l'onglet garde ce qu'il
    /// savait déjà.
    ///
    /// **Un tour, et pas un pourcentage ni une fenêtre.** Le transcript mesure le
    /// numérateur, et rien d'autre : le dénominateur ne s'y trouve pas, et il a coûté un bug
    /// de faire semblant du contraire ([`Self::context_window`]).
    ///
    /// **Les tokens et le modèle sortent ensemble parce qu'ils sont écrits ensemble.** Un
    /// tour d'assistant porte son `usage` et son identifiant de modèle sur la même ligne :
    /// deux méthodes feraient deux parcours de la queue, et laisseraient le nom d'un tour
    /// rencontrer la mesure d'un autre.
    fn read_turn(&self, transcript_tail: &str) -> Option<super::usage::Turn>;

    /// Le **nom court** de ce modèle, tel qu'il s'écrit dans la barre — ou rien.
    ///
    /// La table des noms vit ici, à côté de celle des fenêtres ([`Self::context_window`]) et
    /// pour la même raison : `claude-opus-5` est un mot de l'outil, que ni le cœur ni l'écran
    /// n'ont à connaître. Un identifiant qu'on ne sait pas nommer rend `None`, et le segment
    /// disparaît alors entièrement — jamais un tiret, jamais `unknown`. C'est la règle
    /// d'[`Self::context_window`], appliquée à l'autre moitié de ce que l'identifiant dit.
    ///
    /// Deux entrées, parce qu'aucune ne suffit :
    ///
    /// - `ran` est l'identifiant que le **transcript** a écrit. C'est ce qui a réellement
    ///   tourné, donc ce qui suit un `/model` changé en cours de session — au premier tour
    ///   d'agent qui suit le changement.
    /// - `configured` est celui que la **configuration** nomme, quand elle en nomme un. Elle
    ///   seule porte le suffixe `[1m]` : le transcript écrit `claude-opus-5` qu'on tourne en
    ///   200 k ou en 1 M, et sans elle il n'y aurait aucun moyen de distinguer les deux.
    fn model_name(&self, ran: &str, configured: Option<&str>) -> Option<String>;

    /// Où cet outil peut nommer le modèle avec lequel il tourne, **du plus spécifique au
    /// moins spécifique**.
    ///
    /// L'ordre *est* la règle, et il appartient à l'adaptateur parce qu'il reproduit celui de
    /// l'outil : pour Claude Code, `ANTHROPIC_MODEL`, puis le `settings.local.json` du dépôt,
    /// puis son `settings.json`, puis celui du foyer.
    ///
    /// **Ce sont des adresses, jamais des contenus** : l'adaptateur décrit où regarder, la
    /// feature ouvre ([`super::usage::ToolConfig`]). Le même partage que pour
    /// [`Self::instrumentation`], et pour la même raison — un adaptateur qui lirait le disque
    /// ferait exister une deuxième façon de lire chez l'utilisateur.
    ///
    /// `cwd` est le dossier où l'agent tourne, quand on le connaît ; `home` le dossier
    /// personnel, résolu par la feature. Les deux sont facultatifs, et une liste **vide** est
    /// la réponse normale d'un outil qui répond [`UsageSupport::None`].
    fn model_sources(&self, cwd: Option<&Path>, home: Option<&Path>) -> Vec<ModelSource>;

    /// La fenêtre dans laquelle cette session tourne, si l'outil peut le dire **des deux
    /// identifiants à la fois**.
    ///
    /// **C'est ici que vit la table, et nulle part ailleurs** : `opus[1m]` ne veut rien dire
    /// pour le cœur, et un identifiant que l'outil ne reconnaît pas ne doit surtout pas
    /// tomber sur une valeur par défaut. C'est très exactement le bug qu'un
    /// `DEFAULT_CONTEXT_WINDOW = 200_000` a produit : un pourcentage cinq fois trop haut,
    /// affiché avec l'aplomb d'une mesure.
    ///
    /// Les deux entrées sont celles de [`Self::model_name`], et pour une raison plus forte
    /// que la symétrie : **le numérateur et le dénominateur ne viennent pas de la même
    /// source**. `ran` est ce que le transcript a écrit — c'est de cette conversation-là que
    /// vient la mesure —, `configured` ce que la configuration annonce, et c'est elle seule
    /// qui porte le suffixe `[1m]`. Quand les deux ne parlent pas du même modèle, l'outil
    /// répond `None` : mieux vaut aucune jauge qu'un pourcentage calculé sur la fenêtre d'un
    /// autre modèle. Un `ran` absent ne contredit rien, et la configuration répond seule.
    ///
    /// `None` est donc la bonne réponse pour tout ce qui n'est pas reconnu — ou pas
    /// d'accord —, et la jauge disparaît alors sans que la mesure disparaisse avec elle.
    fn context_window(&self, ran: Option<&str>, configured: Option<&str>) -> Option<u64>;
}
