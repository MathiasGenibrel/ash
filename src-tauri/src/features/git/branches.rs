//! La liste de branches de la popup (spec §7.1) — groupée, ordonnée, et située.
//!
//! Deux choses la distinguent d'une liste de branches ordinaire, et les deux se calculent
//! ici :
//!
//! - **quelle branche vit dans quel worktree.** Deux worktrees ne peuvent pas être sur la
//!   même branche : la correspondance est donc une fonction, et c'est elle qui remplit la
//!   colonne de droite ([ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md)) ;
//! - **quels agents cette liste met en danger.** Un checkout déplace des fichiers sous les
//!   pieds de qui écrit dedans. La feature ne connaît pas les agents : elle en reçoit la
//!   liste par le port [`WorkingAgents`](super::working_agents::WorkingAgents), que le
//!   composition root branche sur le registre des onglets.
//!
//! **Le groupement et l'ordre vivent ici, en Rust, et le filtrage vit dans la webview.**
//! Ce n'est pas une inconséquence : ce sont deux natures de règles. Le groupement et l'ordre
//! sont des **faits du dépôt** — quelle branche est courante, laquelle a la pointe la plus
//! récente, laquelle est détenue ailleurs — et le frontend n'a pas le droit de les dériver
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le filtre, lui, est
//! une lecture de ce qui a déjà traversé : il change à chaque frappe, il ne demande rien au
//! disque, et le faire traverser la frontière ferait un aller-retour Tauri par caractère
//! tapé pour rendre un sous-ensemble d'une liste déjà en main.
//!
//! Rien ici n'invoque `git` : les deux sorties brutes arrivent par le port
//! [`BranchReader`](super::git_cli::BranchReader), et tout ce fichier est pur.

use std::path::Path;

use super::working_agents::BusyAgent;

/// De quel côté de la frontière vit une branche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum BranchKind {
    Local,
    Remote,
}

/// Le worktree qui détient une branche, quand ce n'est **pas** celui d'où l'on regarde.
///
/// `None` sur une branche libre : c'est ce qui distingue « je peux la prendre ici » de
/// « elle est prise ailleurs », et c'est toute l'information de la colonne de droite.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BranchWorktree {
    pub root: String,
    /// Le dernier segment du chemin — `ash-sidebar`, ce que la sidebar nomme déjà.
    pub name: String,
}

/// Une branche, telle qu'elle traverse la frontière.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    /// `feat/popup` pour une locale, `origin/feat/popup` pour une distante.
    pub name: String,
    pub kind: BranchKind,
    /// L'objet court de la pointe — `a1b2c3d`.
    pub tip: String,
    /// La date du commit de pointe, en secondes depuis l'époque Unix.
    ///
    /// Elle traverse **brute** parce que c'est un fait, et que la façon de l'écrire (`3 j`,
    /// `il y a 2 h`) est un choix d'affichage — le même arbitrage que `TabInfo::state_since`.
    ///
    /// **`number` et non `bigint`** : `ts-rs` prête un `bigint` à tout entier 64 bits, par
    /// prudence sur ce qui dépasse 2⁵³. Ce serait faux ici comme pour `TabInfo::state_since` —
    /// `serde_json` écrit un nombre JSON, que la webview lit en `number`, et un `bigint`
    /// déclaré mentirait sur ce qui arrive vraiment. Une date de commit en secondes ne
    /// s'approche pas de la borne.
    #[cfg_attr(test, ts(type = "number"))]
    pub committed_at: i64,
    pub worktree: Option<BranchWorktree>,
}

/// Les quatre groupes de la spec §7.1, dans l'ordre où ils s'affichent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum BranchGroup {
    /// La branche courante, **en tête et pas rangée dans l'ordre alphabétique**.
    Current,
    /// Les locales les plus fraîchement commitées, hors la courante.
    Recent,
    /// Le reste des locales, par ordre alphabétique.
    Local,
    /// Les distantes, par ordre alphabétique.
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BranchSection {
    pub group: BranchGroup,
    pub branches: Vec<Branch>,
}

