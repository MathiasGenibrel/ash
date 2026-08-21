//! Ce que la ligne de statut montre, **et dans quel ordre** — les sept interrupteurs du menu
//! contextuel de la vue 5c, et le mode édition de la vue 5e (spec §4.2).
//!
//! **Septième préférence du même fichier**, et pour les raisons qui y ont déjà mis le
//! panneau bas : c'est une préférence d'apparence de la fenêtre, elle décide de ce qui
//! s'affiche, elle survit à la fermeture, et elle se relit au même moment que les six
//! autres. Le partage avec l'autre magasin de préférences du dépôt est net :
//! `features/agents/preferences.rs` détient ce que le **superviseur** consulte au moment
//! d'interrompre — il n'a pas de fenêtre à qui demander —, tandis que tout ce qu'une
//! **fenêtre rend** est ici.
//!
//! Ce qui est détenu, ce sont les **choix**, jamais le dessin : le retrait automatique de la
//! ligne trop étroite reste dans `src/features/terminal/terminal.css`, sous ses `@container`.
//! Les deux règles cohabitent sans se connaître — l'une dit ce que l'utilisateur veut lire,
//! l'autre ce qui tient dans la place restante.
//!
//! **La spec disait « par fenêtre » ; c'est amendé** (2026-08-21) : la phrase visait à
//! exclure un réglage par **onglet**, et réorganiser sa barre à chaque lancement n'est pas un
//! réglage. Le choix vit donc en Rust et dans `~/.ash/theme.json`
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! # Ce qui a changé de nature en #165, et pourquoi
//!
//! #164 gardait **sept booléens nommés**. La vue 5e demande un **ordre**, et des `spacer`s en
//! nombre libre : un `cwd: true` ne dit pas où est le `cwd`, et il n'y a pas de champ nommé
//! pour « le troisième élastique ». Ce qui est détenu est donc désormais une **suite
//! ordonnée** — [`StatusBarLayout`] —, et la visibilité y devient une **appartenance** : un
//! segment est montré s'il est dans la liste, retiré s'il n'y est pas. C'est le modèle de la
//! maquette, dont l'état tient dans un seul tableau, et c'est ce qui évite d'avoir deux
//! vérités — une liste qui dirait la place et des booléens qui diraient la présence
//! finiraient par se contredire, et il faudrait un arbitre.
//!
//! Le prix se paie sur la **compatibilité ascendante**, et il faut le nommer : un champ
//! absent valait « montré », un élément absent d'une liste vaut « masqué ». Un fichier écrit
//! par une version antérieure ne se relit donc **pas** comme une liste, et c'est
//! [`Stored`] qui s'en charge — il accepte les deux formes, et convertit l'ancienne en
//! plaçant chaque segment coché à sa place canonique. Aucun choix n'est perdu ; ce qui est
//! perdu, c'est la possibilité d'ajouter un huitième segment en le supposant montré chez
//! ceux qui n'en ont jamais entendu parler. Le remède est écrit dans [`FULL_ORDER`] : un
//! nouveau segment y prend sa place, et une **migration** l'insère chez qui ne l'a pas.

use serde::de::Deserializer;
use serde::{Deserialize, Serialize, Serializer};

/// Les sept segments de la ligne, dans l'ordre du menu de la vue 5c.
///
/// L'ordre est écrit ici **et** dans `MENU_ORDER` (`src/features/terminal/status-bar.ts`), et
/// rien ne les apparie : ce qui traverse la frontière est un identifiant, et le miroir de
/// `mirror.ts` garantit l'**ensemble** des sept noms, jamais leur suite. L'ordre du menu est
/// une décision de présentation, et c'est le frontend qui la porte ; celui-ci n'existe que
/// pour se lire — `session`, `weekly`, `context`, `model`, puis — après le trait — `agent`,
/// `branch`, `cwd`. Les quatre premiers parlent de ce que la conversation consomme, les
/// trois derniers d'où l'on est et de ce que l'agent fait.
///
/// **Ce n'est pas l'ordre de la barre** : celui-là est [`FULL_ORDER`], et il se règle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum StatusBarSegment {
    /// La pastille du quota de session — `s 63% · 2h14`.
    Session,
    /// La pastille du quota hebdomadaire — **retirée par défaut** (spec §4.2).
    Weekly,
    /// La jauge de contexte et son libellé, qui ne se séparent pas : le libellé double la
    /// barre, et une barre sans son chiffre ne se lit plus.
    Context,
    /// Le nom court du modèle qui tourne — `Opus 5 1M`.
    Model,
    /// Le glyphe d'état de l'agent, son processus et sa durée.
    Agent,
    /// La branche, l'opération en cours et l'état de l'arbre — un seul segment, comme dans
    /// la ligne : l'opération et les compteurs qualifient la branche qui les porte.
    Branch,
    /// Le répertoire de l'onglet actif.
    Cwd,
}

