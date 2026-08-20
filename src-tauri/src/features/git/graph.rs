//! Les couloirs du graphe de commits — une **fonction pure**, et rien d'autre (spec §7.2).
//!
//! Une liste de commits ordonnée topologiquement entre, une affectation de couloirs sort.
//! Aucune entrée-sortie, aucune horloge lue ici : `now` est un paramètre, parce que la règle
//! des 30 jours est une règle datée et qu'un test qui dépend du jour est un test qui casse
//! tout seul.
//!
//! # Pourquoi les couloirs sont calculés en Rust
//!
//! C'est le critère d'acceptation de l'issue #27, et il a une raison : l'affectation dépend
//! de **tout ce qui précède** une ligne — un couloir ouvert par un commit reste ouvert
//! jusqu'à ce que son parent arrive. Une fenêtre calculée côté écran devrait donc soit tout
//! recalculer à chaque rendu, soit garder l'état des couloirs entre deux rendus — c'est-à-dire
//! détenir en TypeScript ce que le backend détient
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! C'est aussi ce qui décide la **forme de la pagination** : le graphe grandit par une
//! **fenêtre** qui repart toujours du sommet, jamais par des pages indépendantes. Une page
//! qui commencerait au 201ᵉ commit ne saurait pas quels couloirs y arrivent, et le dessin
//! serait faux dès le premier trait.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::shared::time::UnixMillis;

/// Combien de couloirs se dessinent avant qu'on replie (spec §7.2).
///
/// « Quatre suffisent à la plupart des dépôts » : au-delà, la colonne du graphe mange la
/// largeur du sujet, et le dessin cesse d'être lisible avant d'être informatif.
pub const MAX_LANES: usize = 4;

/// Au bout de combien de temps une branche est « inactive », en millisecondes.
///
/// Trente jours, tels que la spec §7.2 les écrit. La durée est ici, en clair, et pas dans la
/// règle qui la lit : c'est un choix de produit, et il doit se relire sans dérouler un
/// algorithme.
pub const INACTIVE_AFTER: UnixMillis = 30 * 24 * 60 * 60 * 1_000;

/// Un commit tel que le graphe a besoin de le connaître.
///
/// Les quatre premiers champs sont ceux de [`super::CommitRecord`] — c'est la clé
/// d'attribution d'[ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md),
/// et le graphe ne la redéfinit pas. S'y ajoutent ce que **dessiner** demande : les parents,
/// qui font les traits, et les refs, qui nomment une branche quand on doit dire laquelle a
/// été repliée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCommit {
    pub sha: String,
    /// L'identifiant abrégé, tel que git l'abrège (`%h`) — jamais tronqué ici.
    pub short: String,
    /// Les parents, dans l'ordre de git : le premier est la suite du couloir, les autres
    /// sont des fusions.
    pub parents: Vec<String>,
    /// La date d'auteur telle que git l'écrit (`%aI`). Moitié de la clé de repli d'ADR-0014.
    pub author_date: String,
    /// La même, en secondes Unix (`%at`) : ce qui se compare à une horloge sans analyse.
    pub authored_at: u64,
    /// Le nom d'auteur git — ce que la colonne `by` affiche **faute d'attribution**.
    pub author: String,
    /// Les refs qui pointent ici (`%D`), déjà découpées. Vide pour l'immense majorité.
    pub refs: Vec<String>,
    pub subject: String,
}

/// Un trait qui descend d'une ligne vers la suivante.
///
/// `from` est la colonne au niveau de la ligne qui le porte, `to` la colonne au niveau de la
/// ligne d'en dessous. `from == to` est un trait droit ; le reste est une naissance de
/// branche, une fusion, ou une convergence de deux enfants vers le même parent.
///
/// Le trait est porté par la ligne du **haut** : c'est ce qui permet à l'écran de peindre
/// chaque ligne indépendamment, sans jamais avoir à regarder sa voisine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub from: usize,
    pub to: usize,
}