/// Tout ce que la popup a besoin de savoir, en une seule réponse.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BranchOverview {
    pub worktree_root: String,
    /// La branche d'où l'on regarde. `None` sur un `HEAD` détaché — pendant un rebase, par
    /// exemple : il n'y a alors pas de groupe `current`, et rien à rebaser « depuis ».
    pub current: Option<String>,
    /// Les groupes **non vides**, dans l'ordre d'affichage. Un groupe sans branche
    /// n'apparaît pas : un titre `remote` seul sous une liste vide ne dit rien.
    pub sections: Vec<BranchSection>,
    /// Les agents qui écrivent dans **ce** worktree, et que tout geste sur l'arbre
    /// dérangerait (spec §7.1).
    ///
    /// C'est le champ qui fait la valeur de la popup : il **nomme** les agents, il ne dit
    /// pas qu'il y en a. Vide dans le cas courant.
    pub agents_at_risk: Vec<BusyAgent>,
}

/// Combien de locales le groupe `recent` retient.
///
/// Cinq, et pas dix : le groupe existe pour épargner un filtre à qui revient sur ce qu'il
/// vient de quitter, et une liste de « récentes » assez longue pour qu'on ait à la lire
/// n'épargne rien du tout. Au-delà, une branche retombe dans `local`, où elle reste
/// trouvable par son nom.
const RECENT: usize = 5;

/// Une ref telle que `for-each-ref` l'a écrite, avant tout classement.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawRef {
    tip: String,
    committed_at: i64,
    /// `*` pour le `HEAD` du worktree d'où la commande est partie.
    head: bool,
    /// Le nom complet — `refs/heads/feat/x`, `refs/remotes/origin/main`.
    refname: String,
}

/// Une entrée de `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawWorktree {
    root: String,
    /// La branche sur laquelle il est. `None` s'il est détaché.
    branch: Option<String>,
}

/// Lit la sortie de `for-each-ref`.
///
/// Le format est celui qu'on a imposé : quatre champs séparés par des tabulations. Une ligne
/// qui n'en porte pas quatre est **ignorée** plutôt que devinée — la popup montrera une
/// branche de moins, ce qui est visible, au lieu d'une branche fausse, qui ne l'est pas.
fn parse_refs(output: &str) -> Vec<RawRef> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let tip = fields.next()?;
            let committed_at = fields.next()?.trim().parse::<i64>().ok()?;
            let head = fields.next()? == "*";
            let refname = fields.next()?;
            (!refname.is_empty()).then(|| RawRef {
                tip: tip.to_owned(),
                committed_at,
                head,
                refname: refname.to_owned(),
            })
        })
        .collect()
}

/// Lit la sortie de `git worktree list --porcelain`.
///
/// Une entrée par bloc, séparée par une ligne vide : `worktree <chemin>`, puis `HEAD <sha>`,
/// puis `branch <ref>` ou `detached`. On ne retient que la première et la troisième.
fn parse_worktrees(output: &str) -> Vec<RawWorktree> {
    let mut found = Vec::new();
    let mut root: Option<String> = None;
    let mut branch: Option<String> = None;

    let mut flush = |root: &mut Option<String>, branch: &mut Option<String>| {
        if let Some(path) = root.take() {
            found.push(RawWorktree {
                root: path,
                branch: branch.take(),
            });
        }
        *branch = None;
    };

    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            flush(&mut root, &mut branch);
            root = Some(path.to_owned());
        } else if let Some(name) = line.strip_prefix("branch ") {
            branch = Some(name.to_owned());
        }
    }
    flush(&mut root, &mut branch);
    found
}