impl StatusBarSegment {
    /// Les sept, énumérés. Rien du produit ne les parcourt — c'est le frontend qui dessine
    /// la liste ; cette table sert à interroger les défauts d'un seul geste.
    pub const ALL: [StatusBarSegment; 7] = [
        StatusBarSegment::Session,
        StatusBarSegment::Weekly,
        StatusBarSegment::Context,
        StatusBarSegment::Model,
        StatusBarSegment::Agent,
        StatusBarSegment::Branch,
        StatusBarSegment::Cwd,
    ];
}

/// Ce que la barre porte à une place donnée : un segment, ou un **élastique**.
///
/// Le spacer est ici et non dans [`StatusBarSegment`] parce qu'il n'a pas la même nature : un
/// segment a une identité, il y en a exactement un de chaque, et le menu contextuel le
/// nomme ; un spacer n'a pas d'identité du tout — il y en a zéro, un ou cinq, et deux
/// spacers ne se distinguent que par leur place. C'est aussi pour ça qu'il ne se **bascule**
/// pas : `toggle_status_bar_segment` prend un [`StatusBarSegment`], et l'ajout d'un spacer
/// passe par `set_status_bar_layout`, comme un déplacement.
///
/// Sur le fil et sur le disque, c'est une **chaîne** : les sept noms de segments, plus
/// `"spacer"`. Un tableau de mots se relit à l'œil nu dans `~/.ash/theme.json`, ce qui est la
/// moitié de l'intérêt d'y écrire des préférences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum StatusBarItem {
    Session,
    Weekly,
    Context,
    Model,
    Agent,
    Branch,
    Cwd,
    /// L'élastique de la maquette — `flex: 1`. Hors édition, c'est un espace : il n'a ni
    /// bordure, ni libellé, ni `×`.
    Spacer,
}

impl StatusBarItem {
    /// Le segment que cet élément porte, ou `None` pour un spacer.
    ///
    /// L'appariement des sept noms est écrit **deux fois** — ici et dans [`From`] —, ce qui
    /// est exactement le genre de duplication qui compile en disant autre chose que ce qu'on
    /// croit. Elle est tenue par un seul test, `given_every_segment_when_it_becomes_an_item_then_it_reads_back_as_itself`,
    /// qui parcourt [`StatusBarSegment::ALL`] : une paire de travers dans l'un des deux
    /// `match` s'y voit, dans les deux sens.
    #[must_use]
    pub fn segment(self) -> Option<StatusBarSegment> {
        match self {
            StatusBarItem::Session => Some(StatusBarSegment::Session),
            StatusBarItem::Weekly => Some(StatusBarSegment::Weekly),
            StatusBarItem::Context => Some(StatusBarSegment::Context),
            StatusBarItem::Model => Some(StatusBarSegment::Model),
            StatusBarItem::Agent => Some(StatusBarSegment::Agent),
            StatusBarItem::Branch => Some(StatusBarSegment::Branch),
            StatusBarItem::Cwd => Some(StatusBarSegment::Cwd),
            StatusBarItem::Spacer => None,
        }
    }
}