/// Où une ligne se pose, et ce qui en descend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// L'index du commit dans la liste **donnée en entrée** — le repli en écarte, donc les
    /// lignes ne sont pas numérotées comme les commits.
    pub commit: usize,
    pub lane: usize,
    pub links: Vec<Link>,
}

/// Une branche qu'on a repliée, et de quoi le dire à qui regarde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldedBranch {
    /// Le nom de la branche si un ref la désigne, son identifiant abrégé sinon.
    pub name: String,
    /// La date de son commit le plus récent, en secondes Unix.
    pub last_activity: u64,
}

/// Le dessin complet d'une fenêtre de commits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Layout {
    pub rows: Vec<Placed>,
    /// Combien de couloirs la largeur doit réserver. `0` pour une fenêtre vide.
    pub lanes: usize,
    /// Les branches écartées par la règle des 30 jours, de la plus récente à la plus vieille.
    pub folded: Vec<FoldedBranch>,
}

/// Les couloirs d'une fenêtre de commits, repli compris.
///
/// `commits` est attendu **ordonné topologiquement**, du plus récent au plus ancien : c'est
/// ce que `git log --topo-order` rend, et c'est la seule hypothèse de tout ce fichier. Un
/// parent absent de la fenêtre — parce qu'elle s'arrête avant lui — laisse simplement son
/// couloir courir jusqu'au bas du dessin, ce qui est exactement ce qu'on veut voir d'une
/// histoire tronquée.
///
/// `now` est l'heure murale, en millisecondes : elle ne sert qu'à la règle des 30 jours, et
/// elle est **passée**, jamais lue ici.
pub fn lay_out(commits: &[GraphCommit], now: UnixMillis) -> Layout {
    let placed = assign(commits, &(0..commits.len()).collect::<Vec<_>>());
    if placed.1 <= MAX_LANES {
        return Layout {
            rows: placed.0,
            lanes: placed.1,
            folded: Vec::new(),
        };
    }

    // Au-delà de quatre couloirs, et à ce moment-là seulement, la règle des 30 jours
    // s'applique : ce n'est pas un filtre permanent, c'est un secours de lisibilité.
    let (kept, folded) = fold_inactive(commits, now);
    if folded.is_empty() {
        return Layout {
            rows: placed.0,
            lanes: placed.1,
            folded,
        };
    }

    let (rows, lanes) = assign(commits, &kept);
    Layout {
        rows,
        lanes,
        folded,
    }
}

