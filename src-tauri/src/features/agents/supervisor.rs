//! Le superviseur : une [`AgentMachine`] par onglet, et les deux sources qui la nourrissent.
//!
//! C'est la couture qui manquait entre les trois pièces de la feature. Elle vit **ici**, et
//! ni dans `pty` ni dans le composition root, pour trois raisons qui tiennent ensemble :
//!
//! - le registre de `pty` ne connaît aujourd'hui que le **vocabulaire** [`AgentState`], ce
//!   qui est sain. Lui donner le mécanisme de décision — les adaptateurs, les hooks,
//!   l'horloge des trente secondes — le ferait déborder de son sujet, qui est de tenir des
//!   PTY ;
//! - le composition root n'a pas de test unitaire et n'en aura jamais. Une règle de produit
//!   qui s'y glisse n'en a pas non plus ;
//! - c'est ce qui tient la promesse d'[ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md) :
//!   le jour où les PTY passeront dans un démon `ashd`, les machines partiront avec eux,
//!   sans qu'aucune règle ne soit restée dans l'assemblage.
//!
//! `pty` pose donc une **question** — « quel état pour cet onglet, compte tenu de ce que la
//! sonde voit ? » — par un trait qu'il possède ([`crate::features::pty::AgentStates`]), et
//! le superviseur y répond. C'est la convention du dépôt (les effets système passent par un
//! trait que la feature possède) appliquée à une décision plutôt qu'à un effet.
//!
//! ## Qui a le droit de dire quoi
//!
//! | Source | Ce qu'elle produit |
//! |---|---|
//! | Un hook, traduit par [`Adapter::interpret`] | `working`, `waiting`, `done`, `error` |
//! | Un hook de session, par [`Adapter::session_event`] | **rien** — mais l'onglet cesse d'être servi par la sonde |
//! | La sonde ([`Presence`]) | la **présence** d'un programme, et sa disparition |
//!
//! La deuxième ligne est la précision du 2026-08-24 à ADR-0007, et c'est tout ce qu'elle
//! change : **pour un outil instrumenté, ce sont les hooks qui disent ce que l'agent fait**.
//! Un `SessionStart` fait naître la machine de l'onglet sans y déclarer quoi que ce soit,
//! donc `claude` qui attend un prompt s'y montre `idle` au lieu de `working`. Un outil
//! **sans** hooks n'envoie jamais ce verbe : aucune machine ne naît dans son onglet, et la
//! sonde continue d'y répondre `working` sur la seule présence (spec §6.2) — c'est
//! exactement la raison d'être des deux producteurs de `working`, et elle ne bouge pas.
//!
//! **Les lignes filles ont leur propre découpe du même flux, et elle est plus courte** : une
//! ligne fille appartient à la session qui l'a créée, et ne lui survit pas (spec §6.5). Ce
//! fichier a donc deux endroits — un seul geste, écrit deux fois parce que les deux sources
//! le produisent — où [`Subagents::session_over`] est appelée : quand une session s'ouvre, et
//! quand l'onglet entre dans un état terminal, ce qui est très exactement ce que la machine
//! appelle « la session se ferme » ([`has_finished`]). Rien d'autre ne les touche, et surtout
//! pas un parent qui passe `waiting`.
//!
//! ## Ce qui date un état
//!
//! Le superviseur est aussi le seul endroit qui sache **depuis quand** un onglet est dans
//! l'état qu'il montre : la machine ne répond que pour les onglets où un agent a parlé,
//! alors que la ligne de statut date aussi bien un `vim` que la sonde seule décrit. Ce qui
//! est daté est donc le **verdict**, quelle qu'en soit la source, et la date ne bouge que
//! quand le verdict change. La règle elle-même vit chez le type qu'elle produit —
//! [`AgentStatus::entering`] — et non ici : `pty` en a besoin pour sa doublure, et une
//! seconde copie de trois lignes se serait tue le jour où celle-ci aurait changé.
//!
//! **`waiting` n'a aucune autre source qu'un hook**, et c'est structurel plutôt que
//! surveillé : la sonde n'entre dans ce fichier que par [`Presence`], qui ne porte que trois
//! valeurs et aucun état. Rien n'y lit la sortie du PTY
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
//!
//! ## Un onglet devient un agent, puis redevient un shell
//!
//! Une machine ne naît qu'au premier hook, et meurt dès que son verdict retombe sur `idle`
//! **sans qu'une session la retienne** ([`AgentMachine::holds_a_session`]).
//! Entre les deux, c'est elle qui répond ; en dehors, c'est la sonde — un onglet où personne
//! n'a jamais parlé montre `working` tant qu'un programme tient l'avant-plan, et `idle`
//! sinon, exactement comme avant cette tranche. C'est ce qui évite d'annoncer la fin d'un
//! agent quand l'utilisateur quitte `vim`
//! ([ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)).
//!
//! ## Ce que la sonde n'a pas le droit d'attribuer
//!
//! La machine sait repartir au travail quand un agent démarre sur la ligne d'un agent fini —
//! c'est la flèche « on relance `claude` dans l'onglet qu'on vient de lire »
//! ([`AgentEvent::AgentStarted`]). **Ce fichier ne l'émet pourtant jamais**, et c'est
//! délibéré : la sonde ne rend qu'une [`Presence`], donc rien ici ne distingue `claude` de
//! `cargo test`. Attribuer le front à l'agent reviendrait à parier, et le pari se paie cher
//! dans un seul sens :
//!
//! - une commande ordinaire tapée dans les trente secondes qui suivent un `done` — le geste
//!   le plus courant qui soit — repasserait l'onglet en `working`, puis sa fin donnerait
//!   `error` par [`Exit::Unseen`]. Un `cargo test` vert afficherait un échec, et l'onglet
//!   resterait accroché à cette machine **bien au-delà** des trente secondes, puisqu'un état
//!   actif n'expire jamais ;
//! - à l'inverse, ne rien attribuer laisse la ligne `done` en place pendant qu'un agent
//!   relancé démarre, jusqu'à son premier hook — au plus tard jusqu'à l'expiration des
//!   trente secondes, après quoi la sonde reprend la main et dit `working`.
//!
//! Le second se corrige tout seul et n'annonce rien de faux ; le premier détruit exactement
//! ce que l'état sert à porter. La flèche reste donc dans la machine, où elle est prouvée, et
//! attend son vrai producteur : la reconnaissance d'une commande d'agent par son nom
//! ([ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)), qui est une
//! tranche à part.
//!
//! **Limite connue, elle non bornée ici** : tant que cette reconnaissance n'existe pas, la
//! disparition d'un programme quelconque de l'avant-plan d'un onglet où un agent vit encore
//! est prise pour celle de l'agent. Un `Ctrl-Z` sur un agent en `waiting` donne donc `error`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::adapter::{Adapter, ChildEvent, RawEvent, SessionEvent};
use super::machine::{has_finished, AgentEvent, AgentMachine, Declared, Exit};
use super::notify::{notice, Notice, Notifier};
use super::preferences::{NotificationChoices, NotificationPreferences};
use super::state::{AgentState, AgentStatus};
use super::subagents::{Subagent, Subagents};
use super::usage::{self, SessionUsage, ToolConfig, Transcripts};
use super::wire::EventFrame;
use crate::shared::time::{Clock, UnixMillis};

/// Ce que la sonde voit d'un onglet — et tout ce qu'elle a le droit d'en dire.
///
/// Trois valeurs, aucune conclusion : c'est le point exact où ADR-0007 se tient. La sonde
/// répond à une question de **présence** (`tcgetpgrp`), pas à « que fait cet agent ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Presence {
    /// Le shell tient l'avant-plan : rien ne tourne dans cet onglet.
    Prompt,
    /// Un programme tient l'avant-plan. Lequel ne regarde pas cette feature.
    Program,
    /// La sonde n'a rien pu observer.
    ///
    /// Ce n'est pas « rien ne tourne » : un onglet dont le système ne répond plus doit
    /// garder l'état qu'un hook lui a donné, pas retomber à `idle` parce qu'un appel
    /// système a échoué.
    #[default]
    Unknown,
}

/// Les machines à états, une par onglet, et ce qu'il faut pour les nourrir.
pub struct Supervisor {
    clock: Arc<dyn Clock>,
    /// Les adaptateurs embarqués, dans l'ordre où le composition root les a déclarés.
    ///
    /// La trame ne dit pas de quel outil elle vient — elle porte le vocabulaire canonique
    /// de la spec §6.3, que tous les adaptateurs partagent — donc c'est le premier qui
    /// reconnaît le mot qui répond. Un adaptateur sans instrumentation, `generic` en tête,
    /// ne reconnaît rien et ne peut donc rien avaler au passage.
    adapters: Vec<Arc<dyn Adapter>>,
    /// Où partent les interruptions de la spec §8.
    ///
    /// **Le superviseur est le seul endroit du produit qui sache qu'un état vient de
    /// changer**, par opposition à *être* : c'est lui qui reçoit le `Some(état)` des
    /// machines, et la boucle de sonde qui l'appelle ne voit, elle, qu'un état. Poser la
    /// notification ailleurs reviendrait à la poser sur une lecture, donc trois fois par
    /// seconde ([`super::notify`]).
    notifier: Arc<dyn Notifier>,
    /// Ce que l'utilisateur laisse interrompre (spec §9, `[notifications]`).
    ///
    /// **Consulté ici, sur le chemin qui poste**, et non par la fenêtre qui affiche : une
    /// bannière ne sort que quand Ash est en arrière-plan, donc un filtre côté interface ne
    /// pourrait cacher que ce que l'utilisateur a déjà vu passer. C'est aussi ce qui fait
    /// que le réglage vaut pour la notification et pour rien d'autre — la sidebar continue
    /// de montrer les cinq états, quels que soient les trois interrupteurs.
    preferences: Arc<NotificationPreferences>,
    /// Combien de temps la ligne d'un sous-agent fini reste visible (spec §6.5).
    ///
    /// Injectée, et non lue d'une constante : c'est un **réglage**, dont le composition root
    /// pose la valeur par défaut ([`super::SUBAGENT_LINGER`]). C'est aussi ce qui permet à un
    /// scénario de la décrire au lieu de la subir.
    subagent_linger: Duration,
    /// Par où la fin d'un transcript se lit (`usage.rs`).
    ///
    /// Un port, pour la raison habituelle : ouvrir un fichier est un effet système, et les
    /// scénarios du superviseur doivent pouvoir décrire un transcript au lieu d'en écrire un.
    transcripts: Arc<dyn Transcripts>,
    /// Par où la configuration d'un outil se lit (`usage.rs`).
    ///
    /// Le second port de la mesure, et il existe pour la même raison que le premier : lire un
    /// `settings.json` et une variable d'environnement sont des effets système, et sans lui,
    /// aucun scénario ne pourrait décrire un utilisateur tournant en `opus[1m]` sans toucher
    /// au `~/.claude` de qui lance les tests.
    ///
    /// Il est consulté **au même rythme que la mesure** — à l'arrivée d'un hook portant un
    /// transcript, jamais à une passe de sonde ni sur un chemin de rendu.
    config: Arc<dyn ToolConfig>,
    tabs: Mutex<Tabs>,
}