impl From<StatusBarSegment> for StatusBarItem {
    fn from(segment: StatusBarSegment) -> Self {
        match segment {
            StatusBarSegment::Session => StatusBarItem::Session,
            StatusBarSegment::Weekly => StatusBarItem::Weekly,
            StatusBarSegment::Context => StatusBarItem::Context,
            StatusBarSegment::Model => StatusBarItem::Model,
            StatusBarSegment::Agent => StatusBarItem::Agent,
            StatusBarSegment::Branch => StatusBarItem::Branch,
            StatusBarSegment::Cwd => StatusBarItem::Cwd,
        }
    }
}

/// La barre **entière**, spacer compris, dans l'ordre où la maquette la dessine.
///
/// Deux rôles en une seule table, et c'est ce qui les garde d'accord :
///
/// - elle donne la disposition de départ, une fois le weekly retiré — voir [`Default`] ;
/// - elle donne à chaque segment sa **place canonique**, celle où il revient quand on le
///   recoche. Sans elle, décocher `cwd` puis le recocher le renverrait à l'extrémité droite
///   de la barre, et la promesse de #164 — « cocher ne change que cette ligne » — serait
///   fausse dès qu'on a réorganisé quoi que ce soit.
const FULL_ORDER: [StatusBarItem; 8] = [
    StatusBarItem::Cwd,
    StatusBarItem::Branch,
    StatusBarItem::Agent,
    StatusBarItem::Spacer,
    StatusBarItem::Session,
    StatusBarItem::Weekly,
    StatusBarItem::Context,
    StatusBarItem::Model,
];

/// Le seul segment que la barre ne montre pas à la première ouverture (spec §4.2) : la ligne
/// n'a la place que d'un quota, et le popover est précisément là pour montrer l'autre.
const HIDDEN_BY_DEFAULT: StatusBarItem = StatusBarItem::Weekly;

/// La disposition de la ligne de statut : ce qu'elle montre, et dans quel ordre (spec §4.2,
/// vues 5c et 5e).
///
/// **Une suite, et non un enregistrement de booléens** : voir l'en-tête du module. La
/// visibilité d'un segment est son appartenance à cette suite.
///
/// L'invariant, tenu par [`StatusBarLayout::normalized`], est qu'un **segment y figure au
/// plus une fois** — deux `cwd` dans la barre n'auraient aucun sens, et le second ne
/// pourrait plus se distinguer du premier au moment de le retirer. Les spacers, eux, n'ont
/// pas d'identité : ils se répètent autant qu'on veut.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct StatusBarLayout(Vec<StatusBarItem>);

impl Default for StatusBarLayout {
    fn default() -> Self {
        Self(
            FULL_ORDER
                .into_iter()
                .filter(|item| *item != HIDDEN_BY_DEFAULT)
                .collect(),
        )
    }
}

impl StatusBarLayout {
    /// La disposition proposée par la webview, ramenée à ses invariants.
    ///
    /// **Elle ne refuse jamais** — elle nettoie : un nom qu'un backend plus récent aurait
    /// connu, un segment répété par un glissement mal joué, sont retirés en silence. Refuser
    /// laisserait la barre dans l'état d'avant sans un mot, ce qui se lit comme une panne ;
    /// nettoyer rend une barre qu'on peut regarder.
    ///
    /// Une suite **vide** est valide, et c'est un critère de la tâche : on peut tout jeter.
    /// C'est [`reset`](Self::reset) qui rend la barre par défaut, et le tiroir du mode
    /// édition qui l'offre — une barre vide reste donc récupérable.
    #[must_use]
    pub fn normalized(items: Vec<StatusBarItem>) -> Self {
        let mut kept: Vec<StatusBarItem> = Vec::with_capacity(items.len());
        for item in items {
            if item != StatusBarItem::Spacer && kept.contains(&item) {
                continue;
            }
            kept.push(item);
        }
        Self(kept)
    }

    /// La disposition d'origine — le `reset all` des raccourcis (spec §4.4), appliqué à la
    /// barre.
    #[must_use]
    pub fn reset() -> Self {
        Self::default()
    }

    /// Ce que la barre porte, dans l'ordre.
    #[must_use]
    pub fn items(&self) -> &[StatusBarItem] {
        &self.0
    }

