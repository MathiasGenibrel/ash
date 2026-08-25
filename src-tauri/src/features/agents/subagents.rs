//! Les sous-agents d'un onglet : qui vit dessous, depuis quand, et jusqu'à quand.
//!
//! Un sous-agent n'est **pas** un onglet, et ce fichier existe pour que la différence ne
//! puisse pas se perdre ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), amendement
//! du 2026-08-13, et [ADR-0003](../../../../docs/adr/0003-zone-terminal-unique.md)) :
//!
//! - il n'a **pas de PTY**, donc pas de terminal à sélectionner. La sidebar rend ses lignes
//!   filles inertes, et c'est le parent qu'un clic sélectionne (spec §6.5) ;
//! - il n'est **jamais `waiting`**. Il ne peut pas interroger l'utilisateur, donc il ne peut
//!   pas rendre son parent `waiting` non plus. Deux états lui suffisent, et [`Life`] est ce
//!   qui rend la règle structurelle plutôt que surveillée : il n'y a pas de troisième
//!   variante à écrire ;
//! - son `agent_id` ne distingue que des **frères dans un onglet**. Il n'est ni stable entre
//!   deux sessions, ni une clé de persistance, et l'attribution d'ADR-0014 continue de ne
//!   connaître que l'onglet et l'identifiant de l'outil.
//!
//! ## Comment un enfant apparaît, et comment il s'en va
//!
//! **Aucun outil n'annonce le démarrage d'un sous-agent** : il n'existe pas de
//! `SubagentStart`. La naissance se lit donc au premier événement portant un `agent_id`
//! encore inconnu — un `PreToolUse` déclenché *dans* l'enfant, le plus souvent. Un enfant qui
//! n'emploierait aucun outil n'apparaîtrait qu'à sa fin, par `SubagentStop`, et sa ligne
//! naîtrait alors directement `done` pour le temps de son affichage. C'est peu, mais c'est
//! honnête : Ash ne peut pas montrer ce que l'outil ne dit pas.
//!
//! **Une ligne fille n'appartient pas à l'onglet, mais à la session qui l'a créée**, et elle
//! ne lui survit pas. C'est la seconde façon de finir, et elle existe parce que la première
//! ne suffit pas : `SubagentStop` est le seul verbe qui nomme un enfant, et il ne part pas
//! toujours — un agent tué, un `/clear`, un compactage, un `claude` relancé laissent des
//! enfants que plus rien n'aurait terminés. Une ligne `working · 17h44m` sous un parent qui
//! n'a plus aucun sous-agent en est le symptôme.
//!
//! Deux gestes ferment donc une session, et [`Subagents::session_over`] est le seul chemin
//! des deux : une session qui **s'ouvre** (`SessionStart`, qui couvre le redémarrage, la
//! reprise, le `/clear` et le compactage) et une session qui **finit** (`SessionEnd`, un
//! échec déclaré, un processus disparu). Ce n'est pas une déduction tirée d'un silence : ce
//! sont des hooks, et ADR-0007 ne demande rien d'autre. **Un parent qui passe `waiting`, lui,
//! ne dit rien de ses enfants** — un agent attend couramment ses sous-agents tout en restant
//! disponible, et fermer leurs lignes sur un `Stop` effacerait du travail réel.
//!
//! Ce qui en sort est `done`, jamais `error` : Ash ne sait pas mieux si l'enfant a réussi que
//! dans le cas annoncé, et il ne le prétend pas. La ligne finit ensuite **comme les autres** —
//! [`SUBAGENT_LINGER`], puis elle s'en va. Elle ne disparaît pas d'un coup : un enfant effacé
//! sans être montré serait un travail dont il ne resterait aucune trace à l'écran.
//!
//! **L'échec d'un sous-agent n'a aucune source, et c'est un angle mort assumé.** Pour un
//! onglet, `error` vient de la disparition du processus (spec §6.4) ; un enfant n'a pas de
//! processus à lui — c'est le même `claude`, dans le même onglet. Un enfant qui échoue
//! ressemblera donc exactement à un enfant qui réussit. Rien ici n'invente une source :
//! inventer un `error` par un délai ou par une lecture de la sortie serait précisément ce
//! qu'ADR-0007 refuse.
//!
//! ## Ce qui date une ligne fille
//!
//! [`AgentStatus::entering`], la même règle que pour un onglet, et pour la même raison : ce
//! qui traverse la frontière est une **date d'entrée**, jamais une durée. Le `TabInfo` est
//! comparé entier pour décider s'il faut émettre — une durée vivante ferait partir
//! `ash://tab-changed` chaque seconde pour chaque onglet qui porte un enfant, et l'on
//! paierait un rendu complet de la sidebar par seconde pour animer un compteur. Le compteur
//! est un fait d'affichage, et il le reste.