/// Ce que le superviseur répond pour un onglet : son état daté, et ses enfants.
///
/// Une seule réponse et non deux questions, parce que les deux se lisent sous le même verrou
/// et à la même passe : demander l'état puis les enfants laisserait une passe de sonde
/// s'intercaler entre les deux, et la sidebar afficherait des lignes filles arbitrées à un
/// autre instant que la ligne qui les porte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabAgents {
    pub status: AgentStatus,
    /// Les lignes filles, dans leur ordre d'apparition. Vide dans l'écrasante majorité des
    /// cas — un onglet sans sous-agent, ou un outil qui n'en expose pas.
    pub subagents: Vec<Subagent>,
    /// La place que la conversation occupe, quand l'outil sait la dire (`usage.rs`).
    ///
    /// `None` couvre trois cas que rien ne distingue à l'écran, et c'est voulu : l'outil ne
    /// tient pas de transcript, aucun hook n'en a encore nommé un, ou la queue lue ne portait
    /// aucun tour. Dans les trois, la barre d'état reste ce qu'elle était — pas de jauge
    /// vide, rien qui laisse croire qu'une mesure a échoué.
    ///
    /// Elle voyage avec l'onglet, et par le même chemin que l'état : une mesure n'a pas plus
    /// besoin d'un canal à elle qu'un état (ADR-0009).
    pub usage: Option<SessionUsage>,
}

#[derive(Default)]
struct Tabs {
    /// La fenêtre Ash est-elle au premier plan ? Un **niveau**, tenu ici pour que les
    /// machines nées plus tard le connaissent aussi.
    focused: bool,
    live: HashMap<String, Tab>,
}

struct Tab {
    /// La machine de cet onglet, tant qu'un agent y vit.
    machine: Option<AgentMachine>,
    /// Les sous-agents qui vivent **dans** cet onglet (spec §6.5).
    ///
    /// Ils sont tenus à côté de la machine, et non dedans : la machine décide de l'état de
    /// l'onglet, et aucun événement d'enfant n'a le droit d'y entrer (ADR-0007, amendement du
    /// 2026-08-13). Les deux voisinent sans se parler.
    children: Subagents,
    /// Le dernier verdict rendu pour cet onglet, **avec sa date d'entrée**.
    ///
    /// C'est ici que le temps est retenu, et pas dans la machine : la machine ne répond que
    /// pour les onglets où un agent a parlé, alors que la ligne de statut date aussi bien un
    /// `vim` que la sonde seule décrit. Ce que l'on date est donc le verdict — ce que
    /// l'utilisateur voit — quelle qu'en soit la source.
    ///
    /// La date ne bouge **que** quand l'état change : c'est ce qui laisse la fiche d'un
    /// onglet identique d'une passe à l'autre (voir [`AgentStatus`]).
    answered: Option<AgentStatus>,
    /// La dernière mesure lue pour cet onglet, ou rien.
    ///
    /// **Retenue entre deux hooks, et c'est ce qui la rend stable.** Elle n'est relue qu'à
    /// l'arrivée d'un hook — jamais à une passe de sonde : le transcript ne bouge que quand
    /// l'agent parle, et le relire trois fois par seconde ferait payer une lecture de disque
    /// par onglet et par passe pour un nombre qui n'a pas changé.
    ///
    /// Elle vit **à côté** de la machine, comme les enfants, et pour une raison voisine :
    /// aucune mesure n'a de chemin vers l'état de l'onglet.
    usage: Option<SessionUsage>,
    /// Ce que la sonde a vu la dernière fois — de quoi reconnaître un **front**.
    ///
    /// Retenu même sans machine : sans ça, la machine créée par le premier hook prendrait le
    /// programme déjà en cours pour un agent qui démarre, et remettrait au travail celui qui
    /// vient de déclarer sa fin.
    seen: Presence,
}

impl Tab {
    fn new(subagent_linger: Duration) -> Self {
        Self {
            machine: None,
            children: Subagents::new(subagent_linger),
            usage: None,
            answered: None,
            seen: Presence::default(),
        }
    }
}

impl Supervisor {
    pub fn new(
        clock: Arc<dyn Clock>,
        adapters: Vec<Arc<dyn Adapter>>,
        notifier: Arc<dyn Notifier>,
        preferences: Arc<NotificationPreferences>,
        subagent_linger: Duration,
        transcripts: Arc<dyn Transcripts>,
        config: Arc<dyn ToolConfig>,
    ) -> Self {
        Self {
            clock,
            adapters,
            notifier,
            preferences,
            subagent_linger,
            transcripts,
            config,
            tabs: Mutex::new(Tabs::default()),
        }
    }

    /// Un hook a parlé. C'est la seule source de `waiting`, et elle fait autorité.
    ///
    /// Le mot brut traverse l'adaptateur avant d'atteindre la machine : un verbe qu'aucun
    /// adaptateur ne reconnaît ne produit rien du tout — ni état, ni erreur. Deviner serait
    /// exactement ce qu'ADR-0007 écarte.
    pub fn on_hook(&self, event: &EventFrame) {
        // Les **trois** lectures du même mot brut, et elles ne se recouvrent jamais : un
        // verbe d'état n'est ni un verbe d'enfant ni un verbe de session, et la suite
        // contractuelle le vérifie sur chaque adaptateur (ADR-0007, amendement du
        // 2026-08-13, précision du 2026-08-24).
        let declared = self.translate(&event.kind);
        let child = self.child_event(&event.kind);
        // La troisième est la seule qui ne dise rien de ce que l'agent fait : elle annonce
        // qu'une session existe. Ce qu'elle produit est la **machine** de l'onglet, et c'est
        // par là que la présence cesse d'y répondre.
        let session = self.session_event(&event.kind);

        // La lecture du transcript se fait **avant** le verrou, et c'est sa place : c'est le
        // seul accès au disque de ce chemin, et le tenir pendant qu'on lit ferait attendre
        // toutes les passes de sonde le temps d'une entrée-sortie.
        //
        // Elle ne dépend pas du verbe : `transcript_path` arrive sur **tous** les événements,
        // y compris ceux dont aucun adaptateur ne tire d'état. Une mesure fraîche apportée par
        // un `PreToolUse` vaut celle d'un `Stop`.
        let measured = self.measure(event);

        if declared.is_none() && child.is_none() && session.is_none() && measured.is_none() {
            // Un verbe qu'aucun adaptateur ne reconnaît ne produit rien du tout — ni état, ni
            // ligne fille. Un enfant révélé par un mot inconnu serait deviné.
            return;
        }

        // L'heure est lue **avant** le verrou : rien de ce qui se décide dessous n'en
        // dépend, et une section critique ne s'allonge pas d'un appel système gratuit.
        let now = self.clock.wall();

        // Le verrou est rendu **avant** de poster : une notification est un effet système,
        // et le tenir pendant qu'on sort de la feature ferait dépendre la boucle de sonde
        // de ce que le système met à répondre.
        let interruption = {
            let Ok(mut tabs) = self.tabs.lock() else {
                return;
            };

            let focused = tabs.focused;
            let clock = Arc::clone(&self.clock);
            let tab = tabs
                .live
                .entry(event.tab_id.clone())
                .or_insert_with(|| Tab::new(self.subagent_linger));

            // L'enfant d'abord, et à part : quoi qu'il arrive ensuite à l'onglet, ce qui suit
            // ne touche que ses lignes filles.
            note_child(&mut tab.children, event, child, now);

            // La mesure ensuite, et à part elle aussi : elle est retenue **avant** le retour
            // du cas « ce hook ne dit rien de l'onglet », parce qu'un `SubagentStop` porte un
            // transcript aussi frais qu'un autre. Une mesure absente n'efface pas la
            // précédente : l'onglet garde ce qu'il savait.
            if measured.is_some() {
                tab.usage = measured;
            }

            // La session avant l'état, et c'est le seul ordre qui tienne : `SessionStart`
            // est le premier événement d'une session, et il doit faire naître la machine
            // **sans** rien y déclarer. C'est elle, en existant, qui retire l'onglet à la
            // sonde — sinon un `claude` à son invite s'y montrerait `working`.
            let machine_event = match (session, declared) {
                (Some(SessionEvent::Opened), _) => AgentEvent::SessionOpened,
                (None, Some(declared)) => AgentEvent::Hook(declared),
                (None, None) => {
                    // Le cas du sixième hook : `SubagentStop` a nommé un enfant, et n'a rien
                    // à dire de l'onglet. Aucun état ne change, donc il n'y a rien à poster —
                    // et c'est très exactement le garde-fou de l'amendement, tenu par le
                    // chemin du code plutôt que par une intention.
                    return;
                }
            };
            let changed = tab
                .machine
                .get_or_insert_with(|| watching(clock, focused))
                .on(machine_event);

            // Les enfants après l'onglet, et à cause de lui : une ligne fille appartient à
            // la session qui l'a créée, et ne lui survit pas (spec §6.5). Les deux gestes
            // sont ici, et ce sont des **hooks** — aucun délai, aucun silence interprété :
            //
            // - une session qui s'ouvre. Ce que ses prédécesseurs avaient laissé au travail
            //   vient d'une session qui n'existe plus, et leur `SubagentStop` ne partira
            //   jamais : agent relancé, reprise, `/clear`, compactage ;
            // - une session qui **finit**, c'est-à-dire un onglet qui entre dans un état
            //   terminal. C'est la même lecture que celle qui ferme la session dans la
            //   machine, et c'est pourquoi elle lui est empruntée plutôt que réécrite.
            //
            // Ce qui n'est **pas** ici est aussi décidé : un parent qui passe `waiting` ne
            // dit rien de ses enfants. Un agent attend couramment ses sous-agents tout en
            // restant disponible pour l'utilisateur, et fermer leurs lignes sur un `Stop`
            // effacerait un travail qui tourne vraiment.
            if matches!(machine_event, AgentEvent::SessionOpened)
                || changed.is_some_and(has_finished)
            {
                tab.children.session_over(now);
            }

            // Un hook ne passe pas par la boucle de sonde : dater ici, et non à la passe
            // suivante, est ce qui fait que la durée affichée part du moment où l'agent a
            // parlé, et non de la prochaine passe.
            //
            // `entering` et non une date posée d'autorité : la machine annonce le changement
            // de **son** état, qui part d'`idle`, alors que l'onglet montrait peut-être déjà
            // le mot que le hook déclare — la sonde répondait pour lui. C'est le cas de tout
            // démarrage d'agent, et redater y ferait repartir le compteur de zéro.
            if let Some(state) = changed {
                tab.answered = Some(AgentStatus::entering(tab.answered, state, now));
            }
            interrupt(&event.tab_id, changed, focused, self.preferences.choices())
        };
        self.post(interruption);
    }