    /// Ce segment est-il montré ? — c'est-à-dire : est-il dans la barre ?
    #[must_use]
    pub fn shows(&self, segment: StatusBarSegment) -> bool {
        self.0.contains(&StatusBarItem::from(segment))
    }

    /// La même barre, ce segment-là retiré s'il y était, remis à sa place canonique sinon.
    ///
    /// **Une bascule et non une valeur posée** : le menu montre ce que le backend détient,
    /// donc il demande un changement plutôt que d'annoncer un état qu'il aurait lu juste
    /// avant. C'est ce qui empêche deux panneaux ouverts coup sur coup de se répondre le
    /// même booléen ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) —
    /// la conduite de `toggle_sidebar_column`, pour la même raison.
    ///
    /// « À sa place canonique » veut dire : **auprès de ses voisins**, tels que
    /// [`FULL_ORDER`] les nomme — juste après le plus proche de ceux qui le précèdent et qui
    /// est encore dans la barre, à défaut juste devant le plus proche de ceux qui le suivent,
    /// à défaut au bout. Un segment recoché revient donc là où l'utilisateur s'attend à le
    /// trouver, et non à l'extrémité droite de la ligne.
    ///
    /// Les voisins, et non une position absolue, parce que la barre se **réorganise** : dans
    /// une barre où le modèle est passé en tête, `cwd` n'est plus le premier élément, et
    /// « la place de `cwd` » ne veut plus rien dire dans l'absolu. Ce qui reste vrai, c'est
    /// que la branche suit le `cwd`. C'est ce qui rend un `×` mal visé sans conséquence :
    /// décocher puis recocher rend la barre d'avant partout où le voisin d'origine est
    /// resté.
    #[must_use]
    pub fn toggled(&self, segment: StatusBarSegment) -> Self {
        let target = StatusBarItem::from(segment);
        if self.shows(segment) {
            return Self(
                self.0
                    .iter()
                    .copied()
                    .filter(|item| *item != target)
                    .collect(),
            );
        }

        let mut items = self.0.clone();
        items.insert(self.canonical_index(target), target);
        Self(items)
    }

    /// Où revient un segment qu'on recoche — voir [`toggled`](Self::toggled).
    ///
    /// Les voisins sont interrogés **du plus proche au plus lointain** : un `cwd` qui revient
    /// se pose devant la branche s'il y en a une, devant l'état d'agent sinon, et ainsi de
    /// suite. Chercher le plus proche d'abord est ce qui donne une place plausible dans une
    /// barre à laquelle il manque la moitié des segments.
    ///
    /// Le voisin de gauche est cherché par la **fin** de la barre : c'est sans effet pour un
    /// segment, qui n'y figure qu'une fois, et c'est ce qui pose un `session` derrière le
    /// **dernier** élastique plutôt que derrière le premier quand il y en a plusieurs.
    fn canonical_index(&self, target: StatusBarItem) -> usize {
        let place = FULL_ORDER.iter().position(|item| *item == target);
        let (before, after) = FULL_ORDER.split_at(place.unwrap_or(FULL_ORDER.len()));

        for neighbour in before.iter().rev() {
            if let Some(index) = self.0.iter().rposition(|item| item == neighbour) {
                return index + 1;
            }
        }
        for neighbour in after.iter().skip(1) {
            if let Some(index) = self.0.iter().position(|item| item == neighbour) {
                return index;
            }
        }
        self.0.len()
    }
}

/* ------------------------------------------------------------------------------------- *
 * Le fichier — deux formes acceptées, une seule écrite.
 * ------------------------------------------------------------------------------------- */

impl Serialize for StatusBarLayout {
    /// Toujours la forme nouvelle : un tableau de mots.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StatusBarLayout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Stored::deserialize(deserializer)?.into())
    }
}