use std::time::Duration;

use super::state::{AgentState, AgentStatus};
use crate::shared::time::UnixMillis;

/// Combien de temps la ligne d'un sous-agent fini reste visible avant de disparaître.
///
/// **C'est un réglage, et sa valeur par défaut** (spec §6.5) : le superviseur la reçoit à la
/// construction, et le composition root pose celle-ci. Elle n'est pas encore éditable depuis
/// la fenêtre de réglages — voir le compte rendu de la tranche — mais elle n'est déjà plus un
/// nombre écrit au milieu d'une règle.
///
/// Dix secondes, et non les trente d'une ligne d'onglet ([`super::LINGER`]) : une ligne
/// d'onglet finie *est* l'information qu'on est venu chercher, alors qu'une ligne fille n'est
/// qu'une étape d'un travail qui continue au-dessus d'elle. La garder trop longtemps ferait
/// croître la colonne pendant qu'un agent enchaîne ses sous-tâches.
pub const SUBAGENT_LINGER: Duration = Duration::from_secs(10);

/// Les deux seules vies d'un enfant.
///
/// Il n'y a pas de troisième variante, et c'est ainsi que « un sous-agent n'est jamais
/// `waiting` » tient sans être surveillé : il n'existe aucune façon de l'écrire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Life {
    AtWork,
    Ended,
}

impl Life {
    fn state(self) -> AgentState {
        match self {
            Life::AtWork => AgentState::Working,
            Life::Ended => AgentState::Done,
        }
    }
}

/// Une ligne fille, telle qu'elle traverse la frontière.
///
/// `agent_type` est facultatif parce que rien ne garantit qu'un outil le donne : la trame le
/// transporte tel quel, et c'est la sidebar qui décide comment nommer un enfant anonyme —
/// une décision d'affichage, qui n'a pas à être prise ici
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Subagent {
    /// Ce qui distingue deux frères **dans cet onglet**, et rien de plus.
    pub agent_id: String,
    /// Le type que l'outil donne à l'enfant — `code-reviewer`, `Explore`, `general-purpose`.
    pub agent_type: Option<String>,
    /// `working` ou `done`, jamais autre chose.
    pub state: AgentState,
    /// Quand l'enfant est entré dans cet état, en millisecondes depuis l'époque Unix.
    #[cfg_attr(test, ts(type = "number"))]
    pub since: UnixMillis,
}

/// Les enfants d'un onglet, dans l'ordre où ils sont apparus.
///
/// Un `Vec` et non une table : l'ordre est ce que la colonne montre, et deux ou trois enfants
/// en parallèle ne justifient pas d'indexer quoi que ce soit.
pub struct Subagents {
    linger: Duration,
    children: Vec<Child>,
}

struct Child {
    agent_id: String,
    agent_type: Option<String>,
    life: Life,
    /// L'état montré et sa date, produits par la règle commune — jamais recopiée ici.
    status: AgentStatus,
    /// La session qui portait cet enfant n'existe plus : son `agent_id` ne désigne personne.
    ///
    /// **Ce n'est pas une troisième [`Life`]**, et la distinction porte : une vie est un état
    /// montré, celui-ci est une provenance, et il ne se voit nulle part à l'écran. Il ne sert
    /// qu'à une chose — un `agent_id` n'est unique que **parmi les frères d'une session**
    /// (voir en tête de fichier), donc la session suivante peut très bien renommer un enfant
    /// `agent-7`. Sans ce drapeau, il retrouverait la ligne de son homonyme mort et la
    /// ressusciterait, à la place de naître.
    retired: bool,
}

impl Subagents {
    pub fn new(linger: Duration) -> Self {
        Self {
            linger,
            children: Vec::new(),
        }
    }

    /// Un événement est arrivé de cet enfant : il est là, et il travaille.
    ///
    /// C'est le seul chemin par lequel un enfant naît. Un enfant déjà fini qui reparlerait
    /// repartirait au travail — ça ne devrait pas arriver, et si ça arrive, l'afficher est
    /// plus honnête que de le taire.
    pub fn at_work(&mut self, agent_id: &str, agent_type: Option<&str>, now: UnixMillis) {
        self.record(agent_id, agent_type, Life::AtWork, now);
    }

    /// Cet enfant vient de finir : sa ligne passe `done`, et son compte à rebours part d'ici.
    pub fn ended(&mut self, agent_id: &str, agent_type: Option<&str>, now: UnixMillis) {
        self.record(agent_id, agent_type, Life::Ended, now);
    }

