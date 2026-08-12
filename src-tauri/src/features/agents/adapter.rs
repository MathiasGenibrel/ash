use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::state::AgentState;

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
/// Les quatre méthodes correspondent aux quatre endroits où `claude`, `codex`, `kimi` et
/// `opencode` divergent réellement : leur nom, le mécanisme et l'emplacement de leur
/// instrumentation, leur vocabulaire d'événements, et l'existence de sous-tâches. Rien
/// d'autre ne franchit la frontière : le cœur ne connaît que [`AgentState`], et un
/// adaptateur n'a aucun moyen de lui faire connaître un sixième mot.
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

    /// L'outil expose-t-il des sous-tâches ?
    fn subagents(&self) -> SubagentSupport;
}