/// Les formes qu'un `theme.json` peut porter sous la clé `status_bar`.
///
/// `#[serde(untagged)]` suffit à distinguer les deux premières, et sans ambiguïté : un
/// tableau JSON ne peut pas se lire comme un objet, ni l'inverse. L'ordre des variantes
/// n'est donc pas une règle de priorité déguisée — mais la nouvelle est écrite d'abord,
/// parce que c'est la seule qu'Ash écrit encore.
///
/// La troisième n'est pas une forme : c'est un **filet**. Sans elle, un `status_bar`
/// bricolé à la main ou écrit par un Ash plus récent ferait échouer la lecture de tout
/// `Appearance` — et coûterait le thème, la police et la largeur de colonne qui l'entourent.
/// Une barre incompréhensible ne coûte donc que la barre.
#[derive(Deserialize)]
#[serde(untagged)]
enum Stored {
    /// Ce qu'Ash écrit depuis #165 : la barre entière, dans l'ordre.
    Ordered(Vec<StatusBarItem>),
    /// Ce qu'Ash écrivait en #164 : sept booléens nommés, sans ordre.
    Switches(Switches),
    /// Tout le reste — la barre repart des défauts, le fichier garde le sien.
    Unreadable(serde::de::IgnoredAny),
}

impl From<Stored> for StatusBarLayout {
    fn from(stored: Stored) -> Self {
        match stored {
            Stored::Ordered(items) => Self::normalized(items),
            Stored::Switches(switches) => switches.into(),
            Stored::Unreadable(_) => Self::default(),
        }
    }
}

/// La forme de #164 : un booléen par segment, nommé.
///
/// **Elle ne se lit plus que pour être convertie**, et rien n'y écrit : c'est la mémoire d'un
/// format, pas un état du produit. Chaque champ garde son `#[serde(default)]`, et c'est ce
/// qui fait tenir la promesse d'alors — un fichier écrit avant qu'un segment soit coupable se
/// relit sans le masquer.
///
/// Le sens de la conversion est le seul possible : les booléens disent **quoi**, jamais
/// **où**, donc la place vient de [`FULL_ORDER`]. Un utilisateur qui avait décoché son `cwd`
/// retrouve une barre sans `cwd`, et le reste dans l'ordre d'origine — c'est-à-dire
/// exactement ce qu'il voyait avant la mise à jour.
#[derive(Deserialize)]
struct Switches {
    #[serde(default = "shown")]
    session: bool,
    #[serde(default)]
    weekly: bool,
    #[serde(default = "shown")]
    context: bool,
    #[serde(default = "shown")]
    model: bool,
    #[serde(default = "shown")]
    agent: bool,
    #[serde(default = "shown")]
    branch: bool,
    #[serde(default = "shown")]
    cwd: bool,
}

/// Les segments qu'un fichier de #164 montrait sans le dire.
fn shown() -> bool {
    true
}

impl Switches {
    fn shows(&self, item: StatusBarItem) -> bool {
        match item {
            StatusBarItem::Session => self.session,
            StatusBarItem::Weekly => self.weekly,
            StatusBarItem::Context => self.context,
            StatusBarItem::Model => self.model,
            StatusBarItem::Agent => self.agent,
            StatusBarItem::Branch => self.branch,
            StatusBarItem::Cwd => self.cwd,
            // L'élastique n'était pas un objet en #164 : il était le `flex: 1` du CSS, donc
            // toujours là, et toujours au même endroit. Il le reste après conversion.
            StatusBarItem::Spacer => true,
        }
    }
}