    /// Le temps a passé : les lignes finies depuis assez longtemps s'en vont.
    ///
    /// Appelée par la passe de sonde, comme le `tick` de la machine à états : rien ne se
    /// réveille pour un sous-agent, et la boucle qui passe déjà suffit.
    pub fn tick(&mut self, now: UnixMillis) {
        let linger = self
            .linger
            .as_millis()
            .try_into()
            .unwrap_or(UnixMillis::MAX);
        self.children.retain(|child| {
            child.life == Life::AtWork || now.saturating_sub(child.status.since) < linger
        });
    }

    /// La session qui portait ces enfants n'est plus : ceux qui travaillaient encore finissent.
    ///
    /// Le seul chemin des deux gestes décrits en tête de fichier — une session qui s'ouvre,
    /// une session qui finit —, et ce n'est **pas** [`Self::clear`] : chaque enfant devient
    /// `done` à la date du geste, puis suit le vieillissement normal. La différence est tout
    /// le ticket : effacer ne montre rien, alors que la fin d'un enfant est ce qu'un
    /// utilisateur revient lire.
    ///
    /// Les enfants déjà finis sont retirés eux aussi, sans que leur ligne bouge : ils finissent
    /// de s'afficher, mais leur `agent_id` a cessé de désigner quiconque
    /// ([`Child::retired`]).
    pub fn session_over(&mut self, now: UnixMillis) {
        for child in &mut self.children {
            if child.life == Life::AtWork {
                child.life = Life::Ended;
                child.status = AgentStatus::entering(Some(child.status), Life::Ended.state(), now);
            }
            child.retired = true;
        }
    }

    /// Plus personne : l'onglet n'est plus un agent.
    pub fn clear(&mut self) {
        self.children.clear();
    }

    /// Ce que la sidebar montre sous la ligne de l'onglet.
    pub fn shown(&self) -> Vec<Subagent> {
        self.children
            .iter()
            .map(|child| Subagent {
                agent_id: child.agent_id.clone(),
                agent_type: child.agent_type.clone(),
                state: child.status.state,
                since: child.status.since,
            })
            .collect()
    }