/// Le nom court d'une ref : `refs/heads/feat/x` → `feat/x`, `refs/remotes/origin/x` → `origin/x`.
///
/// `None` pour tout ce qui n'est ni une tête ni une distante — une note, une ref de rebase,
/// une ref d'un outil tiers. La popup montre des branches ; elle n'a pas à deviner ce
/// qu'est le reste.
fn short_name(refname: &str) -> Option<(String, BranchKind)> {
    if let Some(name) = refname.strip_prefix("refs/heads/") {
        return Some((name.to_owned(), BranchKind::Local));
    }
    refname
        .strip_prefix("refs/remotes/")
        .map(|name| (name.to_owned(), BranchKind::Remote))
}

/// Le dernier segment d'un chemin de worktree — ce que la sidebar nomme déjà.
fn worktree_name(root: &str) -> String {
    Path::new(root)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_owned())
}

/// Deux chemins désignent-ils le même worktree ?
///
/// La comparaison ignore un `/` final : `git worktree list` écrit le chemin tel qu'il l'a
/// enregistré, et la racine que le frontend renvoie vient de la résolution d'ADR-0012. Rien
/// de plus n'est tenté — canonicaliser demanderait le disque, et ce fichier est pur.
fn same_worktree(left: &str, right: &str) -> bool {
    left.trim_end_matches('/') == right.trim_end_matches('/')
}