/// L'affectation elle-même, sur un sous-ensemble d'index déjà ordonné.
///
/// Deux invariants tiennent tout le dessin :
///
/// 1. **un trait posé ne change jamais de colonne** — un couloir libéré n'est repris que par
///    une ligne *neuve*, jamais par le déplacement d'une ligne existante. C'est ce qui fait
///    que tous les traits de passage sont droits, et que les seuls traits obliques sont ceux
///    qui *disent* quelque chose : une branche qui naît, une fusion, deux enfants qui se
///    rejoignent sur leur parent ;
/// 2. **le premier parent hérite du couloir du commit** — c'est la convention de git, et
///    c'est elle qui garde `main` sur une colonne droite d'un bout à l'autre.
fn assign(commits: &[GraphCommit], kept: &[usize]) -> (Vec<Placed>, usize) {
    // Le sha attendu par chaque couloir. `None` = couloir libre.
    let mut active: Vec<Option<String>> = Vec::new();
    let mut rows: Vec<Placed> = Vec::new();
    let mut lanes = 0usize;

    for &index in kept {
        let Some(commit) = commits.get(index) else {
            continue;
        };

        // Les couloirs qui attendaient ce commit. Plusieurs quand il a plusieurs enfants
        // dans la fenêtre : ils convergent tous ici.
        let waiting: Vec<usize> = active
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.as_deref() == Some(commit.sha.as_str()))
            .map(|(lane, _)| lane)
            .collect();
        let lane = match waiting.first() {
            Some(&first) => first,
            None => free_slot(&mut active),
        };

        // Les traits que la ligne du dessus faisait descendre vers ces couloirs arrivent en
        // fait sur celui-ci : c'est la seule correction rétroactive de l'algorithme, et elle
        // ne remonte jamais plus haut que la ligne précédente.
        if let Some(previous) = rows.last_mut() {
            for link in &mut previous.links {
                if waiting.contains(&link.to) {
                    link.to = lane;
                }
            }
        }
        for &lane in &waiting {
            if let Some(slot) = active.get_mut(lane) {
                *slot = None;
            }
        }

        // Les parents : le premier reprend le couloir, les suivants ouvrent ou rejoignent.
        let mut opened: Vec<usize> = Vec::new();
        let mut merged: Vec<usize> = Vec::new();
        for (rank, parent) in commit.parents.iter().enumerate() {
            if let Some(existing) = active
                .iter()
                .position(|slot| slot.as_deref() == Some(parent.as_str()))
            {
                // Ce parent est déjà attendu ailleurs : le trait plonge vers ce couloir-là
                // plutôt que d'en ouvrir un doublon.
                merged.push(existing);
                continue;
            }
            let slot = if rank == 0 {
                lane
            } else {
                free_slot(&mut active)
            };
            if slot >= active.len() {
                active.resize(slot + 1, None);
            }
            if let Some(cell) = active.get_mut(slot) {
                *cell = Some(parent.clone());
            }
            opened.push(slot);
        }

        // Ce qui descend vers la ligne suivante.
        let mut links: Vec<Link> = active
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.is_some())
            .map(|(target, _)| Link {
                from: if opened.contains(&target) {
                    lane
                } else {
                    target
                },
                to: target,
            })
            .collect();
        // Une fusion vers un couloir déjà occupé s'ajoute au trait droit de ce couloir : les
        // deux existent, et n'en garder qu'un effacerait le lien de fusion.
        links.extend(merged.iter().map(|&target| Link {
            from: lane,
            to: target,
        }));

        lanes = lanes.max(active.len()).max(lane + 1);
        // Les couloirs vides de la fin ne réservent aucune largeur.
        while active.last() == Some(&None) {
            active.pop();
        }

        rows.push(Placed {
            commit: index,
            lane,
            links,
        });
    }

    (rows, lanes)
}

/// Le premier couloir libre, en en ouvrant un au besoin.
fn free_slot(active: &mut Vec<Option<String>>) -> usize {
    match active.iter().position(Option::is_none) {
        Some(free) => free,
        None => {
            active.push(None);
            active.len() - 1
        }
    }
}

/// La règle des 30 jours : quels commits restent, et quelles branches sont repliées.
///
/// Une **tête** de fenêtre est un commit dont aucun autre commit de la fenêtre n'est
/// l'enfant : c'est ce qui ouvre un couloir. Une tête plus vieille que 30 jours est repliée,
/// et avec elle tout ce qui n'est atteignable que par elle — ses commits communs avec une
/// branche gardée restent, évidemment, puisqu'ils sont atteignables autrement.
///
/// **La tête la plus récente n'est jamais repliée**, même si elle a plus de 30 jours : un
/// dépôt qu'on n'a pas touché depuis six mois doit montrer son histoire, pas un écran vide.
fn fold_inactive(commits: &[GraphCommit], now: UnixMillis) -> (Vec<usize>, Vec<FoldedBranch>) {
    let mut heads: Vec<usize> = tips(commits);
    // De la plus récente à la plus vieille : c'est l'ordre dans lequel on garde, et celui
    // dans lequel on rend ce qui a été replié.
    heads.sort_by_key(|&index| std::cmp::Reverse(commits.get(index).map_or(0, |c| c.authored_at)));

    let inactive_before = now.saturating_sub(INACTIVE_AFTER) / 1_000;
    let mut kept_heads: Vec<usize> = Vec::new();
    let mut folded: Vec<FoldedBranch> = Vec::new();
    for (rank, &index) in heads.iter().enumerate() {
        let Some(commit) = commits.get(index) else {
            continue;
        };
        if rank == 0 || commit.authored_at >= inactive_before {
            kept_heads.push(index);
        } else {
            folded.push(FoldedBranch {
                name: branch_name(commit),
                last_activity: commit.authored_at,
            });
        }
    }

    if folded.is_empty() {
        return ((0..commits.len()).collect(), folded);
    }

    let kept = reachable_from(commits, &kept_heads);
    (kept, folded)
}