    fn record(&mut self, agent_id: &str, agent_type: Option<&str>, life: Life, now: UnixMillis) {
        if let Some(known) = self
            .children
            .iter_mut()
            .find(|child| !child.retired && child.agent_id == agent_id)
        {
            // Le type peut arriver plus tard que le premier événement : un enfant nommé ne
            // redevient jamais anonyme.
            if let Some(agent_type) = agent_type {
                known.agent_type = Some(agent_type.to_owned());
            }
            known.life = life;
            known.status = AgentStatus::entering(Some(known.status), life.state(), now);
            return;
        }

        self.children.push(Child {
            agent_id: agent_id.to_owned(),
            agent_type: agent_type.map(str::to_owned),
            life,
            status: AgentStatus::entering(None, life.state(), now),
            retired: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: UnixMillis = 1_767_225_600_000;

    /// Test Data Builder : les enfants d'un onglet, avec le réglage par défaut.
    fn children() -> Subagents {
        Subagents::new(SUBAGENT_LINGER)
    }

    fn labels(children: &Subagents) -> Vec<(String, AgentState)> {
        children
            .shown()
            .into_iter()
            .map(|child| (child.agent_type.unwrap_or_default(), child.state))
            .collect()
    }

    #[test]
    fn given_two_subagents_running_at_once_when_one_of_them_stops_then_only_that_one_is_done() {
        // Given — le cas que l'amendement du 2026-08-13 rend possible : `ASH_TAB_ID` est
        // identique pour les deux enfants et pour leur parent, donc seul `agent_id` peut
        // apparier un `SubagentStop` à la ligne qui l'attend.
        let mut children = children();
        children.at_work("agent-7", Some("explore"), NOW);
        children.at_work("agent-8", Some("code-reviewer"), NOW);

        // When
        children.ended("agent-8", None, NOW + 5_000);

        // Then
        assert_eq!(
            labels(&children),
            vec![
                ("explore".to_owned(), AgentState::Working),
                ("code-reviewer".to_owned(), AgentState::Done),
            ]
        );
    }

    #[test]
    fn given_a_subagent_that_just_finished_when_ten_seconds_of_sweeps_pass_then_its_row_disappears()
    {
        // Given — la ligne d'un enfant fini est une information brève : la garder ferait
        // croître la colonne à chaque sous-tâche d'un agent qui en enchaîne dix.
        let mut children = children();
        children.at_work("agent-7", Some("explore"), NOW);
        children.ended("agent-7", None, NOW + 1_000);

        // When
        children.tick(NOW + 10_000);
        let still_shown = children.shown().len();
        children.tick(NOW + 11_000);

        // Then — visible pendant dix secondes pleines, et pas une de moins
        assert_eq!(still_shown, 1);
        assert_eq!(children.shown(), vec![]);
    }

    #[test]
    fn given_a_subagent_at_work_when_hours_of_sweeps_pass_then_its_row_never_expires() {
        // Given — l'autre moitié : seule une ligne **finie** s'efface. Une sous-tâche longue
        // — une exploration, une suite de tests — ne doit pas disparaître pour avoir duré,
        // sinon la colonne mentirait sur ce qui tourne.
        let mut children = children();
        children.at_work("agent-7", Some("explore"), NOW);

        // When
        children.tick(NOW + 3_600_000);

        // Then
        assert_eq!(
            labels(&children),
            vec![("explore".to_owned(), AgentState::Working)]
        );
    }

    #[test]
    fn given_a_subagent_that_keeps_using_tools_when_it_speaks_again_then_its_entry_date_never_moves(
    ) {
        // Given — le piège de la datation, à l'échelle d'une ligne fille : un enfant émet un
        // hook par outil employé. Redater à chaque fois ferait changer la fiche de l'onglet à
        // chaque outil, donc partir un `ash://tab-changed`, donc redessiner la sidebar — pour
        // un compteur qui n'a pas bougé de sens.
        let mut children = children();
        children.at_work("agent-7", Some("explore"), NOW);

        // When
        for tool in 1..=20 {
            children.at_work("agent-7", Some("explore"), NOW + tool * 1_000);
        }

        // Then
        assert_eq!(children.shown().first().map(|child| child.since), Some(NOW));
    }

    #[test]
    fn given_two_subagents_at_work_when_their_session_is_over_then_both_finish_and_then_fade_like_any_other(
    ) {
        // Given — le cas du ticket : deux enfants dont le `SubagentStop` ne partira jamais,
        // parce que la session qui les portait n'est plus. Ce qui est en jeu ici est la
        // différence entre finir et disparaître : `clear` les effacerait sans rien montrer.
        let mut children = children();
        children.at_work("agent-7", Some("explore"), NOW);
        children.at_work("agent-8", Some("qa"), NOW);

        // When
        children.session_over(NOW + 60_000);
        let once_the_session_ended = labels(&children);
        children.tick(NOW + 69_000);
        let still_shown = children.shown().len();
        children.tick(NOW + 70_000);

        // Then — `done`, et daté du geste : les dix secondes partent de là, pas de leur
        // naissance, sinon deux enfants nés il y a une heure disparaîtraient sans un rendu.
        assert_eq!(
            once_the_session_ended,
            vec![
                ("explore".to_owned(), AgentState::Done),
                ("qa".to_owned(), AgentState::Done),
            ]
        );
        assert_eq!(still_shown, 2);
        assert_eq!(children.shown(), vec![]);
    }

    #[test]
    fn given_a_retired_subagent_still_on_screen_when_the_next_session_reuses_its_id_then_a_new_row_is_born(
    ) {
        // Given — un `agent_id` ne distingue que des **frères dans un onglet** (voir en tête
        // de fichier) : rien n'empêche la session suivante de renommer un enfant `agent-7`.
        // S'il retrouvait la ligne de son homonyme mort, il en hériterait l'état et la date,
        // et l'on verrait un enfant naître déjà vieux d'une heure.
        let mut children = children();
        children.at_work("agent-7", Some("explore"), NOW);
        children.session_over(NOW + 60_000);

        // When — la nouvelle session, dans la seconde qui suit
        children.at_work("agent-7", Some("code-reviewer"), NOW + 61_000);

        // Then — deux lignes : celle qui finit de s'afficher, et celle qui commence
        assert_eq!(
            labels(&children),
            vec![
                ("explore".to_owned(), AgentState::Done),
                ("code-reviewer".to_owned(), AgentState::Working),
            ]
        );
        assert_eq!(
            children.shown().last().map(|child| child.since),
            Some(NOW + 61_000)
        );
    }

    #[test]
    fn given_a_subagent_that_never_used_a_tool_when_it_stops_then_its_row_appears_already_done() {
        // Given — aucun outil n'annonce le *démarrage* d'un enfant : un enfant qui n'emploie
        // aucun outil n'est révélé que par sa fin. Montrer sa ligne dix secondes est tout ce
        // qu'Ash peut honnêtement en dire ; ne rien montrer effacerait un travail qui a eu
        // lieu.
        let mut children = children();

        // When
        children.ended("agent-9", Some("qa"), NOW);

        // Then
        assert_eq!(labels(&children), vec![("qa".to_owned(), AgentState::Done)]);
    }
}