/// Assemble la réponse de la popup à partir des deux sorties brutes de git.
///
/// `worktree_root` est celui d'où l'on regarde : c'est lui qui décide de la branche courante
/// et de ce que « la branche vit ailleurs » veut dire.
pub fn overview(
    worktree_root: &Path,
    refs_output: &str,
    worktrees_output: &str,
    agents_at_risk: Vec<BusyAgent>,
) -> BranchOverview {
    let here = worktree_root.display().to_string();
    let worktrees = parse_worktrees(worktrees_output);
    let refs = parse_refs(refs_output);

    // La branche courante vient de `git worktree list` **et** du marqueur `%(HEAD)`, dans
    // cet ordre — et cet ordre a une seconde conséquence, vérifiée sur un vrai dépôt : pendant
    // un rebase arrêté, `HEAD` est détaché (c'est ce que la ligne de statut affiche, lu dans
    // les fichiers de contrôle) mais `git worktree list` continue de nommer la branche en
    // cours de rebase. Les deux réponses sont vraies, et c'est celle-ci qui compte ici : c'est
    // elle qui dit qu'aucun autre worktree ne peut prendre cette branche, et c'est elle qui
    // donne à un libellé d'action son second côté.
    // Les deux, parce qu'aucun des deux ne suffit seul : la liste rend un chemin
    // qui peut ne pas s'écrire exactement comme celui qu'on nous a passé (lien symbolique,
    // `/tmp` contre `/private/tmp`), et le marqueur, lui, est toujours juste — mais il n'est
    // posé que si `git` est parti du bon worktree, ce que la liste vérifie.
    let current = worktrees
        .iter()
        .find(|worktree| same_worktree(&worktree.root, &here))
        .and_then(|worktree| worktree.branch.as_deref())
        .and_then(|refname| short_name(refname).map(|(name, _)| name))
        .or_else(|| {
            refs.iter()
                .find(|found| found.head)
                .and_then(|found| short_name(&found.refname).map(|(name, _)| name))
        });

    // Où vit chaque branche — mais **seulement quand ce n'est pas ici** : une branche prise
    // par le worktree d'où l'on regarde n'a rien à dire dans la colonne de droite, elle est
    // déjà la branche courante.
    let elsewhere: Vec<(String, BranchWorktree)> = worktrees
        .iter()
        .filter(|worktree| !same_worktree(&worktree.root, &here))
        .filter_map(|worktree| {
            let refname = worktree.branch.as_deref()?;
            let (name, _) = short_name(refname)?;
            Some((
                name,
                BranchWorktree {
                    root: worktree.root.clone(),
                    name: worktree_name(&worktree.root),
                },
            ))
        })
        .collect();

    let mut current_branch: Option<Branch> = None;
    let mut locals: Vec<Branch> = Vec::new();
    let mut remotes: Vec<Branch> = Vec::new();

    for found in refs {
        let Some((name, kind)) = short_name(&found.refname) else {
            continue;
        };
        // `origin/HEAD` n'est pas une branche : c'est un pointeur vers la branche par défaut
        // du distant, et le proposer ferait une entrée qui double une autre sous un faux nom.
        if kind == BranchKind::Remote && name.ends_with("/HEAD") {
            continue;
        }

        let branch = Branch {
            worktree: elsewhere
                .iter()
                .find(|(held, _)| *held == name)
                .map(|(_, where_)| where_.clone()),
            name: name.clone(),
            kind,
            tip: found.tip,
            committed_at: found.committed_at,
        };

        match kind {
            BranchKind::Remote => remotes.push(branch),
            BranchKind::Local if current.as_deref() == Some(name.as_str()) => {
                current_branch = Some(branch);
            }
            BranchKind::Local => locals.push(branch),
        }
    }

    // Les récentes : les plus fraîches d'abord, à égalité de date le nom pour trancher —
    // sans quoi l'ordre dépendrait de celui où git a rendu ses refs, et la liste sauterait
    // d'une ouverture à l'autre sans que rien n'ait changé.
    locals.sort_by(|left, right| {
        right
            .committed_at
            .cmp(&left.committed_at)
            .then_with(|| left.name.cmp(&right.name))
    });
    let rest = locals.split_off(locals.len().min(RECENT));
    let recent = locals;

    let mut others = rest;
    others.sort_by(|left, right| left.name.cmp(&right.name));
    remotes.sort_by(|left, right| left.name.cmp(&right.name));

    let sections = [
        (
            BranchGroup::Current,
            current_branch
                .map(|branch| vec![branch])
                .unwrap_or_default(),
        ),
        (BranchGroup::Recent, recent),
        (BranchGroup::Local, others),
        (BranchGroup::Remote, remotes),
    ]
    .into_iter()
    .filter(|(_, branches)| !branches.is_empty())
    .map(|(group, branches)| BranchSection { group, branches })
    .collect();

    BranchOverview {
        worktree_root: here,
        current,
        sections,
        agents_at_risk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::AgentState;
    use crate::features::git::working_agents::BusyAgentBuilder;

    /// De quoi écrire une sortie de `for-each-ref` sans la recopier ligne à ligne.
    #[derive(Default)]
    struct Refs(Vec<String>);

    impl Refs {
        fn local(mut self, name: &str, at: i64) -> Self {
            self.0.push(format!("a1b2c3d\t{at}\t \trefs/heads/{name}"));
            self
        }

        fn head(mut self, name: &str, at: i64) -> Self {
            self.0.push(format!("a1b2c3d\t{at}\t*\trefs/heads/{name}"));
            self
        }

        fn remote(mut self, name: &str, at: i64) -> Self {
            self.0
                .push(format!("a1b2c3d\t{at}\t \trefs/remotes/{name}"));
            self
        }

        fn build(self) -> String {
            self.0.join("\n")
        }
    }

    /// De quoi écrire une sortie de `git worktree list --porcelain`.
    #[derive(Default)]
    struct Worktrees(Vec<String>);

    impl Worktrees {
        fn on(mut self, root: &str, branch: &str) -> Self {
            self.0.push(format!(
                "worktree {root}\nHEAD a1b2c3d\nbranch refs/heads/{branch}\n"
            ));
            self
        }

        fn detached(mut self, root: &str) -> Self {
            self.0
                .push(format!("worktree {root}\nHEAD a1b2c3d\ndetached\n"));
            self
        }

        fn build(self) -> String {
            self.0.join("\n")
        }
    }

    fn names(overview: &BranchOverview, group: BranchGroup) -> Vec<&str> {
        overview
            .sections
            .iter()
            .find(|section| section.group == group)
            .map(|section| {
                section
                    .branches
                    .iter()
                    .map(|branch| branch.name.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn given_a_repository_with_several_branches_when_the_popup_asks_for_them_then_the_current_one_comes_first(
    ) {
        // Given
        let refs = Refs::default()
            .local("zeta", 100)
            .head("main", 50)
            .local("alpha", 200)
            .build();
        let worktrees = Worktrees::default().on("/dev/ash", "main").build();

        // When
        let shown = overview(Path::new("/dev/ash"), &refs, &worktrees, Vec::new());

        // Then — en tête, et pas rangée dans l'ordre alphabétique (spec §7.1)
        assert_eq!(shown.current.as_deref(), Some("main"));
        assert_eq!(
            shown.sections.first().map(|section| section.group),
            Some(BranchGroup::Current)
        );
        assert_eq!(names(&shown, BranchGroup::Current), vec!["main"]);
    }

    #[test]
    fn given_more_local_branches_than_the_recent_group_holds_when_they_are_grouped_then_the_freshest_are_recent_and_the_rest_alphabetical(
    ) {
        // Given — sept locales hors la courante, de la plus vieille à la plus fraîche
        let mut refs = Refs::default().head("main", 1000);
        for (position, name) in ["g", "f", "e", "d", "c", "b", "a"].iter().enumerate() {
            let at = 100 + i64::try_from(position).unwrap_or(0);
            refs = refs.local(name, at);
        }
        let worktrees = Worktrees::default().on("/dev/ash", "main").build();

        // When
        let shown = overview(Path::new("/dev/ash"), &refs.build(), &worktrees, Vec::new());

        // Then — les cinq plus fraîches, dans l'ordre des dates
        assert_eq!(
            names(&shown, BranchGroup::Recent),
            vec!["a", "b", "c", "d", "e"]
        );
        // And — le reste retombe dans `local`, par ordre alphabétique
        assert_eq!(names(&shown, BranchGroup::Local), vec!["f", "g"]);
    }

    #[test]
    fn given_a_branch_checked_out_in_another_worktree_when_it_is_listed_then_it_names_that_worktree(
    ) {
        // Given
        let refs = Refs::default()
            .head("main", 100)
            .local("feat/sidebar", 200)
            .build();
        let worktrees = Worktrees::default()
            .on("/dev/ash", "main")
            .on("/wt/ash-sidebar", "feat/sidebar")
            .build();

        // When
        let shown = overview(Path::new("/dev/ash"), &refs, &worktrees, Vec::new());

        // Then — c'est toute la colonne de droite de la spec §7.1
        let held = shown
            .sections
            .iter()
            .flat_map(|section| &section.branches)
            .find(|branch| branch.name == "feat/sidebar")
            .and_then(|branch| branch.worktree.clone());
        assert_eq!(
            held,
            Some(BranchWorktree {
                root: "/wt/ash-sidebar".to_owned(),
                name: "ash-sidebar".to_owned(),
            })
        );
    }

    #[test]
    fn given_the_branch_of_the_worktree_we_are_looking_from_when_it_is_listed_then_no_worktree_is_named(
    ) {
        // Given
        let refs = Refs::default().head("main", 100).build();
        let worktrees = Worktrees::default()
            .on("/dev/ash/", "main")
            .on("/wt/ash-sidebar", "feat/sidebar")
            .build();

        // When — le chemin est écrit avec un `/` final d'un côté, sans de l'autre
        let shown = overview(Path::new("/dev/ash"), &refs, &worktrees, Vec::new());

        // Then — « elle vit ailleurs » serait faux : elle est ici, c'est la courante
        let current = shown
            .sections
            .iter()
            .flat_map(|section| &section.branches)
            .find(|branch| branch.name == "main");
        assert_eq!(current.and_then(|branch| branch.worktree.clone()), None);
    }

    #[test]
    fn given_a_worktree_that_holds_no_branch_at_all_when_the_popup_asks_for_its_branches_then_there_is_no_current_one(
    ) {
        // Given — un `git checkout <sha>` : le worktree ne détient aucune branche. C'est
        // distinct d'un rebase arrêté, où `git worktree list` en nomme encore une.
        let refs = Refs::default().local("main", 100).build();
        let worktrees = Worktrees::default().detached("/dev/ash").build();

        // When
        let shown = overview(Path::new("/dev/ash"), &refs, &worktrees, Vec::new());

        // Then — rien à rebaser « depuis », et pas de groupe `current` fabriqué de toutes pièces
        assert_eq!(shown.current, None);
        assert!(names(&shown, BranchGroup::Current).is_empty());
        assert_eq!(names(&shown, BranchGroup::Recent), vec!["main"]);
    }

    #[test]
    fn given_a_remote_that_publishes_its_default_branch_when_the_remotes_are_listed_then_origin_head_is_not_one_of_them(
    ) {
        // Given
        let refs = Refs::default()
            .head("main", 100)
            .remote("origin/HEAD", 100)
            .remote("origin/main", 100)
            .build();
        let worktrees = Worktrees::default().on("/dev/ash", "main").build();

        // When
        let shown = overview(Path::new("/dev/ash"), &refs, &worktrees, Vec::new());

        // Then — `origin/HEAD` est un pointeur, pas une branche : il doublerait `origin/main`
        assert_eq!(names(&shown, BranchGroup::Remote), vec!["origin/main"]);
    }

    #[test]
    fn given_a_ref_that_is_neither_a_head_nor_a_remote_when_the_branches_are_listed_then_it_is_left_out(
    ) {
        // Given — une ref de rebase en cours, que `refs/heads` ne recouvre pas
        let refs = format!(
            "{}\na1b2c3d\t100\t \trefs/rewritten/onto",
            Refs::default().head("main", 100).build()
        );
        let worktrees = Worktrees::default().on("/dev/ash", "main").build();

        // When
        let shown = overview(Path::new("/dev/ash"), &refs, &worktrees, Vec::new());

        // Then
        let all: Vec<&str> = shown
            .sections
            .iter()
            .flat_map(|section| &section.branches)
            .map(|branch| branch.name.as_str())
            .collect();
        assert_eq!(all, vec!["main"]);
    }

    #[test]
    fn given_a_truncated_line_from_git_when_the_refs_are_read_then_it_is_dropped_rather_than_guessed(
    ) {
        // Given — une ligne à trois champs au lieu de quatre
        let refs = "a1b2c3d\t100\t \trefs/heads/main\na1b2c3d\t100\trefs/heads/broken";

        // When
        let read = parse_refs(refs);

        // Then — une branche de moins se voit ; une branche fausse ne se voit pas
        assert_eq!(read.len(), 1);
        assert_eq!(
            read.first().map(|found| found.refname.as_str()),
            Some("refs/heads/main")
        );
    }

    #[test]
    fn given_an_agent_writing_in_this_worktree_when_the_popup_asks_for_the_branches_then_it_travels_with_them(
    ) {
        // Given
        let claude = BusyAgentBuilder::new()
            .name("claude")
            .state(AgentState::Working)
            .build();
        let refs = Refs::default().head("main", 100).build();
        let worktrees = Worktrees::default().on("/dev/ash", "main").build();

        // When
        let shown = overview(
            Path::new("/dev/ash"),
            &refs,
            &worktrees,
            vec![claude.clone()],
        );

        // Then — la liste et l'avertissement sont lus au même instant : deux réponses
        // laisseraient la popup nommer un agent qui vient de finir
        assert_eq!(shown.agents_at_risk, vec![claude]);
    }
}