    /// Quel état pour cet onglet, compte tenu de ce que la sonde voit ?
    ///
    /// Appelée à chaque passe de la boucle d'ADR-0005 : c'est elle qui fait avancer le temps
    /// des machines, et c'est par son résultat — porté par le `TabInfo` du registre — que
    /// l'état atteint l'écran. Le frontend n'apprend jamais un état autrement.
    pub fn state(&self, tab_id: &str, seen: Presence) -> TabAgents {
        let (answer, interruption) = self.advance(tab_id, seen);
        self.post(interruption);
        answer
    }

    /// La passe de sonde elle-même : ce que l'onglet montre, et ce qu'elle vient
    /// d'apprendre qui mérite d'interrompre l'utilisateur.
    ///
    /// Découpée de [`Self::state`] pour une seule raison, et elle compte : le verrou des
    /// onglets meurt avec cette fonction, donc rien n'est posté en le tenant.
    fn advance(&self, tab_id: &str, seen: Presence) -> (TabAgents, Option<Notice>) {
        let now = self.clock.wall();
        let Ok(mut tabs) = self.tabs.lock() else {
            // Un superviseur empoisonné n'a plus de mémoire ; la sonde, elle, répond
            // toujours. Mieux vaut un onglet honnête qu'un onglet figé.
            //
            // Sans mémoire, il n'y a plus de date d'entrée : l'instant courant est le seul
            // repli honnête, et il fera repartir le compteur de zéro à chaque passe. Un
            // verrou empoisonné veut dire qu'un fil a paniqué ; un compteur qui bégaie est
            // le moindre des symptômes.
            return (
                TabAgents {
                    status: AgentStatus {
                        state: probed(seen),
                        since: now,
                    },
                    subagents: Vec::new(),
                    usage: None,
                },
                None,
            );
        };

        let focused = tabs.focused;
        let tab = tabs
            .live
            .entry(tab_id.to_owned())
            .or_insert_with(|| Tab::new(self.subagent_linger));
        let before = std::mem::replace(&mut tab.seen, seen);
        // Une passe aveugle ne raconte rien : elle ne doit pas non plus effacer le souvenir
        // de la précédente, sinon le retour du système passerait pour un lancement.
        if seen == Presence::Unknown {
            tab.seen = before;
        }

        let mut changed = None;
        let state = match tab.machine.as_mut() {
            // Un onglet où aucun agent n'a jamais parlé : c'est la sonde qui répond pour
            // lui, exactement comme au jalon J1.
            None => probed(seen),
            Some(machine) => {
                // La disparition : le shell a repris son terminal. On ne saura jamais avec
                // quel code — voir [`Exit::Unseen`]. C'est le **seul** front que la sonde
                // permet d'attribuer à l'agent ; le lancement, lui, n'est volontairement
                // émis nulle part ici (voir « Ce que la sonde n'a pas le droit
                // d'attribuer », en tête de fichier).
                //
                // C'est aussi le seul changement d'état qu'une passe de sonde peut
                // produire : le `tick` ci-dessous ne rend jamais qu'`idle`, qui
                // n'interrompt personne.
                if (before, seen) == (Presence::Program, Presence::Prompt) {
                    changed = machine.on(AgentEvent::ProcessVanished(Exit::Unseen));
                }

                machine.tick();
                machine.state()
            }
        };

        // La seconde moitié de la règle des lignes filles (voir [`Self::on_hook`]), sur le
        // seul front que la sonde ait le droit d'attribuer à l'agent : son processus a
        // disparu, donc sa session avec lui. C'est le cas nommé en premier par le ticket —
        // un agent tué n'enverra le `SubagentStop` d'aucun de ses enfants.
        if changed.is_some_and(has_finished) {
            tab.children.session_over(now);
        }

        // Une session ouverte retient la machine, même quand son état est retombé à `idle` :
        // c'est là qu'est un agent instrumenté qui n'a rien en vol. La rendre à la sonde
        // ferait remonter `working` sur sa seule présence à la passe suivante, et emporterait
        // au passage la jauge que son `SessionStart` venait d'apporter.
        let holds_a_session = tab
            .machine
            .as_ref()
            .is_some_and(AgentMachine::holds_a_session);
        if state == AgentState::Idle && !holds_a_session {
            // La ligne est redevenue une ligne shell : l'onglet n'est plus un agent, et
            // c'est de nouveau la sonde qui répond pour lui. Ses enfants partent avec lui —
            // un sous-agent sans agent au-dessus n'existe pas (ADR-0003 : c'est le même
            // processus, dans le même onglet).
            tab.machine = None;
            tab.children.clear();
            // La jauge part avec l'agent : un shell à son invite n'a pas de conversation, et
            // laisser la dernière mesure à l'écran ferait lire la place occupée par un agent
            // qui n'est plus là.
            tab.usage = None;
        }
        // Le seul endroit qui fasse vieillir les lignes filles, comme `tick` est le seul à
        // faire vieillir celle de l'onglet : la boucle de sonde passe déjà, et rien ne
        // réveille personne pour un compte à rebours.
        tab.children.tick(now);

        let status = AgentStatus::entering(tab.answered, state, now);
        tab.answered = Some(status);
        (
            TabAgents {
                status,
                subagents: tab.children.shown(),
                usage: tab.usage.clone(),
            },
            interrupt(tab_id, changed, focused, self.preferences.choices()),
        )
    }

    /// La fenêtre Ash a pris ou perdu le premier plan.
    ///
    /// Un niveau, poussé à toutes les machines : c'est lui qui décide si la ligne d'un agent
    /// fini a été **vue**, donc si ses trente secondes peuvent commencer (spec §6.4).
    ///
    /// **C'est la seule méthode qui touche les machines sans rien poster**, et elle en a le
    /// droit parce que le focus n'annonce jamais de changement d'état — un invariant de
    /// [`AgentMachine`], prouvé chez elle et non supposé ici. Sans lui, une interruption
    /// naîtrait sous le verrou, où la poser ferait attendre la boucle de sonde de tous les
    /// onglets.
    pub fn on_window_focus(&self, focused: bool) {
        let Ok(mut tabs) = self.tabs.lock() else {
            return;
        };
        tabs.focused = focused;
        for tab in tabs.live.values_mut() {
            if let Some(machine) = tab.machine.as_mut() {
                machine.on(AgentEvent::WindowFocus(focused));
            }
        }
    }

    /// Cet onglet n'existe plus.
    ///
    /// Un état d'agent ne survit pas à son onglet : rien n'est restauré, et la mémoire d'un
    /// onglet fermé ne doit pas répondre à un identifiant réattribué (ADR-0009).
    pub fn forget(&self, tab_id: &str) {
        if let Ok(mut tabs) = self.tabs.lock() {
            tabs.live.remove(tab_id);
        }
    }

    /// Pose l'interruption, s'il y en avait une à poser.
    ///
    /// Une ligne, et pas de règle : ce qui décide est [`super::notify::notice`], et ce qui
    /// dit qu'un état a **changé** est la machine. Ici il ne reste qu'à livrer.
    fn post(&self, interruption: Option<Notice>) {
        if let Some(interruption) = interruption {
            self.notifier.post(interruption);
        }
    }

    /// Le mot reçu sur le socket, traduit par le premier adaptateur qui le reconnaît.
    fn translate(&self, kind: &str) -> Option<Declared> {
        let raw = RawEvent::new(kind);
        self.adapters
            .iter()
            .find_map(|adapter| adapter.interpret(&raw))
            .and_then(Declared::of)
    }

    /// Ce que ce mot dit d'un **enfant**, par la seconde porte du trait.
    ///
    /// Symétrique de [`Self::translate`], et volontairement séparée d'elle : les deux lisent
    /// le même mot brut sans jamais se croiser, ce qui est la forme qu'exige l'amendement du
    /// 2026-08-13 à ADR-0007. Un adaptateur qui répondrait aux deux ne passerait pas la suite
    /// contractuelle.
    fn child_event(&self, kind: &str) -> Option<ChildEvent> {
        let raw = RawEvent::new(kind);
        self.adapters
            .iter()
            .find_map(|adapter| adapter.child_event(&raw))
    }

    /// Ce que ce mot dit d'une **session**, par la troisième porte du trait.
    ///
    /// Symétrique des deux autres, et séparée d'elles pour la même raison : les trois lisent
    /// le même mot brut sans jamais se croiser, et la suite contractuelle refuse un
    /// adaptateur qui répondrait à deux d'entre elles.
    fn session_event(&self, kind: &str) -> Option<SessionEvent> {
        let raw = RawEvent::new(kind);
        self.adapters
            .iter()
            .find_map(|adapter| adapter.session_event(&raw))
    }

    /// Ce que le transcript nommé par cette trame dit de la place consommée, ou rien.
    ///
    /// La règle, elle, est dans `usage.rs` : le superviseur ne fait que lui présenter ce
    /// qu'il détient — ses adaptateurs, son port, et le chemin qu'une trame a nommé.
    fn measure(&self, event: &EventFrame) -> Option<SessionUsage> {
        usage::measure(
            &self.adapters,
            self.transcripts.as_ref(),
            self.config.as_ref(),
            event.transcript_path.as_deref(),
            event.cwd.as_deref().map(std::path::Path::new),
        )
    }
}

/// Ce qu'une trame apprend des enfants de son onglet.
///
/// Rien du tout quand elle n'en nomme aucun — c'est le cas de l'écrasante majorité des
/// événements, et de **toutes** les trames que les cinq premiers hooks envoient depuis
/// l'agent principal. Une clé d'enfant vide a déjà été normalisée par le transport
/// (`wire.rs`) : il n'y a pas d'enfant anonyme à inventer ici.
fn note_child(
    children: &mut Subagents,
    event: &EventFrame,
    child: Option<ChildEvent>,
    now: UnixMillis,
) {
    let Some(agent_id) = event.agent_id.as_deref() else {
        return;
    };
    let agent_type = event.agent_type.as_deref();

    match child {
        // La fin, dite par le seul hook qui la connaisse.
        Some(ChildEvent::Ended) => children.ended(agent_id, agent_type, now),
        // La naissance n'a pas de hook : elle se lit sur le premier événement portant un
        // `agent_id` encore inconnu. C'est aussi ce qui garde la ligne d'un enfant vivante
        // tant qu'il emploie des outils.
        None => children.at_work(agent_id, agent_type, now),
    }
}