impl From<Switches> for StatusBarLayout {
    fn from(switches: Switches) -> Self {
        Self(
            FULL_ORDER
                .into_iter()
                .filter(|item| switches.shows(*item))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La clé `status_bar` d'un `theme.json`, relue comme `Appearance` la relit.
    fn parse(json: &str) -> StatusBarLayout {
        serde_json::from_str(json).expect("la clé se relit toujours, quelle qu'en soit la forme")
    }

    #[test]
    fn given_every_segment_when_it_becomes_an_item_then_it_reads_back_as_itself() {
        // Given — les deux `match` qui apparient un segment à un élément de barre sont
        // écrits séparément, et une paire de travers compilerait sans un mot
        let segments = StatusBarSegment::ALL;

        // When
        let round_trip: Vec<Option<StatusBarSegment>> = segments
            .into_iter()
            .map(|segment| StatusBarItem::from(segment).segment())
            .collect();

        // Then
        let expected: Vec<Option<StatusBarSegment>> = segments.into_iter().map(Some).collect();
        assert_eq!(round_trip, expected);
    }

    #[test]
    fn given_a_first_launch_when_the_line_asks_what_it_shows_then_only_the_weekly_quota_is_missing()
    {
        // Given — les défauts de la spec §4.2, et le seul endroit du produit qui les porte :
        // la ligne n'a la place que d'un quota, et le popover est là pour montrer l'autre
        let defaults = StatusBarLayout::default();

        // When
        let hidden: Vec<StatusBarSegment> = StatusBarSegment::ALL
            .into_iter()
            .filter(|segment| !defaults.shows(*segment))
            .collect();

        // Then
        assert_eq!(hidden, vec![StatusBarSegment::Weekly]);
    }

    #[test]
    fn given_a_first_launch_when_the_bar_is_laid_out_then_the_spacer_sits_between_the_two_halves() {
        // Given / When — la disposition de la vue 5e : `cwd · branch · agent · ⟷ · session ·
        // context · model`
        let defaults = StatusBarLayout::default();

        // Then — l'élastique est un objet de la liste, pas une règle de CSS : c'est lui qui
        // pousse l'usage à droite, et c'est lui qu'on pourra déplacer
        assert_eq!(
            defaults.items(),
            [
                StatusBarItem::Cwd,
                StatusBarItem::Branch,
                StatusBarItem::Agent,
                StatusBarItem::Spacer,
                StatusBarItem::Session,
                StatusBarItem::Context,
                StatusBarItem::Model,
            ]
        );
    }

    #[test]
    fn given_a_shown_segment_when_it_is_toggled_then_it_is_the_only_one_that_leaves() {
        // Given — le `cwd`, montré comme les cinq autres
        let before = StatusBarLayout::default();

        // When — l'utilisateur le décoche dans le menu
        let after = before.toggled(StatusBarSegment::Cwd);

        // Then — la branche et l'état de l'agent restent en place, dans le même ordre
        assert!(!after.shows(StatusBarSegment::Cwd));
        assert_eq!(
            after.items(),
            [
                StatusBarItem::Branch,
                StatusBarItem::Agent,
                StatusBarItem::Spacer,
                StatusBarItem::Session,
                StatusBarItem::Context,
                StatusBarItem::Model,
            ]
        );
    }

    #[test]
    fn given_a_reordered_bar_when_a_segment_is_unchecked_and_checked_again_then_it_comes_back_where_it_was(
    ) {
        // Given — une barre que l'utilisateur a réorganisée : le modèle est passé en tête,
        // et rien ne doit défaire ça
        let arranged = StatusBarLayout::normalized(vec![
            StatusBarItem::Model,
            StatusBarItem::Cwd,
            StatusBarItem::Branch,
            StatusBarItem::Spacer,
            StatusBarItem::Session,
        ]);

        // When — un `×` mal visé sur la branche, puis le geste de rattrapage
        let restored = arranged
            .toggled(StatusBarSegment::Branch)
            .toggled(StatusBarSegment::Branch);

        // Then — la barre est celle d'avant : c'est ce qui rend le `×` du mode édition
        // sans conséquence, et ce qui tient la promesse de #164 sur une barre réorganisée
        assert_eq!(restored, arranged);
    }

    #[test]
    fn given_a_bar_emptied_of_everything_when_a_segment_comes_back_then_it_is_the_whole_bar() {
        // Given — le critère « une barre vidée reste récupérable » : la vider est permis
        let empty = StatusBarLayout::normalized(vec![]);

        // When
        let after = empty.toggled(StatusBarSegment::Agent);

        // Then — un segment sans voisin de droite se pose au bout, et la barre existe de
        // nouveau sans passer par le retour aux défauts
        assert!(empty.items().is_empty());
        assert_eq!(after.items(), [StatusBarItem::Agent]);
    }

    #[test]
    fn given_a_layout_proposed_with_a_repeated_segment_when_it_is_normalized_then_only_the_first_survives(
    ) {
        // Given — un glissement mal joué, ou une webview plus vieille : `cwd` deux fois, et
        // deux élastiques qui, eux, ont le droit de coexister
        let proposed = vec![
            StatusBarItem::Cwd,
            StatusBarItem::Spacer,
            StatusBarItem::Cwd,
            StatusBarItem::Spacer,
            StatusBarItem::Agent,
        ];

        // When
        let layout = StatusBarLayout::normalized(proposed);

        // Then — un segment a une identité, un spacer n'en a pas
        assert_eq!(
            layout.items(),
            [
                StatusBarItem::Cwd,
                StatusBarItem::Spacer,
                StatusBarItem::Spacer,
                StatusBarItem::Agent,
            ]
        );
    }

    #[test]
    fn given_a_preference_file_written_by_the_version_before_when_it_is_read_then_the_bar_is_the_one_it_showed(
    ) {
        // Given — un `theme.json` écrit par #164 : sept booléens nommés, le `cwd` décoché,
        // le weekly rallumé, et trois champs qui manquent parce qu'ils sont plus récents
        let older = r#"{ "session": true, "weekly": true, "cwd": false }"#;

        // When
        let layout = parse(older);

        // Then — la barre est exactement celle qu'il voyait : son `cwd` retiré, son weekly
        // à sa place, et le reste dans l'ordre d'origine. Une mise à jour d'Ash ne réorganise
        // rien et ne rallume rien.
        assert_eq!(
            layout.items(),
            [
                StatusBarItem::Branch,
                StatusBarItem::Agent,
                StatusBarItem::Spacer,
                StatusBarItem::Session,
                StatusBarItem::Weekly,
                StatusBarItem::Context,
                StatusBarItem::Model,
            ]
        );
    }

    #[test]
    fn given_a_bar_written_by_ash_when_it_is_read_back_then_it_is_the_same_bar() {
        // Given — le fichier est le seul lien entre deux sessions (critère : l'ordre, les
        // éléments retirés et les spacers survivent à un redémarrage)
        let arranged = StatusBarLayout::normalized(vec![
            StatusBarItem::Session,
            StatusBarItem::Spacer,
            StatusBarItem::Cwd,
            StatusBarItem::Spacer,
            StatusBarItem::Spacer,
            StatusBarItem::Agent,
        ]);

        // When
        let written = serde_json::to_string(&arranged).expect("la barre se sérialise");
        let read = parse(&written);

        // Then
        assert_eq!(
            written,
            r#"["session","spacer","cwd","spacer","spacer","agent"]"#
        );
        assert_eq!(read, arranged);
    }

    #[test]
    fn given_an_empty_bar_when_it_is_read_back_then_it_stays_empty() {
        // Given — vider la barre est un choix, et le distinguer d'un fichier illisible est
        // la seule façon de ne pas le défaire au redémarrage
        let emptied = StatusBarLayout::normalized(vec![]);

        // When
        let written = serde_json::to_string(&emptied).expect("une barre vide se sérialise");
        let read = parse(&written);

        // Then — et surtout pas les défauts : c'est le tiroir du mode édition qui les rend
        assert_eq!(read, emptied);
    }

    #[test]
    fn given_a_bar_written_with_a_word_this_version_does_not_know_when_it_is_read_then_nothing_is_kept(
    ) {
        // Given — un `status_bar` bricolé à la main, ou écrit par un Ash plus récent qui
        // aurait un huitième segment
        let unknown = r#"["cwd","tides","agent"]"#;

        // When
        let read = parse(unknown);

        // Then — la barre repart des défauts, et **rien d'autre du fichier n'est perdu** :
        // le thème, la police et la largeur de colonne se relisent normalement. Garder `cwd`
        // et `agent` en jetant le mot inconnu donnerait une barre que personne n'a demandée,
        // et l'écrirait au prochain changement.
        assert_eq!(read, StatusBarLayout::default());
    }
}