/// Les commits de la fenêtre que personne n'y a pour parent.
fn tips(commits: &[GraphCommit]) -> Vec<usize> {
    let claimed: HashSet<&str> = commits
        .iter()
        .flat_map(|commit| commit.parents.iter().map(String::as_str))
        .collect();
    commits
        .iter()
        .enumerate()
        .filter(|(_, commit)| !claimed.contains(commit.sha.as_str()))
        .map(|(index, _)| index)
        .collect()
}

/// Les index atteignables depuis ces têtes, **dans l'ordre d'origine**.
///
/// L'ordre est celui de la fenêtre et pas celui du parcours : la liste d'entrée est déjà
/// topologique, et la réordonner casserait l'unique hypothèse de [`assign`].
fn reachable_from(commits: &[GraphCommit], heads: &[usize]) -> Vec<usize> {
    let by_sha: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(index, commit)| (commit.sha.as_str(), index))
        .collect();

    let mut seen: HashSet<usize> = HashSet::new();
    let mut queue: VecDeque<usize> = heads.iter().copied().collect();
    seen.extend(heads.iter().copied());
    while let Some(index) = queue.pop_front() {
        let Some(commit) = commits.get(index) else {
            continue;
        };
        for parent in &commit.parents {
            let Some(&next) = by_sha.get(parent.as_str()) else {
                continue;
            };
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }

    (0..commits.len())
        .filter(|index| seen.contains(index))
        .collect()
}

