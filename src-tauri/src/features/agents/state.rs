use crate::shared::time::UnixMillis;

/// Les cinq états d'un agent — le vocabulaire commun du produit.
///
/// C'est **le backend** qui les détient
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : le frontend rend
/// un état, il ne le calcule pas. Et c'est la feature `agents` qui les déclare, et non
/// celle qui les affiche, parce qu'[ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)
/// en fait la frontière que les particularités d'un outil n'ont pas le droit de franchir :
/// un adaptateur traduit vers ces cinq mots, et le cœur n'en connaît pas d'autres.
///
/// Ils ont d'abord vécu dans `pty`, la seule feature qui en produisait. Les faire remonter
/// ici est ce qui permet aux trois producteurs à venir — socket d'événements, adaptateurs,
/// machine à états — de partager le même type sans dépendre les uns des autres.
///
/// À ce jalon, seuls `Idle` et `Working` ont un producteur : la sonde
/// d'[ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md), qui sait si le shell est à
/// son invite ou si autre chose tient l'avant-plan. `Waiting`, `Done` et `Error` viendront
/// des **hooks** ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)), qui interdit de
/// les déduire de la sortie du PTY. Rien ne les produit aujourd'hui, et c'est le
/// comportement correct.
///
/// La représentation sérialisée est le contrat partagé avec le TypeScript
/// (`src/shared/ipc`) : cinq mots en minuscules, et `presentAgentState` en face. Elle se lit
/// **dans les deux sens** depuis que la fenêtre de réglages nomme l'état dont on bascule
/// l'interrupteur (spec §9) : un mot qui n'est pas l'un des cinq est refusé par Tauri avant
/// d'atteindre une règle.
///
/// Un état seul ne dit pas depuis quand il dure : c'est [`AgentStatus`] qui le date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum AgentState {
    Idle,
    Working,
    Waiting,
    Done,
    Error,
}

/// L'état d'un onglet, et **depuis quand** il y est.
///
/// La date est absolue (millisecondes depuis l'époque Unix) et non une durée, et c'est la
/// décision qui porte tout le reste : une durée changerait de valeur à chaque passe de la
/// boucle de sonde, donc la fiche d'onglet changerait avec elle, donc l'event
/// `ash://tab-changed` partirait chaque seconde pour chaque onglet actif — on paierait un
/// rendu complet de la sidebar pour animer un compteur. Envoyée une fois, en absolu, la
/// fiche redevient stable et le compteur redevient ce qu'il est : un problème d'affichage,
/// que le frontend résout avec sa propre horloge.
///
/// C'est bien le backend qui date : le frontend rend une durée, il n'invente pas son
/// origine ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentStatus {
    pub state: AgentState,
    /// Quand cet onglet est **entré** dans cet état, et non quand on l'a lu.
    pub since: UnixMillis,
}

impl AgentStatus {
    /// Le statut d'un onglet qui montre `state`, sachant celui qu'il montrait avant.
    ///
    /// **C'est ici, et nulle part ailleurs, que la datation se décide.** Trois lignes, mais
    /// elles portent toute la promesse du type : la date suit le **verdict affiché**, jamais
    /// la source qui le produit ni la passe qui le lit. Deux conséquences, et il a fallu
    /// écrire la règle une seule fois pour que les deux tiennent ensemble :
    ///
    /// - une passe de sonde qui reconduit le même état rend la même date, sinon la fiche de
    ///   l'onglet changerait trois fois par seconde et l'event `ash://tab-changed`
    ///   deviendrait un flux ;
    /// - un hook qui déclare le mot que l'onglet montrait **déjà** ne redate pas non plus.
    ///   C'est la séquence de tout démarrage d'agent — la sonde voit `claude` prendre
    ///   l'avant-plan, le premier hook n'arrive qu'au premier outil employé — et le
    ///   compteur repartait de zéro sous les yeux de l'utilisateur.
    #[must_use]
    pub fn entering(previous: Option<Self>, state: AgentState, now: UnixMillis) -> Self {
        match previous {
            Some(known) if known.state == state => known,
            _ => Self { state, since: now },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_the_five_agent_states_when_they_cross_the_boundary_then_they_keep_the_names_the_frontend_knows(
    ) {
        // Given — le même modèle est déclaré des deux côtés de la frontière : `AgentState`
        // ici, `AgentState` dans `src/shared/ipc/index.ts`. Depuis #67, `mirror.ts` les
        // tient ensemble à la compilation : un état **renommé** ici fait échouer
        // `bun run typecheck`, et ce test n'a plus à s'en charger.
        //
        // Il reste parce qu'il garde autre chose, que la génération ne sait pas garder :
        // le `match` ci-dessous est exhaustif, donc un état **ajouté** ne compile pas tant
        // que son mot sur le fil n'a pas été décidé ici, à la main. `ts-rs` se contenterait
        // de l'exporter — le nouveau nom traverserait sans que personne ne l'ait choisi.
        let states = [
            AgentState::Idle,
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Done,
            AgentState::Error,
        ];
        let expected = states.map(|state| match state {
            AgentState::Idle => "idle",
            AgentState::Working => "working",
            AgentState::Waiting => "waiting",
            AgentState::Done => "done",
            AgentState::Error => "error",
        });

        // When
        let on_the_wire: Vec<String> = states
            .iter()
            .map(|state| serde_json::to_string(state).unwrap())
            .collect();

        // Then
        assert_eq!(
            on_the_wire,
            expected
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
        );
    }
}