/// L'interruption que mérite un état qui vient de **changer**, ou rien.
///
/// `None` dès que rien n'a changé : c'est la porte étroite par laquelle la spec §8 passe, et
/// elle est étroite exprès — un état lu n'arrive jamais ici, donc un `waiting` qui dure ne
/// peut pas notifier deux fois.
fn interrupt(
    tab_id: &str,
    changed: Option<AgentState>,
    focused: bool,
    choices: NotificationChoices,
) -> Option<Notice> {
    notice(tab_id, changed?, focused, choices)
}

/// Une machine neuve, à qui l'on dit tout de suite si l'utilisateur regarde.
fn watching(clock: Arc<dyn Clock>, focused: bool) -> AgentMachine {
    let mut machine = AgentMachine::new(clock);
    machine.on(AgentEvent::WindowFocus(focused));
    machine
}

/// Ce que la sonde seule permet de dire d'un onglet où aucun agent n'a jamais parlé.
///
/// C'est le comportement du jalon J1, et il est inchangé : `vim`, `htop` ou un `make` qui
/// tourne donnent `working`, et un shell à son invite donne `idle`. Jamais `waiting`.
fn probed(seen: Presence) -> AgentState {
    match seen {
        Presence::Program => AgentState::Working,
        Presence::Prompt | Presence::Unknown => AgentState::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::adapters::{ClaudeCodeAdapter, GenericAdapter};
    use crate::features::agents::fakes::{
        FakeNotificationStore, FakeNotifier, FakeToolConfig, FakeTranscripts, ManualClock,
        FAKE_EPOCH,
    };
    use crate::features::agents::subagents::SUBAGENT_LINGER;
    use std::path::PathBuf;

    const TAB: &str = "01J0TAB";

    /// Ce qu'un scénario a sous la main : le superviseur, le temps, et l'écran de
    /// l'utilisateur.
    struct Assembled {
        supervisor: Supervisor,
        clock: Arc<ManualClock>,
        notifier: Arc<FakeNotifier>,
    }

    /// Test Data Builder : le superviseur tel que le composition root l'assemble.
    ///
    /// Les adaptateurs sont les **vrais** — c'est ce qui fait que ces tests parlent du
    /// vocabulaire que Claude Code écrira réellement dans le `settings.json`, et non d'un
    /// mot inventé pour le test.
    struct SupervisorBuilder {
        clock: Arc<ManualClock>,
        focused: bool,
        subagent_linger: Duration,
        /// Les trois interrupteurs de la spec §9, à leurs défauts sauf mention contraire.
        choices: NotificationChoices,
        /// Les transcripts que ce scénario **décrit** — vide par défaut, donc aucun chemin
        /// nommé par un hook ne mènera nulle part.
        transcripts: FakeTranscripts,
        /// La configuration de l'outil que ce scénario **décrit**.
        ///
        /// Elle nomme un modèle par défaut, parce que la plupart des scénarios ne parlent pas
        /// de la fenêtre et ont malgré tout besoin d'un pourcentage lisible. Aucun d'eux ne
        /// touche au `~/.claude` de qui lance les tests.
        config: FakeToolConfig,
        /// L'outil que cet onglet est censé faire tourner n'a pas d'adaptateur à lui.
        ///
        /// Faux par défaut : les scénarios courants assemblent les **vrais** adaptateurs, tels
        /// que le composition root les pose.
        only_generic: bool,
    }

    impl SupervisorBuilder {
        fn new() -> Self {
            Self {
                clock: ManualClock::new(),
                focused: false,
                subagent_linger: SUBAGENT_LINGER,
                choices: NotificationChoices::default(),
                transcripts: FakeTranscripts::new(),
                config: FakeToolConfig::new()
                    .homed_at(HOME)
                    .declaring_model(HOME_SETTINGS, "sonnet"),
                only_generic: false,
            }
        }

        /// L'utilisateur de ce scénario tourne avec ce modèle-là, déclaré dans son foyer.
        fn running_the_model(mut self, model: &str) -> Self {
            self.config = FakeToolConfig::new()
                .homed_at(HOME)
                .declaring_model(HOME_SETTINGS, model);
            self
        }

        /// Aucune source ne nomme de modèle : le cas par défaut d'une installation neuve.
        fn naming_no_model(mut self) -> Self {
            self.config = FakeToolConfig::new().homed_at(HOME);
            self
        }

        /// Le dépôt de cet onglet déclare son propre modèle, par-dessus celui du foyer.
        fn whose_repository_declares(mut self, model: &str) -> Self {
            self.config = self.config.declaring_model(REPO_SETTINGS, model);
            self
        }

        /// Un transcript existe à ce chemin, et il porte ce texte.
        fn holding_transcript(mut self, path: &str, tail: &str) -> Self {
            self.transcripts = self.transcripts.holding(path, tail);
            self
        }

        /// L'outil de l'onglet est inconnu : seul le socle d'ADR-0008 le sert.
        fn served_only_by_the_generic_adapter(mut self) -> Self {
            self.only_generic = true;
            self
        }

        /// Un utilisateur qui a mis l'un des trois interrupteurs dans cette position.
        fn notifying(mut self, state: AgentState, enabled: bool) -> Self {
            self.choices = self.choices.with(state, enabled);
            self
        }

        /// La fenêtre Ash est au premier plan — l'utilisateur regarde.
        fn watched(mut self) -> Self {
            self.focused = true;
            self
        }

        /// Le réglage de la spec §6.5, posé à une autre valeur que son défaut.
        fn keeping_finished_children_for(mut self, seconds: u64) -> Self {
            self.subagent_linger = Duration::from_secs(seconds);
            self
        }

        fn build(self) -> Assembled {
            let mut adapters: Vec<Arc<dyn Adapter>> = vec![Arc::new(GenericAdapter)];
            if !self.only_generic {
                adapters.push(Arc::new(ClaudeCodeAdapter::new(PathBuf::from(
                    "/Applications/Ash.app/Contents/MacOS/ash-event",
                ))));
            }
            let notifier = FakeNotifier::new();
            let supervisor = Supervisor::new(
                Arc::clone(&self.clock) as Arc<dyn Clock>,
                adapters,
                Arc::clone(&notifier) as Arc<dyn Notifier>,
                FakeNotificationStore::holding(self.choices),
                self.subagent_linger,
                Arc::new(self.transcripts) as Arc<dyn Transcripts>,
                Arc::new(self.config) as Arc<dyn ToolConfig>,
            );
            supervisor.on_window_focus(self.focused);
            Assembled {
                supervisor,
                clock: self.clock,
                notifier,
            }
        }
    }

    /// Ce qu'un hook envoie : le mot canonique de la spec §6.3, et l'onglet qui l'a hérité.
    fn hook(word: &str, tab: &str) -> EventFrame {
        EventFrame::new(word, tab)
    }

    /// Le même hook, déclenché **dans** un sous-agent : l'onglet est le même, l'enfant est
    /// nommé (ADR-0007, amendement du 2026-08-13).
    fn child_hook(word: &str, agent_id: &str, agent_type: &str) -> EventFrame {
        EventFrame::new(word, TAB).with_subagent(Some(agent_id), Some(agent_type))
    }

    /// Une passe de la boucle de sonde, telle que le registre la fait.
    fn sweep(supervisor: &Supervisor, seen: Presence) -> AgentState {
        sweep_status(supervisor, seen).state
    }

    /// La même passe, avec la date d'entrée — pour les seuls scénarios qui en parlent.
    fn sweep_status(supervisor: &Supervisor, seen: Presence) -> AgentStatus {
        supervisor.state(TAB, seen).status
    }

    /// La place consommée que la fiche de l'onglet porterait, à cette passe-ci.
    fn sweep_usage(supervisor: &Supervisor, seen: Presence) -> Option<SessionUsage> {
        supervisor.state(TAB, seen).usage
    }

    /// Le chemin qu'un hook de Claude Code met sur son entrée standard.
    const TRANSCRIPT: &str = "/Users/x/.claude/projects/ash/session.jsonl";

    /// Le foyer de l'utilisateur du scénario, et le fichier où il nomme son modèle.
    const HOME: &str = "/Users/x";
    const HOME_SETTINGS: &str = "/Users/x/.claude/settings.json";

    /// Le dépôt où l'agent de ces scénarios tourne, et le fichier qu'il y pose.
    const REPO: &str = "/dev/ash";
    const REPO_SETTINGS: &str = "/dev/ash/.claude/settings.local.json";

    /// Une queue de transcript qui déclare exactement `used` tokens.
    ///
    /// Un seul compteur renseigné : le partage entre entrée, cache et sortie est ce que les
    /// tests de l'adaptateur prouvent, et le redire ici ne dirait rien du superviseur.
    fn transcript_of(used: u64) -> String {
        format!(r#"{{"type":"assistant","message":{{"usage":{{"input_tokens":{used}}}}}}}"#)
    }

    /// Les lignes filles que la sidebar montrerait, réduites à ce qu'un `Then` lit.
    fn sweep_children(supervisor: &Supervisor, seen: Presence) -> Vec<(String, AgentState)> {
        supervisor
            .state(TAB, seen)
            .subagents
            .into_iter()
            .map(|child| (child.agent_type.unwrap_or_default(), child.state))
            .collect()
    }

    #[test]
    fn given_a_tab_where_no_agent_ever_spoke_when_a_program_takes_the_foreground_then_it_works_and_never_waits(
    ) {
        // Given — `vim`, `htop`, un `make` : la sonde ne sait pas les nommer autrement que
        // « quelque chose tourne », et c'est tout ce qu'elle a le droit d'en dire
        // (ADR-0007, précision du 2026-08-11).
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();

        // When
        let at_the_prompt = sweep(&supervisor, Presence::Prompt);
        let running = sweep(&supervisor, Presence::Program);

        // Then
        assert_eq!(at_the_prompt, AgentState::Idle);
        assert_eq!(running, AgentState::Working);
    }

    #[test]
    fn given_an_instrumented_agent_that_just_opened_when_the_probe_sees_it_hold_the_foreground_then_the_tab_is_idle(
    ) {
        // Given — `claude` vient d'être tapé, ses hooks sont posés, aucun prompt n'est parti.
        // La sonde voit bien un programme tenir l'avant-plan ; jusqu'ici, c'est ce qui
        // affichait `working` et faisait tourner le glyphe pour un agent qui ne fait rien
        // (ADR-0007, précision du 2026-08-24).
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();

        // When — le hook du démarrage, puis dix passes de sonde qui voient le processus
        supervisor.on_hook(&hook("session-start", TAB));
        let shown: Vec<AgentState> = (0..10)
            .map(|_| sweep(&supervisor, Presence::Program))
            .collect();

        // Then — et pas une seule passe à `working` : la machine existe désormais pour cet
        // onglet, donc la présence n'y répond plus.
        assert_eq!(shown, vec![AgentState::Idle; 10]);
    }

    #[test]
    fn given_a_session_that_just_opened_when_the_user_sends_a_prompt_then_the_tab_works_and_waits_as_before(
    ) {
        // Given — le tour complet, depuis l'ouverture. La tranche ne change **que** le
        // moment où rien n'est en vol : les deux flèches du diagramme §6.2 restent les leurs.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        supervisor.on_hook(&hook("session-start", TAB));

        // When
        supervisor.on_hook(&hook("working", TAB));
        let prompted = sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("waiting", TAB));
        let ended = sweep(&supervisor, Presence::Program);

        // Then
        assert_eq!(prompted, AgentState::Working);
        assert_eq!(ended, AgentState::Waiting);
    }

    #[test]
    fn given_a_tool_without_hooks_when_the_probe_sees_it_hold_the_foreground_then_it_still_shows_working(
    ) {
        // Given — un outil reconnu mais **non instrumenté** : le socle d'ADR-0008 le sert, il
        // n'envoie aucun verbe de session, donc aucune machine ne naît dans son onglet. C'est
        // la moitié de la règle qui ne bouge pas — `working` a deux producteurs (spec §6.2),
        // et un outil sans hooks garde le premier.
        let Assembled { supervisor, .. } = SupervisorBuilder::new()
            .served_only_by_the_generic_adapter()
            .build();

        // When
        let running = sweep(&supervisor, Presence::Program);

        // Then
        assert_eq!(running, AgentState::Working);
    }

    #[test]
    fn given_a_resumed_session_when_its_opening_hook_names_the_transcript_then_the_gauge_is_there_before_any_prompt(
    ) {
        // Given — `claude --continue` sur une conversation déjà à moitié pleine. Jusqu'ici la
        // jauge n'existait qu'à partir du premier hook d'état, donc pas avant le premier
        // prompt : l'utilisateur reprenait une session sans savoir la place qu'elle occupait.
        let Assembled { supervisor, .. } = SupervisorBuilder::new()
            .running_the_model("sonnet")
            .holding_transcript(TRANSCRIPT, &transcript_of(80_000))
            .build();

        // When — le seul événement de la session, et dix passes de sonde derrière lui
        supervisor.on_hook(&hook("session-start", TAB).with_transcript(Some(TRANSCRIPT)));
        let measured: Vec<Option<u64>> = (0..10)
            .map(|_| sweep_usage(&supervisor, Presence::Program).map(|usage| usage.used_tokens))
            .collect();

        // Then — et la mesure ne disparaît pas à la passe suivante : l'onglet reste un agent
        // tant que sa session est ouverte, donc la jauge ne part pas avec lui.
        assert_eq!(measured, vec![Some(80_000); 10]);
    }

    #[test]
    fn given_a_brand_new_session_whose_transcript_holds_no_turn_when_it_opens_then_no_gauge_is_shown(
    ) {
        // Given — une session neuve : le fichier existe, mais aucun tour d'assistant n'y a
        // encore été écrit. Une jauge à 0 % dirait « mesuré, et vide » là où il n'y a rien à
        // mesurer, et rien à l'écran ne distinguerait les deux.
        let Assembled { supervisor, .. } = SupervisorBuilder::new()
            .holding_transcript(TRANSCRIPT, r#"{"type":"user","message":{"role":"user"}}"#)
            .build();

        // When
        supervisor.on_hook(&hook("session-start", TAB).with_transcript(Some(TRANSCRIPT)));

        // Then
        assert_eq!(sweep_usage(&supervisor, Presence::Program), None);
    }

    #[test]
    fn given_an_open_session_with_nothing_in_flight_when_the_user_quits_it_then_the_tab_goes_back_to_the_probe(
    ) {
        // Given — on ouvre `claude`, on ne lui demande rien, on le quitte, puis on lance
        // autre chose dans le même onglet. Sans la fermeture de la session, l'onglet
        // resterait accroché à sa machine et le `make` qui suit n'y montrerait plus rien.
        let Assembled {
            supervisor,
            notifier,
            ..
        } = SupervisorBuilder::new().build();
        supervisor.on_hook(&hook("session-start", TAB));
        sweep(&supervisor, Presence::Program);

        // When
        let quit = sweep(&supervisor, Presence::Prompt);
        let something_else = sweep(&supervisor, Presence::Program);

        // Then — et aucun échec inventé, donc aucune bannière : rien n'était en vol.
        assert_eq!(quit, AgentState::Idle);
        assert_eq!(something_else, AgentState::Working);
        assert_eq!(notifier.posted(), Vec::new());
    }

    #[test]
    fn given_a_program_that_ran_without_a_single_hook_when_it_leaves_the_foreground_then_the_tab_is_a_shell_row_again(
    ) {
        // Given — quitter `vim` n'est pas la fin d'un agent. Annoncer `done` ici ferait
        // clignoter la sidebar à chaque commande, et rendrait l'état inutilisable pour ce
        // qu'il sert : reconnaître un agent qui a fini.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().watched().build();
        sweep(&supervisor, Presence::Program);

        // When
        let back_at_the_prompt = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(back_at_the_prompt, AgentState::Idle);
    }

    #[test]
    fn given_a_hook_saying_the_agent_asks_a_question_when_the_probe_keeps_seeing_it_running_then_the_tab_stays_waiting(
    ) {
        // Given — le conflit qui décide de toute cette tranche : la sonde voit `claude` au
        // premier plan et le croirait au travail, alors qu'il attend une réponse. Le hook
        // fait autorité (ADR-0007) ; sans ça, le seul état qui mérite d'interrompre
        // l'utilisateur serait écrasé trois fois par seconde.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);

        // When
        supervisor.on_hook(&hook("waiting", TAB));
        let announced: Vec<AgentState> = (0..10)
            .map(|_| sweep(&supervisor, Presence::Program))
            .collect();

        // Then
        assert_eq!(announced, vec![AgentState::Waiting; 10]);
    }

    #[test]
    fn given_an_agent_that_declared_its_end_when_its_process_disappears_then_the_tab_still_says_done(
    ) {
        // Given — la séquence réelle d'une fin propre : `SessionEnd` part, *puis* le
        // processus quitte l'avant-plan. La disparition ne doit rien retrancher à ce que le
        // hook a dit, sinon toute fin normale s'afficherait en échec.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("done", TAB));

        // When
        let after_the_process_left = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(after_the_process_left, AgentState::Done);
    }

    #[test]
    fn given_an_agent_at_work_when_its_process_disappears_without_declaring_its_end_then_the_tab_shows_an_error(
    ) {
        // Given — plantage, `kill`, `Ctrl-C` en plein travail : un agent instrumenté dit sa
        // fin lui-même, donc partir sans l'avoir dite est anormal. Ash n'aura jamais son
        // code de sortie — il n'a pas lancé le processus (ADR-0006) — et c'est le seul
        // endroit du produit où cette absence se tranche. Voir [`Exit::Unseen`].
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("working", TAB));

        // When
        let after_the_crash = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(after_the_crash, AgentState::Error);
    }

    #[test]
    fn given_a_finished_agent_line_when_the_user_runs_an_ordinary_command_before_it_expires_then_no_failure_is_ever_announced(
    ) {
        // Given — la séquence la plus banale du produit : l'agent finit, et l'utilisateur
        // enchaîne sur un `cargo test` dans la seconde qui suit. La sonde ne rend qu'une
        // présence : rien ne distingue ce programme de l'agent (ADR-0006). Le prendre pour
        // lui annoncerait un échec sur une commande qui a réussi — et pour bien plus longtemps
        // que trente secondes, puisqu'un état actif n'expire jamais.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().watched().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("done", TAB));
        sweep(&supervisor, Presence::Prompt);

        // When — la commande prend l'avant-plan, tourne une minute, et se termine bien
        let while_it_runs = sweep(&supervisor, Presence::Program);
        clock.advance(60);
        let once_it_is_over = sweep(&supervisor, Presence::Prompt);

        // Then — la ligne `done` a vécu, puis l'onglet est redevenu une ligne shell
        assert_eq!(while_it_runs, AgentState::Done);
        assert_eq!(once_it_is_over, AgentState::Idle);
    }

    #[test]
    fn given_an_agent_that_keeps_working_when_the_loop_sweeps_for_a_quarter_of_an_hour_then_its_entry_date_never_moves(
    ) {
        // Given — c'est le piège que cette tranche existe pour éviter. La boucle passe trois
        // fois par seconde ; si chaque passe redatait l'état, la fiche de l'onglet changerait
        // à chaque passe, et l'event `ash://tab-changed` deviendrait un flux continu — un
        // rendu complet de la sidebar par seconde, pour animer un compteur.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("working", TAB));
        let entered = sweep_status(&supervisor, Presence::Program);

        // When — un quart d'heure de travail, sans qu'aucun hook ne parle
        let dates: Vec<UnixMillis> = (0..900)
            .map(|_| {
                clock.advance(1);
                sweep_status(&supervisor, Presence::Program).since
            })
            .collect();

        // Then — la date d'entrée est celle du hook, et elle n'a pas bougé d'une milliseconde
        assert_eq!(entered.since, FAKE_EPOCH);
        assert_eq!(dates, vec![FAKE_EPOCH; 900]);
    }

    #[test]
    fn given_a_tab_that_changes_state_when_it_is_swept_again_then_the_date_moves_to_the_moment_it_changed(
    ) {
        // Given — l'autre moitié : une date qui ne bougerait jamais afficherait `waiting ·
        // 15m22s` sur un agent qui vient à peine de poser sa question. C'est le hook qui date,
        // pas la passe de sonde qui le constate.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("working", TAB));

        // When — dix minutes de travail, puis une question
        clock.advance(600);
        let while_working = sweep_status(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("waiting", TAB));
        clock.advance(5);
        let once_it_asks = sweep_status(&supervisor, Presence::Program);

        // Then
        assert_eq!(
            (while_working.state, while_working.since),
            (AgentState::Working, FAKE_EPOCH)
        );
        assert_eq!(
            (once_it_asks.state, once_it_asks.since),
            (AgentState::Waiting, FAKE_EPOCH + 600_000)
        );
    }

    #[test]
    fn given_a_tab_where_no_agent_ever_spoke_when_a_program_takes_the_foreground_then_that_state_is_dated_too(
    ) {
        // Given — la ligne de statut date ce qu'elle montre, et elle montre aussi les onglets
        // où aucun agent n'a jamais parlé : `vim`, un `make` qui tourne. Ne dater que les
        // états venus d'un hook ferait apparaître et disparaître le compteur d'un onglet à
        // l'autre, sans que rien ne l'explique à l'écran.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Prompt);

        // When — un programme prend l'avant-plan une minute après l'ouverture
        clock.advance(60);
        let started = sweep_status(&supervisor, Presence::Program);
        clock.advance(30);
        let still_running = sweep_status(&supervisor, Presence::Program);

        // Then
        assert_eq!(started.since, FAKE_EPOCH + 60_000);
        assert_eq!(
            (still_running.state, still_running.since),
            (AgentState::Working, FAKE_EPOCH + 60_000)
        );
    }

    #[test]
    fn given_a_tab_the_probe_already_shows_working_when_the_agent_says_the_same_word_then_the_counter_does_not_start_over(
    ) {
        // Given — la séquence de **tout** démarrage d'agent, et elle se voit à l'écran :
        // la sonde voit `claude` prendre l'avant-plan et l'onglet affiche `working` ; le
        // premier hook n'arrive qu'ensuite, au premier outil employé. Le verdict montré n'a
        // pas changé entre les deux — c'est le même mot —, donc le compteur ne doit pas
        // repartir de zéro sous les yeux de l'utilisateur. La date suit le verdict, pas la
        // source qui le produit.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().build();
        let started = sweep_status(&supervisor, Presence::Program);

        // When — l'agent emploie son premier outil dix secondes plus tard
        clock.advance(10);
        supervisor.on_hook(&hook("working", TAB));
        let once_it_spoke = sweep_status(&supervisor, Presence::Program);

        // Then
        assert_eq!(
            (started.state, started.since),
            (AgentState::Working, FAKE_EPOCH)
        );
        assert_eq!(
            (once_it_spoke.state, once_it_spoke.since),
            (AgentState::Working, FAKE_EPOCH)
        );
    }

    #[test]
    fn given_a_subagent_that_finishes_when_its_stop_hook_arrives_then_the_tab_state_is_left_alone()
    {
        // Given — le garde-fou n°1 de l'amendement du 2026-08-13, vu du superviseur : un
        // enfant qui finit ne rend pas `claude` disponible. Si `subagent-stop` atteignait la
        // machine, l'onglet afficherait `done` pendant que l'agent principal continue de
        // travailler — et la ligne s'effacerait toute seule trente secondes plus tard.
        let Assembled {
            supervisor,
            notifier,
            ..
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("working", TAB));
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));

        // When
        supervisor.on_hook(&child_hook("subagent-stop", "agent-7", "explore"));

        // Then — l'onglet travaille toujours, et rien n'a interrompu l'utilisateur
        assert_eq!(sweep(&supervisor, Presence::Program), AgentState::Working);
        assert_eq!(notifier.titles(), Vec::<String>::new());
    }

    #[test]
    fn given_two_subagents_running_at_once_when_one_of_them_stops_then_each_row_carries_its_own_state(
    ) {
        // Given — plusieurs sous-agents en parallèle, c'est le cas courant d'un `Task` lancé
        // en éventail. `ASH_TAB_ID` est identique pour les deux et pour leur parent : seul
        // `agent_id` peut apparier un `SubagentStop` à la bonne ligne.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));
        supervisor.on_hook(&child_hook("working", "agent-8", "code-reviewer"));

        // When
        supervisor.on_hook(&child_hook("subagent-stop", "agent-8", "code-reviewer"));

        // Then
        assert_eq!(
            sweep_children(&supervisor, Presence::Program),
            vec![
                ("explore".to_owned(), AgentState::Working),
                ("code-reviewer".to_owned(), AgentState::Done),
            ]
        );
    }

    #[test]
    fn given_a_finished_subagent_row_when_the_configured_delay_passes_then_it_disappears_from_the_tab(
    ) {
        // Given — la durée est un **réglage** (spec §6.5) : ce scénario en pose une autre que
        // les dix secondes par défaut, pour prouver que c'est bien elle qui décide et non un
        // nombre écrit au milieu de la règle.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new()
            .keeping_finished_children_for(3)
            .build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));
        supervisor.on_hook(&child_hook("subagent-stop", "agent-7", "explore"));

        // When
        clock.advance(2);
        let still_shown = sweep_children(&supervisor, Presence::Program);
        clock.advance(1);
        let expired = sweep_children(&supervisor, Presence::Program);

        // Then
        assert_eq!(still_shown, vec![("explore".to_owned(), AgentState::Done)]);
        assert_eq!(expired, vec![]);
    }

    #[test]
    fn given_two_working_children_when_a_new_session_opens_in_their_tab_then_both_finish_and_leave_ten_seconds_later(
    ) {
        // Given — le cas rapporté : `claude` relancé, repris, `/clear`, compacté. Les enfants
        // de la session d'avant n'enverront jamais leur `SubagentStop`, et leurs lignes
        // restaient `working` indéfiniment — `17h44m` sous un parent qui n'a plus rien en vol.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().build();
        supervisor.on_hook(&hook("session-start", TAB));
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));
        supervisor.on_hook(&child_hook("working", "agent-8", "qa"));

        // When — la nouvelle session s'ouvre
        clock.advance(3_600);
        supervisor.on_hook(&hook("session-start", TAB));
        let once_it_reopened = sweep_children(&supervisor, Presence::Program);
        clock.advance(10);
        let ten_seconds_later = sweep_children(&supervisor, Presence::Program);

        // Then — `done`, puis la même fin que n'importe quel enfant. Pas `error` : Ash ne sait
        // pas mieux si l'enfant a réussi que dans le cas annoncé.
        assert_eq!(
            once_it_reopened,
            vec![
                ("explore".to_owned(), AgentState::Done),
                ("qa".to_owned(), AgentState::Done),
            ]
        );
        assert_eq!(ten_seconds_later, vec![]);
    }

    #[test]
    fn given_a_tab_whose_session_just_closed_its_children_when_new_ones_appear_then_they_are_new_rows(
    ) {
        // Given — un `agent_id` ne distingue que des frères dans un onglet : la session
        // suivante peut renommer un enfant `agent-7`. Le voir reprendre la ligne du mort
        // afficherait un sous-agent né il y a une heure.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().build();
        supervisor.on_hook(&hook("session-start", TAB));
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));
        clock.advance(3_600);
        supervisor.on_hook(&hook("session-start", TAB));

        // When — la session neuve fait naître un enfant qui porte le même identifiant
        supervisor.on_hook(&child_hook("working", "agent-7", "code-reviewer"));

        // Then
        assert_eq!(
            sweep_children(&supervisor, Presence::Program),
            vec![
                ("explore".to_owned(), AgentState::Done),
                ("code-reviewer".to_owned(), AgentState::Working),
            ]
        );
    }

    #[test]
    fn given_a_working_child_when_its_session_ends_then_its_row_finishes_instead_of_being_wiped_with_the_tab(
    ) {
        // Given — avant cette correction, la ligne restait `working` pendant les trente
        // secondes de la ligne `done` de l'onglet, puis disparaissait d'un coup au retour à
        // `idle` : un enfant montré en train de travailler pendant une demi-minute alors que
        // son agent était parti, puis effacé sans jamais avoir été vu finir.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().watched().build();
        supervisor.on_hook(&hook("session-start", TAB));
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));

        // When — `SessionEnd`, que l'adaptateur traduit en `done` pour l'onglet
        supervisor.on_hook(&hook("done", TAB));
        let once_the_session_ended = supervisor.state(TAB, Presence::Program);
        clock.advance(10);
        let ten_seconds_later = sweep_children(&supervisor, Presence::Program);

        // Then — l'onglet garde ses trente secondes, l'enfant a les siennes : dix
        assert_eq!(once_the_session_ended.status.state, AgentState::Done);
        assert_eq!(
            once_the_session_ended
                .subagents
                .into_iter()
                .map(|child| (child.agent_type.unwrap_or_default(), child.state))
                .collect::<Vec<_>>(),
            vec![("explore".to_owned(), AgentState::Done)]
        );
        assert_eq!(ten_seconds_later, vec![]);
        // Et l'onglet n'est pas parti avec lui : il lui reste vingt secondes à être lu.
        assert_eq!(sweep(&supervisor, Presence::Program), AgentState::Done);
    }

    #[test]
    fn given_a_child_at_work_when_its_parent_starts_waiting_for_the_user_then_the_child_row_never_moves(
    ) {
        // Given — la correction explicite de l'utilisateur, et la moitié de la règle qu'il
        // serait le plus facile de casser : un agent attend couramment ses sous-agents *tout
        // en restant disponible*. Fermer ses lignes filles sur un `Stop` — ou sur l'âge de
        // l'enfant — effacerait un travail qui tourne vraiment.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().build();
        supervisor.on_hook(&hook("session-start", TAB));
        supervisor.on_hook(&hook("working", TAB));
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));

        // When — le parent rend la main, et six heures passent sans que l'enfant reparle
        supervisor.on_hook(&hook("waiting", TAB));
        clock.advance(6 * 3_600);
        let still_running = supervisor.state(TAB, Presence::Program);

        // Then — aucun plafond d'âge : la durée d'un enfant ne conclut rien (spec §6.4)
        assert_eq!(still_running.status.state, AgentState::Waiting);
        assert_eq!(
            sweep_children(&supervisor, Presence::Program),
            vec![("explore".to_owned(), AgentState::Working)]
        );
    }

    #[test]
    fn given_a_child_at_work_when_the_agent_that_ran_it_vanishes_then_its_row_finishes_too() {
        // Given — le premier cas nommé par le ticket : l'agent est tué. Aucun `SubagentStop`
        // ne partira, et un sous-agent n'a pas de processus à interroger — c'est le même
        // `claude`, dans le même onglet (ADR-0003). La disparition du parent est donc tout ce
        // qu'Ash saura jamais de la fin de ses enfants.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        supervisor.on_hook(&hook("session-start", TAB));
        supervisor.on_hook(&hook("working", TAB));
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));
        sweep(&supervisor, Presence::Program);

        // When — le shell reprend son terminal
        let orphaned = supervisor.state(TAB, Presence::Prompt);

        // Then — l'onglet conclut selon la spec §6.4, et l'enfant finit avec lui
        assert_eq!(orphaned.status.state, AgentState::Error);
        assert_eq!(
            orphaned
                .subagents
                .into_iter()
                .map(|child| child.state)
                .collect::<Vec<_>>(),
            vec![AgentState::Done]
        );
    }

    #[test]
    fn given_a_tab_whose_agent_works_with_children_when_the_loop_sweeps_for_a_quarter_of_an_hour_then_nothing_it_shows_ever_changes(
    ) {
        // Given — le piège de #98, à l'échelle des lignes filles : la fiche d'un onglet est
        // comparée **entière** pour décider s'il faut émettre. Si une ligne fille portait sa
        // durée plutôt que sa date d'entrée, chaque onglet qui porte un enfant changerait à
        // chaque seconde, et `ash://tab-changed` deviendrait un flux continu — un rendu
        // complet de la sidebar par seconde, pour animer un compteur qui se calcule à
        // l'affichage.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("working", TAB));
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));
        let first = supervisor.state(TAB, Presence::Program);

        // When — un quart d'heure de boucle, sans qu'aucun hook ne parle
        let answers: Vec<TabAgents> = (0..900)
            .map(|_| {
                clock.advance(1);
                supervisor.state(TAB, Presence::Program)
            })
            .collect();

        // Then — la réponse est identique, au champ près, d'un bout à l'autre
        assert_eq!(first.subagents.len(), 1);
        assert_eq!(answers, vec![first; 900]);
    }

    #[test]
    fn given_a_tool_that_reports_no_subagent_when_its_hooks_arrive_then_the_tab_never_grows_a_child_row(
    ) {
        // Given — `SubagentSupport::None` doit se voir : un outil qui n'expose pas ses
        // sous-tâches ne produit **aucune** ligne fille, et rien ne doit suggérer qu'il en
        // manque (spec §6.5). Ses hooks n'emportent pas d'`agent_id`, et c'est cette absence
        // qui décide — pas une devinette sur le nom de l'outil.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);

        // When
        for word in ["working", "waiting", "done"] {
            supervisor.on_hook(&hook(word, TAB));
        }

        // Then
        assert_eq!(sweep_children(&supervisor, Presence::Program), vec![]);
    }

    #[test]
    fn given_an_agent_whose_children_are_still_shown_when_the_tab_becomes_a_shell_row_again_then_nothing_remains_under_it(
    ) {
        // Given — un sous-agent n'a pas de processus à lui : c'est le même `claude`, dans le
        // même onglet (ADR-0003). Quand l'onglet redevient une ligne shell, laisser des
        // lignes filles montrerait des enfants sans parent, que rien ne viendrait jamais
        // effacer.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().watched().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&child_hook("working", "agent-7", "explore"));
        supervisor.on_hook(&hook("done", TAB));

        // When — la ligne `done` vit ses trente secondes, puis l'onglet redevient un shell
        sweep(&supervisor, Presence::Prompt);
        clock.advance(30);
        let once_it_is_a_shell_row = supervisor.state(TAB, Presence::Prompt);

        // Then
        assert_eq!(once_it_is_a_shell_row.status.state, AgentState::Idle);
        assert_eq!(once_it_is_a_shell_row.subagents, vec![]);
    }

    #[test]
    fn given_a_word_no_adapter_understands_when_it_arrives_from_the_socket_then_the_tab_keeps_its_state(
    ) {
        // Given — `Stop` est un vrai nom de hook de Claude Code, `idle` un vrai état du
        // produit : ni l'un ni l'autre n'est un mot que le bloc d'Ash écrit (spec §6.3).
        // Les accepter reviendrait à deviner, et un `waiting` deviné est exactement ce
        // qu'ADR-0007 refuse.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);

        // When
        for word in ["Stop", "Notification", "idle", "", "waiting "] {
            supervisor.on_hook(&hook(word, TAB));
        }

        // Then — la sonde continue de répondre, comme si rien n'était arrivé
        assert_eq!(sweep(&supervisor, Presence::Program), AgentState::Working);
        assert_eq!(sweep(&supervisor, Presence::Prompt), AgentState::Idle);
    }

    #[test]
    fn given_two_claude_accounts_in_two_tabs_when_one_of_them_asks_a_question_then_only_its_own_tab_waits(
    ) {
        // Given — `claude` et `claude-perso`, deux dossiers de configuration, deux blocs de
        // hooks, et un seul socket. Ce qui les sépare est `ASH_TAB_ID`, et rien d'autre :
        // ni le `cwd`, ni un horodatage, ni le pid (ADR-0007).
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        supervisor.state("01J0PRO", Presence::Program);
        supervisor.state("01J0PERSO", Presence::Program);

        // When
        supervisor.on_hook(&hook("waiting", "01J0PERSO"));

        // Then
        assert_eq!(
            supervisor.state("01J0PRO", Presence::Program).status.state,
            AgentState::Working
        );
        assert_eq!(
            supervisor
                .state("01J0PERSO", Presence::Program)
                .status
                .state,
            AgentState::Waiting
        );
    }

    #[test]
    fn given_a_done_line_the_user_has_seen_when_thirty_seconds_of_sweeps_pass_then_the_tab_becomes_a_shell_row_again(
    ) {
        // Given — la règle des 30 s de la spec §6.4 est prouvée par la machine ; ce qui se
        // prouve ici est qu'elle est **branchée** : personne d'autre que la boucle de sonde
        // ne fait avancer le temps, et une ligne `done` que rien ne rafraîchirait resterait
        // pour toujours.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().watched().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("done", TAB));
        sweep(&supervisor, Presence::Prompt);

        // When
        clock.advance(29);
        let still_shown = sweep(&supervisor, Presence::Prompt);
        clock.advance(1);
        let expired = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(still_shown, AgentState::Done);
        assert_eq!(expired, AgentState::Idle);
    }

    #[test]
    fn given_a_done_line_produced_while_ash_was_hidden_when_the_window_comes_back_then_the_thirty_seconds_start_there(
    ) {
        // Given — Ash derrière l'éditeur, l'agent finit tout seul. Effacer la ligne au bout
        // de 30 s d'absence ferait disparaître l'information avant que personne ne l'ait
        // lue : un agent aurait travaillé pour rien.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("done", TAB));
        clock.advance(3600);

        // When
        let while_hidden = sweep(&supervisor, Presence::Prompt);
        supervisor.on_window_focus(true);
        clock.advance(30);
        let after_being_seen = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(while_hidden, AgentState::Done);
        assert_eq!(after_being_seen, AgentState::Idle);
    }

    #[test]
    fn given_an_agent_whose_tab_is_closed_when_a_new_tab_takes_its_place_then_nothing_of_it_remains(
    ) {
        // Given — rien n'est restauré (ADR-0009), et surtout pas dans un onglet qui n'est
        // plus celui-là. Un état qui survivrait à son onglet serait un agent fantôme dans la
        // sidebar, sans processus derrière lui.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("waiting", TAB));

        // When
        supervisor.forget(TAB);

        // Then
        assert_eq!(sweep(&supervisor, Presence::Program), AgentState::Working);
    }

    #[test]
    fn given_a_tab_the_probe_can_no_longer_observe_when_the_loop_sweeps_then_its_declared_state_survives(
    ) {
        // Given — un appel système qui échoue, un processus qui se dérobe entre deux
        // passes : c'est courant, et ça ne dit rien de l'agent. Retomber à `idle` là-dessus
        // ferait clignoter la sidebar au gré de la charge de la machine.
        let Assembled { supervisor, .. } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("waiting", TAB));

        // When
        let blind = sweep(&supervisor, Presence::Unknown);
        let seeing_again = sweep(&supervisor, Presence::Program);

        // Then — et la reprise de la sonde n'est pas prise pour un nouveau lancement, qui
        // aurait écrasé le `waiting` par un `working`
        assert_eq!(blind, AgentState::Waiting);
        assert_eq!(seeing_again, AgentState::Waiting);
    }

    #[test]
    fn given_a_waiting_agent_whose_state_persists_when_the_probe_keeps_sweeping_then_the_user_is_interrupted_exactly_once(
    ) {
        // Given — l'état est **lu** trois fois par seconde par la boucle d'ADR-0005. Une
        // notification accrochée à la lecture en poserait trois par seconde, et la première
        // chose qu'un utilisateur ferait serait de couper les notifications d'Ash — ce qui
        // détruirait le seul critère de sortie du jalon (voir un `waiting` en moins de 10 s).
        let Assembled {
            supervisor,
            notifier,
            ..
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);

        // When — trente secondes de boucle, et un seul `waiting`
        supervisor.on_hook(&hook("waiting", TAB));
        for _ in 0..100 {
            sweep(&supervisor, Presence::Program);
        }

        // Then
        assert_eq!(notifier.titles(), vec!["an agent is waiting".to_owned()]);
    }

    #[test]
    fn given_a_user_who_turned_waiting_off_when_an_agent_asks_a_question_then_nothing_reaches_his_screen(
    ) {
        // Given — l'interrupteur de la spec §9 doit couper la bannière **là où elle part**.
        // Ce scénario est le seul qui le prouve de bout en bout : le superviseur est le seul
        // producteur de bannières du produit, et il ne demande son avis à personne au moment
        // de poster — une bannière sort précisément quand la fenêtre n'est pas là pour
        // filtrer quoi que ce soit
        let Assembled {
            supervisor,
            notifier,
            ..
        } = SupervisorBuilder::new()
            .notifying(AgentState::Waiting, false)
            .build();
        sweep(&supervisor, Presence::Program);

        // When — l'agent pose sa question, et la boucle continue de passer
        supervisor.on_hook(&hook("waiting", TAB));
        let asking = sweep(&supervisor, Presence::Program);

        // Then — la sidebar continue de dire `waiting` : l'interrupteur coupe la bannière,
        // pas l'état
        assert_eq!(asking, AgentState::Waiting);
        assert_eq!(notifier.titles(), Vec::<String>::new());
    }

    #[test]
    fn given_a_user_who_turned_done_on_when_an_agent_declares_the_end_of_its_work_then_a_banner_reaches_him(
    ) {
        // Given — le symétrique, et la seule chose qui rende le troisième interrupteur autre
        // qu'un bouton décoratif : `done` ne notifie pas en v1 (spec §8), et l'allumer est le
        // seul moyen de changer ça
        let Assembled {
            supervisor,
            notifier,
            ..
        } = SupervisorBuilder::new()
            .notifying(AgentState::Done, true)
            .build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("working", TAB));

        // When
        supervisor.on_hook(&hook("done", TAB));

        // Then
        assert_eq!(notifier.titles(), vec!["an agent finished".to_owned()]);
    }

    #[test]
    fn given_ash_in_the_background_when_an_agent_asks_a_question_then_ash_never_brings_itself_forward(
    ) {
        // Given — le troisième critère de la spec §8 est une **interdiction** : jamais de
        // sélection automatique ni de vol de focus (ADR-0010, ADR-0015). Elle s'observe
        // ici, et pas seulement dans la forme du port : si le superviseur se croyait
        // regardé après avoir notifié, la ligne d'un agent fini partirait son compte à
        // rebours de trente secondes sans que personne ne l'ait vue — et l'information
        // disparaîtrait de l'écran avant que l'utilisateur ne revienne.
        //
        // **Depuis que le clic se capte, l'interdiction porte plus loin** : la bannière
        // emporte l'onglet qu'un clic sélectionnerait, et c'est le clic — un geste de
        // l'utilisateur — qui doit être seul à le faire. Poser la bannière ne sélectionne
        // rien, et ne peut rien sélectionner : [`Notifier::post`] ne rend rien et ne reçoit
        // aucune poignée d'application.
        let Assembled {
            supervisor,
            clock,
            notifier,
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);

        // When — l'agent interrompt l'utilisateur, puis termine son travail
        supervisor.on_hook(&hook("waiting", TAB));
        supervisor.on_hook(&hook("done", TAB));
        clock.advance(3600);
        let an_hour_later = sweep(&supervisor, Presence::Prompt);

        // Then — la bannière est bien partie, en nommant l'onglet que le clic ramènera, et
        // la fenêtre n'a pas pris le premier plan pour autant
        assert_eq!(notifier.titles(), vec!["an agent is waiting".to_owned()]);
        assert_eq!(
            notifier
                .posted()
                .into_iter()
                .map(|notice| notice.tab_id)
                .collect::<Vec<_>>(),
            vec![TAB.to_owned()]
        );
        assert_eq!(an_hour_later, AgentState::Done);
    }

    #[test]
    fn given_an_agent_that_vanishes_without_declaring_its_end_when_the_probe_sees_it_then_the_failure_reaches_the_user_outside_ash(
    ) {
        // Given — `error` est le second état qui interrompt (spec §8), et son producteur
        // n'est pas un hook mais la boucle de sonde. C'est le seul chemin de notification
        // qui parte d'une passe de sonde : le brancher au verdict plutôt qu'au changement
        // rendrait le `Some` de la machine inutile.
        let Assembled {
            supervisor,
            notifier,
            ..
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("working", TAB));

        // When — le processus quitte l'avant-plan, puis la boucle continue de passer
        let after_the_crash = sweep(&supervisor, Presence::Prompt);
        for _ in 0..10 {
            sweep(&supervisor, Presence::Prompt);
        }

        // Then
        assert_eq!(after_the_crash, AgentState::Error);
        assert_eq!(
            notifier.titles(),
            vec!["an agent stopped on an error".to_owned()]
        );
    }

    #[test]
    fn given_an_agent_that_finishes_while_the_user_looks_away_when_it_declares_done_then_nothing_interrupts_him(
    ) {
        // Given — « `done` ne notifie pas en v1 » (spec §8). Un travail fini n'attend rien :
        // la ligne de la sidebar suffit. C'est un refus, donc rien ne l'attraperait s'il
        // disparaissait — et l'interruption qui compte perdrait sa valeur.
        let Assembled {
            supervisor,
            clock,
            notifier,
        } = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);

        // When
        supervisor.on_hook(&hook("done", TAB));
        sweep(&supervisor, Presence::Prompt);
        clock.advance(60);
        sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(notifier.titles(), Vec::<String>::new());
    }

    #[test]
    fn given_a_hook_that_names_a_transcript_when_it_arrives_then_the_tab_carries_what_the_session_consumes(
    ) {
        // Given — le scénario du ticket : un onglet servi par `claude-code`, et un transcript
        // que l'outil tient déjà. Rien n'a été installé pour ça — `transcript_path` voyage sur
        // le `stdin` de **chaque** hook.
        let Assembled { supervisor, .. } = SupervisorBuilder::new()
            .holding_transcript(TRANSCRIPT, &transcript_of(146_273))
            .build();
        sweep(&supervisor, Presence::Program);

        // When — l'agent finit son tour.
        supervisor.on_hook(&hook("done", TAB).with_transcript(Some(TRANSCRIPT)));

        // Then — la mesure voyage avec l'onglet, à la passe suivante, sans canal à elle.
        assert_eq!(
            sweep_usage(&supervisor, Presence::Program),
            Some(SessionUsage {
                used_tokens: 146_273,
                window_tokens: Some(200_000),
                // La queue de ce scénario ne nomme aucun modèle — c'est la mesure qu'il
                // vérifie, et le nom a ses propres scénarios.
                model: None,
            })
        );
    }

    #[test]
    fn given_a_measured_tab_when_a_hook_arrives_without_a_transcript_then_the_last_measure_stays() {
        // Given — un onglet qui a déjà sa mesure. La suite est le cas courant d'un outil qui
        // ne nomme pas toujours son transcript, ou d'une trame que la borne du fil a
        // dépouillée : une absence de mesure n'est pas une mesure à zéro.
        let Assembled { supervisor, .. } = SupervisorBuilder::new()
            .holding_transcript(TRANSCRIPT, &transcript_of(90_000))
            .build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("working", TAB).with_transcript(Some(TRANSCRIPT)));

        // When — deux hooks muets sur ce point : l'un sans chemin, l'autre en nommant un
        // fichier que rien ne peut lire.
        supervisor.on_hook(&hook("waiting", TAB));
        supervisor.on_hook(&hook("working", TAB).with_transcript(Some("/tmp/effacé.jsonl")));

        // Then — l'onglet garde ce qu'il savait, et aucune erreur n'a de trace visible.
        assert_eq!(
            sweep_usage(&supervisor, Presence::Program).map(|usage| usage.used_tokens),
            Some(90_000)
        );
    }

    #[test]
    fn given_a_measured_tab_when_it_becomes_a_shell_row_again_then_its_gauge_leaves_with_the_agent()
    {
        // Given — la jauge décrit une conversation, pas un onglet. Un shell revenu à son
        // invite n'en a plus, et laisser la dernière mesure ferait lire la place occupée par
        // un agent qui n'est plus là.
        let Assembled {
            supervisor, clock, ..
        } = SupervisorBuilder::new()
            .watched()
            .holding_transcript(TRANSCRIPT, &transcript_of(90_000))
            .build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("done", TAB).with_transcript(Some(TRANSCRIPT)));

        // When — la ligne `done` vit ses trente secondes, puis l'onglet redevient un shell.
        sweep(&supervisor, Presence::Prompt);
        clock.advance(30);
        let once_it_is_a_shell_row = supervisor.state(TAB, Presence::Prompt);

        // Then
        assert_eq!(once_it_is_a_shell_row.status.state, AgentState::Idle);
        assert_eq!(once_it_is_a_shell_row.usage, None);
    }

    #[test]
    fn given_a_tab_served_only_by_the_generic_adapter_when_a_transcript_is_named_then_nothing_measures_it(
    ) {
        // Given — le deuxième scénario du ticket. L'adaptateur `generic` déclare
        // `UsageSupport::None` : même en lui présentant un transcript parfaitement lisible,
        // l'onglet doit rester **sans** usage — pas une jauge à zéro, pas un tiret.
        let Assembled { supervisor, .. } = SupervisorBuilder::new()
            .served_only_by_the_generic_adapter()
            .holding_transcript(TRANSCRIPT, &transcript_of(146_273))
            .build();
        sweep(&supervisor, Presence::Program);

        // When
        supervisor.on_hook(&hook("done", TAB).with_transcript(Some(TRANSCRIPT)));

        // Then — et l'état n'a pas bougé non plus : aucun adaptateur ne comprend ce mot ici.
        let after = supervisor.state(TAB, Presence::Program);
        assert_eq!(after.usage, None);
        assert_eq!(after.status.state, AgentState::Working);
    }

    #[test]
    fn given_a_hook_carrying_only_a_transcript_when_no_adapter_reads_its_verb_then_the_measure_still_lands(
    ) {
        // Given — un onglet mesuré par un verbe qu'aucun adaptateur ne traduit. Le chemin
        // court d'`on_hook` — « rien à dire, on s'arrête » — doit tenir compte de la mesure,
        // sinon un `PreToolUse` porteur d'un transcript frais serait jeté avec son verbe.
        let Assembled { supervisor, .. } = SupervisorBuilder::new()
            .holding_transcript(TRANSCRIPT, &transcript_of(12_345))
            .build();
        sweep(&supervisor, Presence::Program);

        // When — `Stop` est un nom de hook de Claude Code, pas un mot de la spec §6.3 : il ne
        // se traduit en aucun état.
        supervisor.on_hook(&hook("Stop", TAB).with_transcript(Some(TRANSCRIPT)));

        // Then — l'état reste celui de la sonde, et la mesure est arrivée quand même.
        let after = supervisor.state(TAB, Presence::Program);
        assert_eq!(after.status.state, AgentState::Working);
        assert_eq!(after.usage.map(|usage| usage.used_tokens), Some(12_345));
    }

    #[test]
    fn given_a_hook_saying_where_it_ran_when_the_repository_declares_its_own_model_then_that_window_is_the_one_measured(
    ) {
        // Given — le `cwd` traverse comme une **donnée**, jamais comme une corrélation
        // (ADR-0007 : `ASH_TAB_ID` reste la seule). Ce qu'il apporte est le chemin des deux
        // couches de configuration du dépôt, que le foyer seul ne peut pas donner.
        let Assembled { supervisor, .. } = SupervisorBuilder::new()
            .running_the_model("sonnet")
            .whose_repository_declares("opus[1m]")
            .holding_transcript(TRANSCRIPT, &transcript_of(57_200))
            .build();
        sweep(&supervisor, Presence::Program);

        // When
        supervisor.on_hook(
            &hook("waiting", TAB)
                .with_transcript(Some(TRANSCRIPT))
                .with_cwd(Some(REPO)),
        );

        // Then — la fenêtre du dépôt, pas celle du foyer. L'onglet reste bien corrélé par son
        // `tab_id` : c'est lui, et lui seul, qui a désigné cette machine à états.
        assert_eq!(
            sweep_usage(&supervisor, Presence::Program),
            Some(SessionUsage {
                used_tokens: 57_200,
                window_tokens: Some(1_000_000),
                model: None,
            })
        );
    }

    #[test]
    fn given_a_tab_whose_tool_names_no_model_anywhere_when_it_is_measured_then_the_window_is_unknown_and_the_count_survives(
    ) {
        // Given — l'installation par défaut : personne n'a posé de `model` nulle part. C'est
        // le cas que l'ancien `DEFAULT_CONTEXT_WINDOW` traitait en supposant 200 000, ce qui
        // faisait lire `ctx 29%` à une conversation qui en occupait 6 % de sa fenêtre réelle.
        let Assembled { supervisor, .. } = SupervisorBuilder::new()
            .naming_no_model()
            .holding_transcript(TRANSCRIPT, &transcript_of(57_200))
            .build();
        sweep(&supervisor, Presence::Program);

        // When
        supervisor.on_hook(&hook("waiting", TAB).with_transcript(Some(TRANSCRIPT)));

        // Then — aucun dénominateur, et le numérateur intact : l'écran lira `ctx 57k`, sans
        // barre et sans couleur de seuil.
        assert_eq!(
            sweep_usage(&supervisor, Presence::Program),
            Some(SessionUsage {
                used_tokens: 57_200,
                window_tokens: None,
                model: None,
            })
        );
    }
}