/// Le nom sous lequel on dit qu'une branche a été repliée.
///
/// Un ref quand il y en a un, l'identifiant abrégé sinon : dire « 3 branches repliées » sans
/// pouvoir en nommer une n'apprend rien.
fn branch_name(commit: &GraphCommit) -> String {
    commit
        .refs
        .iter()
        .find(|name| !name.starts_with("HEAD"))
        .cloned()
        .or_else(|| commit.refs.first().cloned())
        .unwrap_or_else(|| commit.short.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ash regarde un dépôt le 12 août 2026, à midi.
    const NOW: UnixMillis = 1_786_000_000_000;
    const DAY: u64 = 24 * 60 * 60;

    /// Test Data Builder : un commit dessinable, avec des défauts valides et déterministes.
    struct CommitBuilder {
        sha: String,
        parents: Vec<String>,
        refs: Vec<String>,
        /// L'âge en jours, compté depuis [`NOW`] : un test qui parle de la règle des 30
        /// jours n'a aucune raison de manipuler des secondes Unix.
        days_ago: u64,
    }

    impl CommitBuilder {
        fn new(sha: &str) -> Self {
            Self {
                sha: sha.to_owned(),
                parents: Vec::new(),
                refs: Vec::new(),
                days_ago: 0,
            }
        }

        fn parents(mut self, parents: &[&str]) -> Self {
            self.parents = parents.iter().map(|&p| p.to_owned()).collect();
            self
        }

        fn named(mut self, name: &str) -> Self {
            self.refs.push(name.to_owned());
            self
        }

        fn days_ago(mut self, days: u64) -> Self {
            self.days_ago = days;
            self
        }

        fn build(self) -> GraphCommit {
            GraphCommit {
                short: self.sha.clone(),
                sha: self.sha,
                parents: self.parents,
                author_date: "2026-08-12T12:00:00+02:00".to_owned(),
                authored_at: NOW / 1_000 - self.days_ago * DAY,
                author: "mathias".to_owned(),
                refs: self.refs,
                subject: "feat: something".to_owned(),
            }
        }
    }

    fn commit(sha: &str, parents: &[&str]) -> GraphCommit {
        CommitBuilder::new(sha).parents(parents).build()
    }

    /// Le couloir de chaque ligne, dans l'ordre du dessin.
    fn lanes_of(layout: &Layout) -> Vec<usize> {
        layout.rows.iter().map(|row| row.lane).collect()
    }

    /// Les shas dessinés, dans l'ordre du dessin.
    fn drawn(commits: &[GraphCommit], layout: &Layout) -> Vec<String> {
        layout
            .rows
            .iter()
            .filter_map(|row| commits.get(row.commit))
            .map(|commit| commit.sha.clone())
            .collect()
    }

    #[test]
    fn given_a_straight_history_when_it_is_laid_out_then_every_commit_shares_one_lane() {
        // Given — la forme de l'immense majorité d'un dépôt : une ligne droite.
        let commits = vec![commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])];

        // When
        let layout = lay_out(&commits, NOW);

        // Then
        assert_eq!(lanes_of(&layout), vec![0, 0, 0]);
        assert_eq!(layout.lanes, 1);
        // Et les traits sont droits, sauf le dernier commit qui n'a pas de parent.
        assert_eq!(layout.rows[0].links, vec![Link { from: 0, to: 0 }]);
        assert_eq!(layout.rows[1].links, vec![Link { from: 0, to: 0 }]);
        assert!(
            layout.rows[2].links.is_empty(),
            "une racine ne descend nulle part"
        );
    }

    #[test]
    fn given_two_branches_grown_from_one_commit_when_they_are_laid_out_then_they_converge_on_their_parent(
    ) {
        // Given — la fourche : deux têtes, un ancêtre commun. C'est ce que deux worktrees
        // d'un même dépôt produisent en permanence (ADR-0012).
        let commits = vec![
            commit("feat", &["base"]),
            commit("fix", &["base"]),
            commit("base", &[]),
        ];

        // When
        let layout = lay_out(&commits, NOW);

        // Then — deux couloirs, et les deux traits arrivent sur la ligne de `base`
        assert_eq!(lanes_of(&layout), vec![0, 1, 0]);
        assert_eq!(layout.lanes, 2);
        assert_eq!(layout.rows[0].links, vec![Link { from: 0, to: 0 }]);
        assert_eq!(
            layout.rows[1].links,
            vec![Link { from: 0, to: 0 }, Link { from: 1, to: 0 }],
            "le trait de `fix` oblique vers le couloir où `base` se dessine"
        );
    }

    #[test]
    fn given_a_merge_commit_when_it_is_laid_out_then_a_second_lane_opens_for_its_second_parent() {
        // Given — une fusion : un commit à deux parents, et c'est le second qui ouvre un
        // couloir. Le premier garde le sien, ce qui laisse `main` droit.
        let commits = vec![
            commit("merge", &["main", "side"]),
            commit("side", &["base"]),
            commit("main", &["base"]),
            commit("base", &[]),
        ];

        // When
        let layout = lay_out(&commits, NOW);

        // Then
        assert_eq!(layout.rows[0].lane, 0);
        assert_eq!(
            layout.rows[0].links,
            vec![Link { from: 0, to: 0 }, Link { from: 0, to: 1 }],
            "les deux traits partent du couloir de la fusion, l'un droit et l'autre oblique"
        );
        // `side` est bien dessiné dans le couloir que la fusion lui a ouvert, et `base` dans
        // celui qui l'attendait : un trait posé ne change jamais de colonne.
        assert_eq!(lanes_of(&layout), vec![0, 1, 0, 1]);
    }

    #[test]
    fn given_two_unrelated_branches_when_they_are_laid_out_then_each_keeps_its_own_lane() {
        // Given — deux histoires sans ancêtre commun dans la fenêtre. Rien ne les rapproche,
        // et rien ne doit les mêler.
        let commits = vec![
            commit("a2", &["a1"]),
            commit("b2", &["b1"]),
            commit("a1", &[]),
            commit("b1", &[]),
        ];

        // When
        let layout = lay_out(&commits, NOW);

        // Then
        assert_eq!(lanes_of(&layout), vec![0, 1, 0, 1]);
        assert_eq!(layout.lanes, 2);
    }

    #[test]
    fn given_a_branch_whose_parent_falls_outside_the_window_when_it_is_laid_out_then_its_lane_runs_off_the_bottom(
    ) {
        // Given — la fenêtre s'arrête avant l'ancêtre commun, ce qui est le cas normal d'un
        // dépôt de plusieurs milliers de commits. L'histoire est tronquée, et le dessin doit
        // le montrer plutôt que refermer un couloir qui continue.
        let commits = vec![commit("c", &["hors-fenêtre"])];

        // When
        let layout = lay_out(&commits, NOW);

        // Then
        assert_eq!(layout.rows[0].links, vec![Link { from: 0, to: 0 }]);
    }

    #[test]
    fn given_more_than_four_lanes_when_a_branch_has_been_untouched_for_a_month_then_it_is_folded_away(
    ) {
        // Given — six têtes ouvertes en même temps, dont deux abandonnées depuis plus de 30
        // jours. Chacune a son propre ancêtre hors fenêtre, donc chacune tient un couloir :
        // c'est la seule forme qui déclenche vraiment la règle de la spec §7.2, qui ne
        // s'applique qu'**au-delà** de quatre couloirs.
        let commits = vec![
            CommitBuilder::new("t1")
                .days_ago(1)
                .parents(&["p1"])
                .build(),
            CommitBuilder::new("t2")
                .days_ago(2)
                .parents(&["p2"])
                .build(),
            CommitBuilder::new("t3")
                .days_ago(3)
                .parents(&["p3"])
                .build(),
            CommitBuilder::new("t4")
                .days_ago(4)
                .parents(&["p4"])
                .build(),
            CommitBuilder::new("vieux")
                .named("wip/2024")
                .days_ago(90)
                .parents(&["p5"])
                .build(),
            CommitBuilder::new("ancien")
                .named("spike")
                .days_ago(200)
                .parents(&["p6"])
                .build(),
        ];

        // When
        let layout = lay_out(&commits, NOW);

        // Then — les deux vieilles branches ne sont plus dessinées, et elles sont nommées
        assert_eq!(layout.lanes, 4);
        assert_eq!(drawn(&commits, &layout), vec!["t1", "t2", "t3", "t4"]);
        assert_eq!(
            layout.folded,
            vec![
                FoldedBranch {
                    name: "wip/2024".to_owned(),
                    last_activity: NOW / 1_000 - 90 * DAY,
                },
                FoldedBranch {
                    name: "spike".to_owned(),
                    last_activity: NOW / 1_000 - 200 * DAY,
                },
            ]
        );
    }

    #[test]
    fn given_five_lanes_all_touched_this_week_when_they_are_laid_out_then_nothing_is_folded() {
        // Given — cinq worktrees actifs en parallèle, ce qui est exactement le mode de
        // travail d'Ash. Replier l'un d'eux effacerait le travail en cours.
        let commits = vec![
            CommitBuilder::new("t1")
                .days_ago(0)
                .parents(&["p1"])
                .build(),
            CommitBuilder::new("t2")
                .days_ago(1)
                .parents(&["p2"])
                .build(),
            CommitBuilder::new("t3")
                .days_ago(2)
                .parents(&["p3"])
                .build(),
            CommitBuilder::new("t4")
                .days_ago(3)
                .parents(&["p4"])
                .build(),
            CommitBuilder::new("t5")
                .days_ago(4)
                .parents(&["p5"])
                .build(),
        ];

        // When
        let layout = lay_out(&commits, NOW);

        // Then — cinq couloirs, assumés : la règle replie les branches inactives, pas les
        // branches en trop.
        assert_eq!(layout.lanes, 5);
        assert!(layout.folded.is_empty());
        assert_eq!(layout.rows.len(), 5);
    }

    #[test]
    fn given_a_repository_nobody_has_touched_for_a_year_when_it_is_laid_out_then_its_newest_branch_survives(
    ) {
        // Given — cinq têtes, toutes plus vieilles que 30 jours. Sans garde, la règle
        // replierait tout et le panneau montrerait un dessin vide au lieu d'une histoire.
        let commits = vec![
            CommitBuilder::new("t1")
                .days_ago(100)
                .parents(&["p1"])
                .build(),
            CommitBuilder::new("t2")
                .days_ago(200)
                .parents(&["p2"])
                .build(),
            CommitBuilder::new("t3")
                .days_ago(300)
                .parents(&["p3"])
                .build(),
            CommitBuilder::new("t4")
                .days_ago(400)
                .parents(&["p4"])
                .build(),
            CommitBuilder::new("t5")
                .days_ago(500)
                .parents(&["p5"])
                .build(),
        ];

        // When
        let layout = lay_out(&commits, NOW);

        // Then
        assert_eq!(drawn(&commits, &layout), vec!["t1"]);
        assert_eq!(layout.folded.len(), 4);
    }

    #[test]
    fn given_a_folded_branch_when_it_shares_history_with_a_kept_one_then_the_shared_commits_stay() {
        // Given — cinq têtes ouvertes, dont une abandonnée, toutes issues du même tronc.
        // Replier une branche ne doit pas emporter les commits que le reste du dépôt
        // contient aussi : ce serait effacer l'histoire commune pour cause de branche morte.
        let commits = vec![
            CommitBuilder::new("t1")
                .days_ago(1)
                .parents(&["a1"])
                .build(),
            CommitBuilder::new("t2")
                .days_ago(2)
                .parents(&["a2"])
                .build(),
            CommitBuilder::new("t3")
                .days_ago(3)
                .parents(&["a3"])
                .build(),
            CommitBuilder::new("t4")
                .days_ago(4)
                .parents(&["a4"])
                .build(),
            CommitBuilder::new("mort")
                .named("wip")
                .days_ago(120)
                .parents(&["a5"])
                .build(),
            CommitBuilder::new("a1")
                .days_ago(10)
                .parents(&["tronc"])
                .build(),
            CommitBuilder::new("a2")
                .days_ago(11)
                .parents(&["tronc"])
                .build(),
            CommitBuilder::new("a3")
                .days_ago(12)
                .parents(&["tronc"])
                .build(),
            CommitBuilder::new("a4")
                .days_ago(13)
                .parents(&["tronc"])
                .build(),
            CommitBuilder::new("a5")
                .days_ago(125)
                .parents(&["tronc"])
                .build(),
            CommitBuilder::new("tronc").days_ago(300).build(),
        ];

        // When
        let layout = lay_out(&commits, NOW);

        // Then — `tronc` reste, `mort` et son seul ancêtre propre partent
        let shown = drawn(&commits, &layout);
        assert!(shown.contains(&"tronc".to_owned()), "{shown:?}");
        assert!(!shown.contains(&"mort".to_owned()), "{shown:?}");
        assert!(!shown.contains(&"a5".to_owned()), "{shown:?}");
        assert_eq!(
            layout.folded,
            vec![FoldedBranch {
                name: "wip".to_owned(),
                last_activity: NOW / 1_000 - 120 * DAY,
            }]
        );
    }

    #[test]
    fn given_a_freed_lane_when_a_new_branch_appears_then_it_reuses_the_column_instead_of_widening()
    {
        // Given — une branche courte se referme, puis une autre naît plus bas. Sans réemploi,
        // le dessin s'élargirait indéfiniment sur un dépôt de plusieurs milliers de commits,
        // et la règle des quatre couloirs se déclencherait pour rien.
        let commits = vec![
            commit("head", &["base"]),
            commit("court", &[]),
            commit("base", &[]),
            commit("autre", &[]),
        ];

        // When
        let layout = lay_out(&commits, NOW);

        // Then — `autre` reprend un couloir libéré au lieu d'en ouvrir un troisième
        assert_eq!(lanes_of(&layout), vec![0, 1, 0, 0]);
        assert_eq!(layout.lanes, 2);
    }

    #[test]
    fn given_an_empty_window_when_it_is_laid_out_then_it_reserves_no_width() {
        // Given — un dépôt sans commit, ou un `git log` qui n'a pas répondu.
        let commits: Vec<GraphCommit> = Vec::new();

        // When
        let layout = lay_out(&commits, NOW);

        // Then
        assert_eq!(layout, Layout::default());
    }
}
