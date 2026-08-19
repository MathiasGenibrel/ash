//! Les liaisons — **la** liste unique des raccourcis d'Ash.
//!
//! Avant cette feature, les accélérateurs étaient en dur dans `descriptor()` et la fenêtre
//! de réglages les **lisait** (issue #110). La liste ne s'est pas dédoublée pour devenir
//! réglable : elle a **changé de côté**. Ce module la détient, le menu natif s'en déduit, et
//! l'écran la montre — un seul détenteur, deux lecteurs
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! Ce que la feature ne sait pas, et ne doit pas savoir : quelles actions Ash a, ce qu'elles
//! font, et à quoi ressemble un menu. Tout ça lui est **donné** à la construction, sous la
//! forme d'une liste d'[`ActionBinding`] que `src-tauri/src/menu.rs` compose — c'est lui qui
//! possède les actions, et lui qui saura les rejouer.
//!
//! **L'effet système de la feature**, avec ses deux adaptateurs :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `BindingStore` (`store.rs`) | `FileBindingStore` — `~/.ash/shortcuts.json` | `FakeBindingStore` (`fakes.rs`) |

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::combination::{Combination, KeyStroke};
use super::error::ShortcutError;
use super::reserved::{reservation, Reservation};
use super::store::{BindingStore, StoredBindings};

/// Comment une action se montre dans la section `shortcuts`.
///
/// Les trois cas viennent du menu réel, pas d'une envie de généralité : les trois entrées de
/// thème n'ont pas de raccourci, et les neuf positions d'onglet en ont un chacune mais se
/// lisent en une ligne (spec §4.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    /// Pas de ligne du tout — l'action n'a pas de raccourci, et n'a pas à en avoir.
    Hidden,
    /// Une ligne, et elle se capture.
    Row,
    /// Une ligne pour toute une famille, lue d'un bout à l'autre — `Tab 1 … Tab 9`,
    /// `⌘1 … ⌘9`. Elle ne se capture pas : neuf positions rendues réglables une par une
    /// feraient neuf lignes presque identiques, et une famille à demi rebindée ne veut rien
    /// dire. `through` nomme l'action de l'autre extrémité.
    ///
    /// **Elle est incessible, et c'est une décision, pas un reste** (issue #137). La question
    /// s'est posée le jour où capturer `⌘1` sur une autre action a posé deux liaisons sur la
    /// touche : fallait-il refuser la capture, ou rendre la famille cessible ? Rendre
    /// cessible demandait de décider ce que devient une famille amputée et ce que le retour
    /// au défaut lui rend, pour une combinaison qui n'a aucune raison d'aller ailleurs — et
    /// `⌘1`…`⌘9` s'adaptent déjà au clavier, puisque macOS y répond sous `⌘&`, `⌘é`, `⌘"` sur
    /// un AZERTY. C'est donc la capture qui est refusée, et le refus **nomme** ce qui tient
    /// la touche : on annonce, on n'interdit pas sans expliquer.
    Family { through: String },
}

/// Une action du menu, telle que `menu.rs` la déclare aux liaisons.
///
/// C'est **le défaut par action** dont `back to default`, `reset all` et le compteur
/// `n changed` ont besoin : sans lui, il n'y a rien à comparer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionBinding {
    /// L'identifiant d'entrée de menu — la clé, dans le fichier comme sur la frontière.
    pub action: String,
    /// Le sous-menu où l'action vit : c'est ce qui **groupe** la liste.
    pub group: String,
    pub label: String,
    /// La combinaison d'origine, ou `None` pour une action sans raccourci.
    pub default: Option<Combination>,
    pub listing: Listing,
}

impl ActionBinding {
    fn rebindable(&self) -> bool {
        matches!(self.listing, Listing::Row)
    }
}

/// Une ligne de la section `shortcuts` (spec §4.4, planche `3j`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRow {
    /// L'identifiant de l'action — ce que la capture renvoie au backend.
    pub action: String,
    /// Le sous-menu où l'action vit : c'est ce qui **groupe** la liste.
    pub group: String,
    pub label: String,
    /// La combinaison, écrite comme macOS l'écrit — `⇧⌘T`, `⌃⇥`, `⌘+`. Vide quand la ligne
    /// n'a **aucun** raccourci : c'est l'état que `⌫` produit.
    pub keys: String,
    /// Ce que `back to default` rendrait. Vide si le défaut est « aucun raccourci ».
    pub default_keys: String,
    /// La ligne a été changée : c'est elle qui porte l'icône de retour, et elle seule.
    pub changed: bool,
    /// Cette ligne s'ouvre en capture. Faux pour la famille des positions d'onglet.
    pub rebindable: bool,
    /// Ce qui prend la combinaison avant Ash, s'il y a quelqu'un.
    pub reservation: Option<Reservation>,
}

/// Les deux lignes d'un conflit, et la ou les issues qui le referment.
///
/// Il naît d'une capture qui viserait une combinaison déjà prise, et **rien n'est appliqué
/// tant qu'il vit** : c'est la règle de la planche, « ash ne réattribue jamais en silence ».
///
/// Il a **deux formes**, et ce qui les distingue est son détenteur (issue #137) :
/// une ligne réglable peut céder sa combinaison, la famille des positions d'onglet ne le peut
/// pas. La seconde forme est donc un **refus** : les deux mêmes lignes, un diagnostic qui dit
/// pourquoi c'est sans appel, et **pas** de `give` — voir [`Held`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ShortcutConflict {
    /// La combinaison disputée, en glyphes — et écrite comme elle a été **pressée** : `⌘&`
    /// sur un clavier français, là où la liaison, elle, se lit `⌘1`. Le bloc répond à une
    /// frappe, donc il parle de la touche qui est sous les doigts.
    pub keys: String,
    /// L'action qui détient la combinaison — la ligne `already assigned`.
    pub holder: String,
    pub holder_label: String,
    /// L'action qui vient de la capturer — la ligne `just now`.
    pub asked: String,
    pub asked_label: String,
    /// Le diagnostic, sous le filet : `two actions on ⌘X — the last one set would silently
    /// win`.
    pub diagnosis: String,
    /// Le libellé de l'issue conséquente — `give ⌘X to New Tab`.
    ///
    /// **Absent quand le détenteur ne peut pas céder** : la famille `Tab 1 … Tab 9` n'est pas
    /// réglable, donc lui reprendre `⌘1` ne tiendrait pas une session — la relecture du
    /// fichier le lui rendrait au démarrage suivant. Le bloc n'est alors plus un choix, c'est
    /// un refus, et il n'offre que `keep`.
    pub give: Option<String>,
    /// Le libellé de l'issue secondaire — `keep the old one`. Seule issue d'un refus.
    pub keep: String,
}

/// Tout ce que la section `shortcuts` affiche, d'un bloc.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ShortcutsReport {
    pub rows: Vec<ShortcutRow>,
    /// Le compteur d'en-tête — `n changed`.
    pub changed: usize,
    pub conflict: Option<ShortcutConflict>,
}

/// Ce que le bloc de capture montre pendant qu'on tape.
///
/// Il est rendu par le backend et non calculé dans la webview pour la même raison que le
/// reste : les glyphes, la règle du modificateur et la table des réservées sont ici.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CapturePreview {
    /// La combinaison frappée, en glyphes — vide si elle n'en fait pas une.
    pub keys: String,
    /// Elle peut être confirmée par `⏎`.
    pub accepted: bool,
    /// Pourquoi elle ne peut pas l'être, le cas échéant.
    pub why: Option<String>,
    /// L'avertissement de la planche — **jamais** un refus.
    pub reservation: Option<Reservation>,
}

/// L'issue choisie devant un conflit. Deux, nommées, et pas de troisième silencieuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum ConflictChoice {
    /// Donner la combinaison à l'action qui vient de la capturer — l'autre la perd, et se
    /// retrouve sans raccourci plutôt qu'avec un raccourci qui ne part jamais.
    Give,
    /// Garder celle qui l'avait. La capture est jetée.
    Keep,
}

/// Un changement a-t-il eu lieu, et le menu natif doit-il être refait ?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rebound(pub bool);

/// Ce qu'une capture attend d'être appliquée.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    action: String,
    combination: Combination,
    /// La combinaison écrite comme elle a été **pressée** — `⌘&` sur un AZERTY, là où la
    /// liaison, elle, s'écrit `⌘1`. Retenue ici parce que la frappe est perdue ensuite, et
    /// que tout ce que le bloc dit parle de ce qui vient d'être tapé.
    keys: String,
    holder: Held,
}

/// Qui tient la combinaison disputée — et, par là, **ce que le bloc peut offrir**.
///
/// Les deux cas ne sont pas un booléen posé à côté d'un identifiant : c'est cette valeur qui
/// rend un `give` sur une ligne fixe **inconstruisible**, plutôt qu'une vérification de plus
/// au moment de résoudre (issue #137). Le refus n'est pas une exception rattrapée en chemin,
/// c'est l'autre branche du même type.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Held {
    /// Une ligne réglable : elle peut céder sa combinaison, et le bloc a ses deux issues.
    Negotiable(String),
    /// Une ligne qui ne se règle pas — la famille des positions d'onglet. Rien ne lui est
    /// repris : `⌘1`…`⌘9` restent à la sélection d'onglet, et le bloc devient un refus.
    Fixed(String),
}

impl Held {
    fn action(&self) -> &str {
        match self {
            Held::Negotiable(action) | Held::Fixed(action) => action,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Chosen {
    /// Ce qui s'écarte du défaut. `Some(_)` : une autre combinaison. `None` : aucun
    /// raccourci — c'est ce que `⌫` pose, et il faut pouvoir le distinguer de « pas
    /// d'entrée », qui veut dire « le défaut ».
    overrides: BTreeMap<String, Option<Combination>>,
    pending: Option<Pending>,
}

/// Les liaisons en vigueur — **la** source de vérité des raccourcis.
pub struct Bindings {
    actions: Vec<ActionBinding>,
    current: Mutex<Chosen>,
    store: Arc<dyn BindingStore>,
}

impl Bindings {
    /// Repart des liaisons de la session précédente, ou des défauts.
    ///
    /// Ce qui est relu est **assaini**, et pas cru sur parole : `~/.ash/shortcuts.json` est
    /// un fichier de l'utilisateur, éditable à la main. Une entrée qui nomme une action que
    /// le menu n'a plus, une action non rebindable, ou une combinaison qu'une autre entrée
    /// tient déjà, est laissée de côté — l'action repart de son défaut. Sans ça, un fichier
    /// bricolé poserait deux fois la même touche dans le menu natif, où c'est la dernière
    /// posée qui gagne, en silence : exactement ce que le bloc de conflit existe pour éviter.
    pub fn restore(store: Arc<dyn BindingStore>, actions: Vec<ActionBinding>) -> Self {
        let stored = store.load().unwrap_or_default();
        let bindings = Self {
            actions,
            current: Mutex::new(Chosen::default()),
            store,
        };
        let sane = bindings.sanitized(stored.bindings);
        bindings.locked().overrides = sane;
        bindings
    }

    /// Les liaisons relues, débarrassées de ce qui ne se tient pas.
    ///
    /// **En deux temps, et l'ordre des deux est ce qui fait toute la différence.** Le fichier
    /// est d'abord relu **en entier**, puis les collisions sont tranchées sur le monde qu'il
    /// décrit. Juger une entrée sur un monde à demi construit — où les actions pas encore
    /// lues portent encore leur défaut — faisait perdre au redémarrage tout ce qui *déplace*
    /// une combinaison plutôt que d'en poser une neuve : un `give` du bloc de conflit
    /// (`⌘W` à New Tab, plus rien à Close Tab) et, a fortiori, un échange de deux touches se
    /// voyaient refusés au motif que l'ancienne détentrice tenait *encore*, dans ce monde
    /// partiel, la combinaison qu'elle venait de céder.
    fn sanitized(
        &self,
        stored: BTreeMap<String, Option<Combination>>,
    ) -> BTreeMap<String, Option<Combination>> {
        // 1. Ce que le fichier propose, moins ce qui ne nomme rien de rebindable : une
        //    action que le menu n'a plus, ou dont le raccourci n'est pas à donner.
        let mut kept: BTreeMap<String, Option<Combination>> = self
            .actions
            .iter()
            .filter(|one| one.rebindable())
            .filter_map(|declared| {
                let candidate = stored.get(&declared.action)?;
                Some((declared.action.clone(), candidate.clone()))
            })
            .collect();

        // 2. Une combinaison n'est tenue que par une action — le **même** invariant qu'une
        //    capture ou qu'un retour au défaut, posé à la même question ([`holder`]). Le
        //    parcours est dans l'ordre du **menu** et non celui du fichier : c'est ce qui
        //    rend l'arbitrage déterministe d'une relecture à l'autre, et c'est ce qui fait
        //    que « déjà tenue » veut dire « tenue par une ligne déjà tranchée ». Sans lui,
        //    un fichier bricolé poserait deux fois la même touche dans le menu natif, où
        //    c'est la dernière posée qui gagne, en silence — exactement ce que le bloc de
        //    conflit existe pour éviter.
        //
        //    Les lignes **fixes** sont tranchées avant toutes les autres, quel que soit leur
        //    rang : leur combinaison ne bouge jamais, ni par le fichier ni par un geste. Les
        //    compter est ce qui ferme le trou de l'issue #137 — un `shortcuts.json` bricolé
        //    qui pose `⌘1` sur New Tab est tranché ici, par le même chemin que le reste, et
        //    New Tab repart de son défaut plutôt que de doubler la sélection d'onglet.
        let fixed: Vec<&ActionBinding> = self
            .actions
            .iter()
            .filter(|one| !one.rebindable())
            .collect();
        let ordered: Vec<&ActionBinding> =
            self.actions.iter().filter(|one| one.rebindable()).collect();
        for (rank, declared) in ordered.iter().enumerate() {
            let settled = || fixed.iter().chain(ordered[..rank].iter()).copied();
            // Une ligne sans raccourci ne dispute rien à personne.
            let Some(in_use) = effective(declared, &kept) else {
                continue;
            };
            if holder(settled(), &declared.action, &in_use, &kept).is_none() {
                continue;
            }
            // Son défaut s'il est libre — elle y repart, comme si le fichier n'avait rien
            // dit d'elle. Sinon rien : elle reste **sans** raccourci plutôt que d'en porter
            // un que le menu donnerait à une autre.
            let fallback = declared
                .default
                .clone()
                .filter(|free| holder(settled(), &declared.action, free, &kept).is_none());
            // Et c'est [`record`] qui l'inscrit, comme partout ailleurs : l'assainissement
            // n'a pas plus le droit qu'une capture d'écrire une valeur qui répète le défaut.
            record(declared, fallback, &mut kept);
        }
        kept
    }

    /// La combinaison en vigueur pour cette action — ce que le menu natif doit poser.
    pub fn effective(&self, action: &str) -> Option<Combination> {
        let declared = self.actions.iter().find(|one| one.action == action)?;
        effective(declared, &self.locked().overrides)
    }

    /// L'accélérateur à donner à l'entrée de menu, ou `None` — l'entrée reste alors
    /// cliquable à la souris, sans touche.
    pub fn accelerator(&self, action: &str) -> Option<String> {
        self.effective(action)
            .map(|combination| combination.accelerator())
    }

    /// L'action à qui cette frappe appartient, s'il y en a une.
    ///
    /// C'est la question que pose la webview pour les frappes que le menu natif ne peut
    /// **pas** consommer — `⌃⇥` et `⌃⇧⇥`, dont `muda` rend un équivalent clavier qu'AppKit ne
    /// reconnaît jamais (voir l'en-tête de `src-tauri/src/menu.rs`). Elle rend un
    /// identifiant d'action, jamais une combinaison : la webview n'a ainsi ni table de
    /// touches, ni règle de comparaison, et rien à tenir à jour quand une liaison change
    /// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    ///
    /// Une frappe que personne ne tient rend `None`, et c'est ce qui fait qu'un raccourci
    /// **déplacé cesse de répondre à son ancienne touche** — la seule chose qui rendait le
    /// rebinding à moitié vrai.
    pub fn owner(&self, stroke: &KeyStroke) -> Option<String> {
        let sought = Combination::from_stroke(stroke).ok()?;
        let overrides = &self.locked().overrides;
        self.actions
            .iter()
            .find(|declared| effective(declared, overrides).as_ref() == Some(&sought))
            .map(|declared| declared.action.clone())
    }

    /// La combinaison en vigueur d'une action, écrite comme macOS l'écrit — vide s'il n'y en
    /// a aucune.
    ///
    /// C'est l'autre sens de la même question : ce qu'une surface affiche d'un raccourci.
    /// Le pied de la sidebar montre `⌘T` parce qu'il le demande ici, et non parce qu'il le
    /// sait — sans quoi il continuerait de l'annoncer après un rebinding.
    pub fn keys(&self, action: &str) -> String {
        self.effective(action)
            .map(|combination| combination.glyphs())
            .unwrap_or_default()
    }

    /// Tout ce que la section `shortcuts` affiche.
    pub fn report(&self) -> ShortcutsReport {
        let chosen = self.locked().clone();
        let rows: Vec<ShortcutRow> = self
            .actions
            .iter()
            .filter_map(|declared| self.row(declared, &chosen.overrides))
            .collect();
        ShortcutsReport {
            changed: rows.iter().filter(|row| row.changed).count(),
            conflict: chosen
                .pending
                .as_ref()
                .map(|pending| self.conflict(pending)),
            rows,
        }
    }

    fn row(
        &self,
        declared: &ActionBinding,
        overrides: &BTreeMap<String, Option<Combination>>,
    ) -> Option<ShortcutRow> {
        let in_use = effective(declared, overrides);
        let written = |combination: &Option<Combination>| {
            combination
                .as_ref()
                .map(Combination::glyphs)
                .unwrap_or_default()
        };

        let (label, keys) = match &declared.listing {
            Listing::Hidden => return None,
            Listing::Row => (self.listed_as(&declared.action), written(&in_use)),
            Listing::Family { through } => {
                let last = self.actions.iter().find(|one| &one.action == through)?;
                (
                    self.listed_as(&declared.action),
                    format!("{} … {}", written(&in_use), written(&last.default)),
                )
            }
        };

        Some(ShortcutRow {
            action: declared.action.clone(),
            group: declared.group.clone(),
            label,
            keys,
            default_keys: written(&declared.default),
            changed: declared.rebindable() && in_use != declared.default,
            rebindable: declared.rebindable(),
            reservation: in_use.as_ref().and_then(reservation),
        })
    }

    /// Le bloc qu'une capture en attente fait sortir — un choix, ou un refus.
    ///
    /// Les deux textes se répondent, et c'est voulu : le premier dit ce qui **se passerait**
    /// sans ce bloc — dans un menu natif, deux entrées sur la même touche laissent gagner la
    /// dernière posée, sans rien dire à personne. Le second dit ce qu'Ash **ne fera pas**, et
    /// pourquoi, dans le registre d'une combinaison réservée (`reserved.rs`) : on annonce, on
    /// n'interdit pas sans expliquer.
    fn conflict(&self, pending: &Pending) -> ShortcutConflict {
        let keys = pending.keys.clone();
        let asked_label = self.listed_as(&pending.action);
        let holder_label = self.listed_as(pending.holder.action());
        let (diagnosis, give) = match &pending.holder {
            Held::Negotiable(_) => (
                format!("two actions on {keys} — the last one set would silently win"),
                Some(format!("give {keys} to {asked_label}")),
            ),
            Held::Fixed(_) => (
                format!(
                    "{keys} belongs to {holder_label} — that row is not rebindable, and ash will not take it away"
                ),
                None,
            ),
        };
        ShortcutConflict {
            holder: pending.holder.action().to_owned(),
            holder_label,
            asked: pending.action.clone(),
            diagnosis,
            give,
            keep: "keep the old one".to_owned(),
            asked_label,
            keys,
        }
    }

    /// Le nom sous lequel une action se **lit** dans la liste — le sien, ou celui de la
    /// famille qui l'englobe.
    ///
    /// Les huit positions d'onglet qui suivent `Tab 1` n'ont pas de ligne à elles
    /// (`Listing::Hidden`) : nommer le détenteur de `⌘é` « Tab 2 » désignerait une ligne que
    /// l'écran ne montre nulle part, et un refus qui nomme l'invisible n'explique rien. La
    /// famille se retrouve par l'ordre de déclaration — de sa ligne jusqu'à l'action que son
    /// `through` nomme, incluse.
    fn listed_as(&self, action: &str) -> String {
        let Some((rank, declared)) = self
            .actions
            .iter()
            .enumerate()
            .find(|(_, one)| one.action == action)
        else {
            return action.to_owned();
        };
        let heading = self.covering(rank).unwrap_or(declared);
        match &heading.listing {
            Listing::Family { through } => self
                .actions
                .iter()
                .find(|one| &one.action == through)
                .map(|last| format!("{} … {}", heading.label, last.label))
                .unwrap_or_else(|| heading.label.clone()),
            _ => heading.label.clone(),
        }
    }

    /// La ligne de famille qui englobe l'action de ce rang, s'il y en a une.
    fn covering(&self, rank: usize) -> Option<&ActionBinding> {
        self.actions.iter().enumerate().find_map(|(head, one)| {
            let Listing::Family { through } = &one.listing else {
                return None;
            };
            let tail = self
                .actions
                .iter()
                .position(|other| &other.action == through)?;
            (head <= rank && rank <= tail).then_some(one)
        })
    }

    /// Ce que le bloc de capture montre d'une frappe, sans rien retenir.
    pub fn preview(&self, stroke: &KeyStroke) -> CapturePreview {
        match Combination::from_stroke(stroke) {
            Ok(combination) => CapturePreview {
                keys: combination.glyphs(),
                accepted: true,
                why: None,
                // Annoncée, jamais interdite : `accepted` reste vrai.
                reservation: reservation(&combination),
            },
            Err(why) => CapturePreview {
                keys: String::new(),
                accepted: false,
                why: Some(why.to_string()),
                reservation: None,
            },
        }
    }

    /// Pose la combinaison qu'une capture a rendue — ou ouvre le conflit qu'elle produirait.
    ///
    /// **Rien n'est réattribué en silence** : si une autre action tient déjà la combinaison,
    /// la capture est mise en attente et le bloc de conflit sort. C'est l'utilisateur qui
    /// tranche, par l'une des deux issues nommées de [`resolve`](Self::resolve).
    pub fn bind(&self, action: &str, stroke: &KeyStroke) -> Result<Rebound, ShortcutError> {
        let combination = Combination::from_stroke(stroke)?;
        let declared = self.declared(action)?;
        if !declared.rebindable() {
            return Err(ShortcutError::FixedBinding {
                action: action.to_owned(),
            });
        }
        // Ce que le bloc de conflit écrira est la combinaison **telle qu'elle a été pressée**
        // — `⌘&` sur un AZERTY : c'est la seule touche que l'utilisateur ait sous les doigts.
        let pressed = combination.glyphs_pressed(stroke);
        Ok(self.claim(declared, Some(combination), Some(pressed)))
    }

    /// Pose ce qu'une action doit porter — **le seul chemin par lequel une liaison change**.
    ///
    /// Il n'y a qu'un endroit où l'on écrit, donc qu'un endroit où l'invariant « une
    /// combinaison, une action » s'applique : la capture, le `⌫`, le retour au défaut et les
    /// deux issues d'un conflit passent tous par ici. Avoir eu deux règles à deux endroits
    /// est ce qui a laissé un `back to default` reposer une combinaison déjà tenue, en
    /// silence (issue #134) — et le menu natif laisse alors gagner la dernière entrée posée,
    /// sans rien dire à personne.
    ///
    /// **Rien n'est appliqué tant qu'un conflit vit** : si une autre ligne tient déjà ce
    /// qu'on veut poser, la demande est mise en attente et le bloc sort. C'est l'utilisateur
    /// qui tranche, par l'une des deux issues nommées de [`resolve`](Self::resolve).
    fn claim(
        &self,
        declared: &ActionBinding,
        wanted: Option<Combination>,
        pressed: Option<String>,
    ) -> Rebound {
        let mut chosen = self.locked();
        if let Some(sought) = &wanted {
            if let Some(held) = holder(
                self.actions.iter(),
                &declared.action,
                sought,
                &chosen.overrides,
            ) {
                chosen.pending = Some(Pending {
                    action: declared.action.clone(),
                    keys: pressed.unwrap_or_else(|| sought.glyphs()),
                    combination: sought.clone(),
                    // Ce que le détenteur **est** décide de ce que le bloc offrira, et c'est
                    // décidé ici, une fois : une ligne qui ne se règle pas ne cède rien.
                    holder: if held.rebindable() {
                        Held::Negotiable(held.action.clone())
                    } else {
                        Held::Fixed(held.action.clone())
                    },
                });
                return Rebound(false);
            }
        }

        // Rien ne dispute plus rien : le bloc de conflit resté ouvert n'a plus d'objet, et
        // le refermer sans rien appliquer, c'est ce que `keep` fait déjà.
        chosen.pending = None;
        let mut next = chosen.overrides.clone();
        record(declared, wanted, &mut next);
        // Une capture qui redonne à une ligne ce qu'elle a déjà n'est pas un changement :
        // refaire le menu pour ça le reposerait à chaque frappe confirmée.
        if next == chosen.overrides {
            return Rebound(false);
        }
        chosen.overrides = next;
        self.keep(chosen);
        Rebound(true)
    }

    /// Retire le raccourci d'une ligne — le `⌫` du bloc de capture.
    ///
    /// L'action reste dans le menu, **cliquable à la souris** : la spec §4.4 demande que
    /// toutes ces actions le restent, et retirer l'entrée avec sa touche l'aurait rendue
    /// introuvable.
    pub fn clear(&self, action: &str) -> Result<Rebound, ShortcutError> {
        let declared = self.declared(action)?;
        if !declared.rebindable() {
            return Err(ShortcutError::FixedBinding {
                action: action.to_owned(),
            });
        }
        Ok(self.claim(declared, None, None))
    }

    /// Rend son défaut à une ligne — l'icône de retour, qui n'existe que sur les lignes
    /// changées.
    ///
    /// **C'est une pose comme une autre**, et c'est tout le propos de l'issue #134 : le
    /// défaut d'une ligne peut très bien être la combinaison qu'une autre porte depuis
    /// qu'on la lui a donnée. Le geste ouvre alors le **même** bloc de conflit qu'une
    /// capture, avec ses deux issues nommées, et rien n'est écrit tant que l'utilisateur n'a
    /// pas choisi.
    pub fn reset(&self, action: &str) -> Result<Rebound, ShortcutError> {
        let declared = self.declared(action)?;
        Ok(self.claim(declared, declared.default.clone(), None))
    }

    /// `reset all` — toutes les lignes reprennent leur défaut.
    ///
    /// Le seul geste qui repose **toute** la table d'un coup, donc le seul qui ne se traite
    /// pas ligne à ligne : douze conflits ne se tranchent pas un par un, et il n'y a de
    /// toute façon rien à arbitrer. Les défauts d'Ash ne se disputent aucune combinaison —
    /// `menu.rs` en tient le test —, et repartir d'eux est exactement ce que la relecture
    /// d'un fichier vide donnerait. C'est donc l'assainissement qui répond, plutôt qu'un
    /// `clear()` qui ferait *confiance* à cette propriété sans jamais la vérifier : le jour
    /// où deux défauts se marcheraient dessus, le menu n'en porterait quand même qu'un.
    pub fn reset_all(&self) -> Rebound {
        let restored = self.sanitized(BTreeMap::new());
        let mut chosen = self.locked();
        if chosen.overrides == restored && chosen.pending.is_none() {
            return Rebound(false);
        }
        let rebound = chosen.overrides != restored;
        chosen.overrides = restored;
        chosen.pending = None;
        self.keep(chosen);
        Rebound(rebound)
    }

    /// Referme un conflit par l'issue choisie.
    ///
    /// Elles sont deux quand le détenteur peut céder, **une seule** quand il ne le peut pas :
    /// c'est [`Held`] qui le dit, et c'est pourquoi un `give` sur une ligne fixe ne se
    /// construit pas ici (issue #137).
    ///
    /// `Give` ne se contente pas de poser la combinaison sur la nouvelle action : elle
    /// **retire** celle de l'ancienne. Les laisser toutes les deux serait rouvrir le conflit
    /// que le geste vient de fermer.
    pub fn resolve(&self, choice: ConflictChoice) -> Rebound {
        let mut chosen = self.locked();
        let Some(pending) = chosen.pending.take() else {
            return Rebound(false);
        };
        match (choice, &pending.holder) {
            // `Keep` jette la capture — et c'est la **seule** issue d'un refus : le bloc
            // qu'une ligne fixe fait sortir n'offre pas de `give`, et un `give` qui
            // arriverait quand même n'a rien à quoi s'appliquer. Le reprendre à la famille
            // des positions d'onglet ne tiendrait de toute façon pas une session : la
            // relecture du fichier le lui rendrait au démarrage suivant (issue #137).
            (ConflictChoice::Keep, _) | (ConflictChoice::Give, Held::Fixed(_)) => Rebound(false),
            (ConflictChoice::Give, Held::Negotiable(holder)) => {
                // L'ancienne détentrice **d'abord** : la combinaison doit être libre avant
                // d'être reposée, sans quoi les deux lignes la porteraient le temps d'une
                // instruction — et c'est cet état-là que le bloc existe pour ne jamais
                // atteindre.
                let mut next = chosen.overrides.clone();
                for (action, wanted) in [
                    (holder.as_str(), None),
                    (pending.action.as_str(), Some(pending.combination.clone())),
                ] {
                    if let Some(declared) = self.actions.iter().find(|one| one.action == action) {
                        record(declared, wanted, &mut next);
                    }
                }
                chosen.overrides = next;
                self.keep(chosen);
                Rebound(true)
            }
        }
    }

    fn declared(&self, action: &str) -> Result<&ActionBinding, ShortcutError> {
        self.actions
            .iter()
            .find(|one| one.action == action)
            .ok_or_else(|| ShortcutError::UnknownAction {
                action: action.to_owned(),
            })
    }

    /// Écrit les liaisons sur le disque.
    ///
    /// L'échec ne remet pas le changement en cause : le raccourci s'applique tout de suite,
    /// il ne survivra simplement pas au redémarrage. Refuser une liaison parce que `~/.ash`
    /// n'est pas inscriptible serait incompréhensible pour qui vient de la poser.
    fn keep(&self, chosen: std::sync::MutexGuard<'_, Chosen>) {
        let stored = StoredBindings {
            bindings: chosen.overrides.clone(),
        };
        drop(chosen);
        let _ = self.store.save(&stored);
    }

    /// Un verrou empoisonné veut dire qu'un fil a paniqué **ailleurs** en le tenant. La
    /// valeur qu'il protège est une table de raccourcis : elle est intacte, et propager la
    /// panique éteindrait la fenêtre pour un `⌘T`.
    fn locked(&self) -> std::sync::MutexGuard<'_, Chosen> {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// L'action qui tient déjà cette combinaison, si ce n'est pas `except`.
///
/// **C'est l'unique lecture de l'invariant « une combinaison, une action »**, et c'est ce qui
/// permet aux quatre chemins d'y répondre pareil : la capture, le retour au défaut, `reset
/// all` et la relecture d'un fichier édité à la main posent tous cette question-ci. En avoir
/// eu deux, avec deux règles, est ce qui a produit l'issue #134 — et, juste avant elle, une
/// relecture qui défaisait ce qu'un `give` avait décidé.
///
/// `among` n'est pas toujours toute la table : l'assainissement d'un fichier ne compare
/// qu'aux lignes qu'il a **déjà tranchées**, pour que l'arbitrage suive l'ordre du menu.
///
/// **Tout le monde compte, y compris les lignes qui ne se règlent pas** (issue #137). La
/// question posée ici est « qui tient cette combinaison ? », et elle n'a rien à voir avec
/// « à qui puis-je proposer une issue ? » : mêler les deux — en filtrant sur `rebindable` —
/// laissait `⌘1`…`⌘9` tenues par personne aux yeux de la règle, et une capture de `⌘1` posait
/// alors **deux** actions sur la touche, en silence. Ce que le détenteur est se lit après,
/// une fois qu'il est trouvé : voir [`Held`].
fn holder<'a>(
    among: impl IntoIterator<Item = &'a ActionBinding>,
    except: &str,
    combination: &Combination,
    overrides: &BTreeMap<String, Option<Combination>>,
) -> Option<&'a ActionBinding> {
    among.into_iter().find(|other| {
        other.action != except && effective(other, overrides).as_ref() == Some(combination)
    })
}

/// Inscrit dans la table des écarts ce qu'une action doit porter.
///
/// **C'est l'unique écriture**, comme [`holder`] est l'unique lecture : la capture, le `⌫`,
/// le retour au défaut, les deux issues d'un conflit et l'assainissement d'un fichier relu y
/// passent tous. Une seconde façon d'écrire serait une seconde façon de se tromper.
///
/// Le fichier ne garde que les **écarts** : une liaison qui retombe sur son défaut n'y est
/// pas écrite, elle en **sort**. C'est ce qui fait qu'un retour au défaut et une capture qui
/// redonne à une ligne sa combinaison d'origine sont le même geste, et qu'un défaut d'Ash qui
/// changera demain sera suivi plutôt que figé par une entrée devenue muette.
fn record(
    declared: &ActionBinding,
    wanted: Option<Combination>,
    overrides: &mut BTreeMap<String, Option<Combination>>,
) {
    if wanted == declared.default {
        overrides.remove(&declared.action);
    } else {
        overrides.insert(declared.action.clone(), wanted);
    }
}

/// La combinaison en vigueur : ce qui a été choisi, sinon le défaut.
///
/// Hors de l'`impl` parce que l'assainissement la calcule sur une table **en cours de
/// construction**, qui n'est pas encore celle de l'objet.
fn effective(
    declared: &ActionBinding,
    overrides: &BTreeMap<String, Option<Combination>>,
) -> Option<Combination> {
    match overrides.get(&declared.action) {
        Some(chosen) => chosen.clone(),
        None => declared.default.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::fakes::FakeBindingStore as FakeStore;

    /// Les actions du menu, en petit : de quoi jouer une capture, un conflit et une famille
    /// sans dépendre de ce que le menu réel déclare aujourd'hui.
    struct BindingsBuilder {
        actions: Vec<ActionBinding>,
        store: Arc<dyn BindingStore>,
    }

    impl BindingsBuilder {
        fn new() -> Self {
            Self {
                actions: Vec::new(),
                store: Arc::new(FakeStore::default()),
            }
        }

        fn action(mut self, action: &str, label: &str, default: Option<&str>) -> Self {
            self.actions.push(ActionBinding {
                action: action.to_owned(),
                group: "terminal".to_owned(),
                label: label.to_owned(),
                default: default.map(|written| Combination::parse(written).unwrap()),
                listing: Listing::Row,
            });
            self
        }

        fn family(mut self, action: &str, label: &str, default: &str, through: &str) -> Self {
            self.actions.push(ActionBinding {
                action: action.to_owned(),
                group: "terminal".to_owned(),
                label: label.to_owned(),
                default: Some(Combination::parse(default).unwrap()),
                listing: Listing::Family {
                    through: through.to_owned(),
                },
            });
            self
        }

        /// Une position d'onglet qui n'a pas de ligne à elle : elle porte sa combinaison, et
        /// se lit sous la ligne de la famille — `Tab 2 … Tab 9` dans le menu réel.
        fn hidden(mut self, action: &str, label: &str, default: &str) -> Self {
            self.actions.push(ActionBinding {
                action: action.to_owned(),
                group: "terminal".to_owned(),
                label: label.to_owned(),
                default: Some(Combination::parse(default).unwrap()),
                listing: Listing::Hidden,
            });
            self
        }

        fn store(mut self, store: Arc<dyn BindingStore>) -> Self {
            self.store = store;
            self
        }

        fn build(self) -> Bindings {
            Bindings::restore(self.store, self.actions)
        }
    }

    /// Deux actions ordinaires : `New Tab` sur `⌘T`, `Close Tab` sur `⌘W`.
    fn two_tabs() -> BindingsBuilder {
        BindingsBuilder::new()
            .action("tab:new", "New Tab", Some("Cmd+T"))
            .action("tab:close", "Close Tab", Some("Cmd+W"))
    }

    /// Une frappe telle que la webview la rapporte : le caractère produit **et** la
    /// position physique. Les deux coïncident sur un clavier US ; ce qui se passe quand ils
    /// diffèrent est éprouvé dans `combination.rs`.
    fn stroke(character: &str, code: &str, command: bool, control: bool) -> KeyStroke {
        KeyStroke {
            key: character.to_owned(),
            code: code.to_owned(),
            command,
            control,
            option: false,
            shift: false,
        }
    }

    /// Les combinaisons que deux actions porteraient à la fois — toujours vide, quel que
    /// soit le chemin qui a mené là. Dans un menu natif, c'est la dernière entrée posée qui
    /// gagne, sans rien dire à personne.
    fn doubled(bindings: &Bindings, actions: &[&str]) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        let mut twice: Vec<String> = Vec::new();
        for action in actions {
            let Some(carried) = bindings.accelerator(action) else {
                continue;
            };
            if seen.contains(&carried) {
                twice.push(carried);
            } else {
                seen.push(carried);
            }
        }
        twice
    }

    fn row<'a>(report: &'a ShortcutsReport, action: &str) -> &'a ShortcutRow {
        report
            .rows
            .iter()
            .find(|row| row.action == action)
            .expect("la ligne demandée")
    }

    #[test]
    fn given_a_chord_the_menu_cannot_catch_when_the_webview_asks_who_holds_it_then_it_gets_an_action_and_no_combination(
    ) {
        // Given — `⌃⇥` : `muda` lui donne un équivalent clavier qu'AppKit ne reconnaît
        // jamais, donc c'est la webview qui la capte et qui vient demander
        let bindings = BindingsBuilder::new()
            .action("tab:next", "Select Next Tab", Some("Ctrl+Tab"))
            .build();

        // When
        let held = bindings.owner(&stroke("Tab", "Tab", false, true));

        // Then — un identifiant d'action, celui que `ash://menu-action` porte déjà : la
        // webview n'a ni table de touches ni règle de comparaison à tenir
        assert_eq!(held.as_deref(), Some("tab:next"));
    }

    #[test]
    fn given_select_next_tab_moved_to_another_combination_when_the_old_chord_is_pressed_then_nobody_holds_it(
    ) {
        // Given — c'est la moitié manquante du rebinding : l'écran disait le raccourci
        // déplacé, et l'ancienne touche continuait de changer d'onglet
        let bindings = BindingsBuilder::new()
            .action("tab:next", "Select Next Tab", Some("Ctrl+Tab"))
            .build();

        // When
        bindings
            .bind("tab:next", &stroke("j", "KeyJ", true, false))
            .unwrap();

        // Then — l'ancienne ne mène plus nulle part, la nouvelle mène à l'action
        assert_eq!(bindings.owner(&stroke("Tab", "Tab", false, true)), None);
        assert_eq!(
            bindings.owner(&stroke("j", "KeyJ", true, false)).as_deref(),
            Some("tab:next")
        );
    }

    #[test]
    fn given_an_action_a_surface_announces_when_its_binding_moves_then_the_surface_is_told_the_new_one(
    ) {
        // Given — le pied de la sidebar annonce `⌘T`. Écrit en dur, il ment au premier
        // rebinding ; demandé ici, il suit
        let bindings = two_tabs().build();
        assert_eq!(bindings.keys("tab:new"), "⌘T");

        // When
        bindings
            .bind("tab:new", &stroke("j", "KeyJ", true, false))
            .unwrap();

        // Then
        assert_eq!(bindings.keys("tab:new"), "⌘J");
    }

    #[test]
    fn given_an_action_left_without_any_shortcut_when_a_surface_asks_what_to_show_then_it_gets_nothing_to_show(
    ) {
        // Given — `⌫` retire le raccourci, et l'action reste. Une surface qui recevrait un
        // libellé de repli (`none`, `—`) l'afficherait comme si c'était une touche
        let bindings = two_tabs().build();

        // When
        bindings.clear("tab:new").unwrap();

        // Then
        assert_eq!(bindings.keys("tab:new"), "");
    }

    #[test]
    fn given_a_line_at_its_default_when_a_free_combination_is_captured_then_the_line_carries_it() {
        // Given
        let bindings = two_tabs().build();

        // When
        let rebound = bindings
            .bind("tab:new", &stroke("j", "KeyJ", true, false))
            .unwrap();

        // Then — et le menu doit être refait : c'est lui qui porte la touche
        assert_eq!(rebound, Rebound(true));
        assert_eq!(row(&bindings.report(), "tab:new").keys, "⌘J");
    }

    #[test]
    fn given_a_captured_combination_when_the_menu_asks_for_its_accelerator_then_it_is_the_new_one()
    {
        // Given — c'est le test qui compte : une liaison changée doit se retrouver dans le
        // **menu natif**, pas seulement dans l'écran. C'est ce qui prouve que la liste est
        // restée unique (issue #110), et que l'écran n'en est pas devenu une seconde
        let bindings = two_tabs().build();

        // When
        bindings
            .bind("tab:new", &stroke("j", "KeyJ", true, false))
            .unwrap();

        // Then — au format que l'analyseur de `muda` lit, celui de l'entrée de menu
        assert_eq!(bindings.accelerator("tab:new").as_deref(), Some("Cmd+KeyJ"));
    }

    #[test]
    fn given_a_combination_another_action_already_holds_when_it_is_captured_then_nothing_is_reassigned(
    ) {
        // Given — `⌘W` est à `Close Tab`
        let bindings = two_tabs().build();

        // When — on la capture sur `New Tab`
        bindings
            .bind("tab:new", &stroke("w", "KeyW", true, false))
            .unwrap();

        // Then — les deux lignes n'ont pas bougé, et le conflit nomme ses deux côtés et ses
        // deux issues. Sans ce refus, le menu natif aurait porté deux fois `⌘W`, où c'est la
        // dernière entrée posée qui gagne — en silence
        let report = bindings.report();
        assert_eq!(row(&report, "tab:new").keys, "⌘T");
        assert_eq!(row(&report, "tab:close").keys, "⌘W");
        let conflict = report.conflict.unwrap();
        assert_eq!(conflict.holder, "tab:close");
        assert_eq!(conflict.asked, "tab:new");
        assert_eq!(conflict.give.as_deref(), Some("give ⌘W to New Tab"));
        assert_eq!(
            conflict.diagnosis,
            "two actions on ⌘W — the last one set would silently win"
        );
    }

    #[test]
    fn given_a_conflict_when_the_combination_is_given_away_then_the_previous_holder_keeps_none_of_it(
    ) {
        // Given
        let bindings = two_tabs().build();
        bindings
            .bind("tab:new", &stroke("w", "KeyW", true, false))
            .unwrap();

        // When
        let rebound = bindings.resolve(ConflictChoice::Give);

        // Then — l'ancienne détentrice se retrouve **sans raccourci**, pas avec un raccourci
        // qui ne partirait jamais : les laisser toutes les deux rouvrirait le conflit que ce
        // geste vient de fermer
        let report = bindings.report();
        assert_eq!(rebound, Rebound(true));
        assert_eq!(row(&report, "tab:new").keys, "⌘W");
        assert_eq!(row(&report, "tab:close").keys, "");
        assert_eq!(report.conflict, None);
    }

    #[test]
    fn given_a_conflict_when_the_old_one_is_kept_then_the_capture_is_dropped() {
        // Given
        let bindings = two_tabs().build();
        bindings
            .bind("tab:new", &stroke("w", "KeyW", true, false))
            .unwrap();

        // When
        bindings.resolve(ConflictChoice::Keep);

        // Then — rien n'a bougé, et le bloc a disparu
        let report = bindings.report();
        assert_eq!(row(&report, "tab:new").keys, "⌘T");
        assert_eq!(row(&report, "tab:close").keys, "⌘W");
        assert_eq!(report.conflict, None);
    }

    #[test]
    fn given_a_line_when_its_shortcut_is_removed_then_it_has_none_and_the_line_is_still_there() {
        // Given — le `⌫` du bloc de capture
        let bindings = two_tabs().build();

        // When
        bindings.clear("tab:new").unwrap();

        // Then — l'action reste listée, sans touche : la spec §4.4 demande qu'elle reste
        // atteignable à la souris, donc son entrée de menu ne disparaît pas avec sa touche
        let report = bindings.report();
        assert_eq!(row(&report, "tab:new").keys, "");
        assert_eq!(bindings.accelerator("tab:new"), None);
    }

    #[test]
    fn given_a_changed_line_when_it_goes_back_to_default_then_it_stops_counting_as_changed() {
        // Given — l'icône de retour n'existe que sur les lignes changées, et le compteur
        // `n changed` est sa seule contrepartie en en-tête
        let bindings = two_tabs().build();
        bindings
            .bind("tab:new", &stroke("j", "KeyJ", true, false))
            .unwrap();
        assert_eq!(bindings.report().changed, 1);

        // When
        bindings.reset("tab:new").unwrap();

        // Then
        let report = bindings.report();
        assert_eq!(report.changed, 0);
        assert!(!row(&report, "tab:new").changed);
        assert_eq!(row(&report, "tab:new").keys, "⌘T");
    }

    #[test]
    fn given_a_combination_another_action_was_given_when_its_first_owner_asks_for_its_default_back_then_the_same_conflict_block_opens(
    ) {
        // Given — `⌘T` a été donné à `Close Tab`, et `New Tab` n'a plus rien. Son défaut est
        // pourtant toujours `⌘T` : cliquer l'icône de retour posait les **deux** actions sur
        // `⌘T` sans un mot, le menu natif laissait gagner la dernière, et `Close Tab`
        // devenait injoignable au clavier (issue #134)
        let bindings = two_tabs().build();
        bindings
            .bind("tab:close", &stroke("t", "KeyT", true, false))
            .unwrap();
        bindings.resolve(ConflictChoice::Give);

        // When
        let rebound = bindings.reset("tab:new").unwrap();

        // Then — rien n'est écrit, et le bloc est celui de la capture, avec ses deux issues
        let report = bindings.report();
        assert_eq!(rebound, Rebound(false));
        assert_eq!(row(&report, "tab:new").keys, "");
        assert_eq!(row(&report, "tab:close").keys, "⌘T");
        let conflict = report.conflict.unwrap();
        assert_eq!(conflict.holder, "tab:close");
        assert_eq!(conflict.asked, "tab:new");
        assert_eq!(conflict.give.as_deref(), Some("give ⌘T to New Tab"));
        assert_eq!(conflict.keep, "keep the old one");
    }

    #[test]
    fn given_a_conflict_opened_by_a_return_to_default_when_it_is_settled_then_it_settles_like_any_other(
    ) {
        // Given — le bloc n'a pas deux formes selon d'où il vient : ses deux issues font la
        // même chose, et le choix survit au redémarrage comme celui d'une capture
        let store = Arc::new(FakeStore::default());
        let bindings = two_tabs()
            .store(Arc::clone(&store) as Arc<dyn BindingStore>)
            .build();
        bindings
            .bind("tab:close", &stroke("t", "KeyT", true, false))
            .unwrap();
        bindings.resolve(ConflictChoice::Give);
        bindings.reset("tab:new").unwrap();

        // When
        let rebound = bindings.resolve(ConflictChoice::Give);

        // Then — `New Tab` retrouve son défaut, `Close Tab` se retrouve sans raccourci, et
        // la session suivante lit la même chose
        let report = bindings.report();
        assert_eq!(rebound, Rebound(true));
        assert_eq!(row(&report, "tab:new").keys, "⌘T");
        assert_eq!(row(&report, "tab:close").keys, "");
        assert_eq!(report.conflict, None);
        let next = two_tabs().store(store as Arc<dyn BindingStore>).build();
        assert_eq!(next.accelerator("tab:new").as_deref(), Some("Cmd+KeyT"));
        assert_eq!(next.accelerator("tab:close"), None);
    }

    #[test]
    fn given_a_conflict_opened_by_a_return_to_default_when_the_old_one_is_kept_then_nothing_moves()
    {
        // Given
        let bindings = two_tabs().build();
        bindings
            .bind("tab:close", &stroke("t", "KeyT", true, false))
            .unwrap();
        bindings.resolve(ConflictChoice::Give);
        bindings.reset("tab:new").unwrap();

        // When
        bindings.resolve(ConflictChoice::Keep);

        // Then — la ligne reste sans raccourci plutôt que de reprendre un défaut que sa
        // voisine porte
        let report = bindings.report();
        assert_eq!(row(&report, "tab:new").keys, "");
        assert_eq!(row(&report, "tab:close").keys, "⌘T");
        assert_eq!(report.conflict, None);
    }

    #[test]
    fn given_two_defaults_that_would_collide_when_everything_is_reset_then_the_menu_still_carries_one_of_them(
    ) {
        // Given — les défauts d'Ash ne se disputent rien, et `menu.rs` en tient le test. Mais
        // `reset all` ne doit pas *croire* cette propriété : le jour où une entrée de menu
        // naîtrait sur une touche déjà prise, repartir des défauts poserait deux actions sur
        // la même combinaison — ce que le bloc de conflit interdit partout ailleurs
        let bindings = BindingsBuilder::new()
            .action("tab:new", "New Tab", Some("Cmd+T"))
            .action("tab:close", "Close Tab", Some("Cmd+T"))
            .build();
        bindings
            .bind("tab:close", &stroke("j", "KeyJ", true, false))
            .unwrap();

        // When
        bindings.reset_all();

        // Then — la première dans l'ordre du menu garde la combinaison, la seconde reste sans
        // raccourci : le même arbitrage que pour un fichier bricolé, et déterminé de la même
        // façon
        assert_eq!(bindings.accelerator("tab:new").as_deref(), Some("Cmd+KeyT"));
        assert_eq!(bindings.accelerator("tab:close"), None);
    }

    #[test]
    fn given_every_path_that_poses_a_combination_when_one_of_them_would_double_it_then_no_two_actions_carry_it(
    ) {
        // Given — cinq chemins mènent à une liaison, et l'invariant n'en a qu'un seul
        // endroit : avoir eu deux règles à deux endroits est exactement ce qui a laissé le
        // retour au défaut poser une combinaison déjà tenue (issue #134), et n'avoir compté
        // que les lignes **réglables** est ce qui laissait la famille des positions d'onglet
        // tenue par personne (issue #137)
        let three = || {
            BindingsBuilder::new()
                .action("tab:new", "New Tab", Some("Cmd+T"))
                .action("tab:close", "Close Tab", Some("Cmd+W"))
                .action("tab:clear", "Clear Scrollback", None)
                .family("tab:select:1", "Tab 1", "Cmd+1", "tab:select:9")
                .hidden("tab:select:9", "Tab 9", "Cmd+9")
        };
        let rows = [
            "tab:new",
            "tab:close",
            "tab:clear",
            "tab:select:1",
            "tab:select:9",
        ];

        // When — 1. une capture qui vise une combinaison prise
        let captured = three().build();
        captured
            .bind("tab:clear", &stroke("t", "KeyT", true, false))
            .unwrap();

        // 2. un retour au défaut, sur une combinaison qu'un `give` a déplacée
        let restored = three().build();
        restored
            .bind("tab:close", &stroke("t", "KeyT", true, false))
            .unwrap();
        restored.resolve(ConflictChoice::Give);
        restored.reset("tab:new").unwrap();

        // 3. `reset all`
        let all = three().build();
        all.bind("tab:clear", &stroke("t", "KeyT", true, false))
            .unwrap();
        all.resolve(ConflictChoice::Give);
        all.reset_all();

        // 4. un fichier édité à la main, qui pose la même touche deux fois — et qui pose
        //    aussi `⌘1`, que la famille des positions d'onglet tient sans pouvoir la céder
        let store = Arc::new(FakeStore::default());
        let mut bricolage = BTreeMap::new();
        for action in ["tab:new", "tab:close"] {
            bricolage.insert(
                action.to_owned(),
                Some(Combination::parse("Cmd+KeyJ").unwrap()),
            );
        }
        bricolage.insert(
            "tab:clear".to_owned(),
            Some(Combination::parse("Cmd+Digit1").unwrap()),
        );
        store
            .save(&StoredBindings {
                bindings: bricolage,
            })
            .unwrap();
        let edited = three().store(store as Arc<dyn BindingStore>).build();

        // 5. une capture qui vise une combinaison tenue par une ligne **fixe**
        let fixed = three().build();
        fixed
            .bind("tab:new", &stroke("1", "Digit1", true, false))
            .unwrap();
        // le bloc est lu avant d'être refermé : c'est lui qui porte le refus
        let refusal = fixed.report().conflict.unwrap();
        // et l'issue qu'il n'offre pas, demandée quand même : elle ne prend rien à personne
        fixed.resolve(ConflictChoice::Give);

        // Then — aucun chemin ne pose deux fois la même touche, et les trois qui devaient
        // s'arrêter se sont arrêtés sur le bloc plutôt qu'en silence
        for settled in [&captured, &restored, &all, &edited, &fixed] {
            assert_eq!(doubled(settled, &rows), Vec::<String>::new());
        }
        assert!(captured.report().conflict.is_some());
        assert!(restored.report().conflict.is_some());
        // Le refus nomme la famille qui tient la touche, et n'offre que « garder l'ancien » :
        // `⌘1` reste à la sélection d'onglet, et New Tab garde ce qu'elle avait
        assert_eq!(refusal.holder_label, "Tab 1 … Tab 9");
        assert_eq!(refusal.give, None);
        assert!(refusal.diagnosis.contains("⌘1 belongs to Tab 1 … Tab 9"));
        assert_eq!(fixed.accelerator("tab:new").as_deref(), Some("Cmd+KeyT"));
        assert_eq!(
            fixed.accelerator("tab:select:1").as_deref(),
            Some("Cmd+Digit1")
        );
        // Et le fichier bricolé est tranché à la relecture, par le même chemin : `⌘1` n'est
        // pas volée à la famille, et la ligne qui la réclamait repart sans raccourci
        assert_eq!(edited.accelerator("tab:clear"), None);
    }

    #[test]
    fn given_several_changed_lines_when_everything_is_reset_then_all_of_them_are_back_to_default() {
        // Given
        let bindings = two_tabs().build();
        bindings
            .bind("tab:new", &stroke("j", "KeyJ", true, false))
            .unwrap();
        bindings.clear("tab:close").unwrap();

        // When
        bindings.reset_all();

        // Then
        let report = bindings.report();
        assert_eq!(report.changed, 0);
        assert_eq!(row(&report, "tab:close").keys, "⌘W");
    }

    #[test]
    fn given_a_binding_chosen_in_a_previous_session_when_ash_starts_again_then_the_menu_opens_with_it(
    ) {
        // Given — le critère « le choix survit au redémarrage », et il se vérifie du côté
        // du **menu** : c'est l'accélérateur qui doit repartir avec la bonne touche
        let store = Arc::new(FakeStore::default());
        two_tabs()
            .store(Arc::clone(&store) as Arc<dyn BindingStore>)
            .build()
            .bind("tab:new", &stroke("j", "KeyJ", true, false))
            .unwrap();

        // When — la session suivante
        let next = two_tabs().store(store as Arc<dyn BindingStore>).build();

        // Then
        assert_eq!(next.accelerator("tab:new").as_deref(), Some("Cmd+KeyJ"));
    }

    #[test]
    fn given_a_removed_shortcut_when_ash_starts_again_then_it_is_still_removed_and_not_back_to_default(
    ) {
        // Given — « aucun raccourci » n'est pas « pas de choix » : sans les distinguer sur
        // le disque, un `⌫` se serait défait tout seul au redémarrage
        let store = Arc::new(FakeStore::default());
        two_tabs()
            .store(Arc::clone(&store) as Arc<dyn BindingStore>)
            .build()
            .clear("tab:new")
            .unwrap();

        // When
        let next = two_tabs().store(store as Arc<dyn BindingStore>).build();

        // Then
        assert_eq!(next.accelerator("tab:new"), None);
    }

    #[test]
    fn given_a_conflict_settled_by_giving_the_combination_away_when_ash_starts_again_then_it_is_still_given(
    ) {
        // Given — le geste que le bloc de conflit existe pour offrir : `⌘W` passe de
        // `Close Tab` à `New Tab`, et l'ancienne détentrice se retrouve sans raccourci.
        // C'est le seul geste qui **déplace** une combinaison, donc le seul que relire le
        // fichier action par action, sur un monde encore à demi aux défauts, pouvait défaire
        let store = Arc::new(FakeStore::default());
        let chosen = two_tabs()
            .store(Arc::clone(&store) as Arc<dyn BindingStore>)
            .build();
        chosen
            .bind("tab:new", &stroke("w", "KeyW", true, false))
            .unwrap();
        chosen.resolve(ConflictChoice::Give);

        // When — la session suivante
        let next = two_tabs().store(store as Arc<dyn BindingStore>).build();

        // Then — et non `⌘T` revenu tout seul sur `New Tab`, ce qui aurait rendu le bloc de
        // conflit inutile dès le redémarrage
        assert_eq!(next.accelerator("tab:new").as_deref(), Some("Cmd+KeyW"));
        assert_eq!(next.accelerator("tab:close"), None);
    }

    #[test]
    fn given_two_shortcuts_traded_for_one_another_when_ash_starts_again_then_they_are_still_traded()
    {
        // Given — échanger `⌘T` et `⌘W`, le cas qui a dicté toute la tranche : chacune des
        // deux lignes porte la combinaison que l'autre avait par défaut
        let store = Arc::new(FakeStore::default());
        let chosen = two_tabs()
            .store(Arc::clone(&store) as Arc<dyn BindingStore>)
            .build();
        chosen
            .bind("tab:new", &stroke("w", "KeyW", true, false))
            .unwrap();
        chosen.resolve(ConflictChoice::Give);
        chosen
            .bind("tab:close", &stroke("t", "KeyT", true, false))
            .unwrap();

        // When
        let next = two_tabs().store(store as Arc<dyn BindingStore>).build();

        // Then
        assert_eq!(next.accelerator("tab:new").as_deref(), Some("Cmd+KeyW"));
        assert_eq!(next.accelerator("tab:close").as_deref(), Some("Cmd+KeyT"));
    }

    #[test]
    fn given_a_shortcuts_file_edited_by_hand_when_it_puts_two_actions_on_one_combination_then_the_menu_gets_only_one(
    ) {
        // Given — le fichier est éditable à la main, et un menu natif laisse gagner la
        // dernière entrée posée sans rien dire : c'est exactement ce que le bloc de conflit
        // existe pour éviter, donc le fichier ne doit pas pouvoir le contourner
        let store = Arc::new(FakeStore::default());
        let mut bricolage = BTreeMap::new();
        bricolage.insert(
            "tab:new".to_owned(),
            Some(Combination::parse("Cmd+KeyJ").unwrap()),
        );
        bricolage.insert(
            "tab:close".to_owned(),
            Some(Combination::parse("Cmd+KeyJ").unwrap()),
        );
        store
            .save(&StoredBindings {
                bindings: bricolage,
            })
            .unwrap();

        // When
        let bindings = two_tabs().store(store as Arc<dyn BindingStore>).build();

        // Then — la première dans l'ordre du menu est honorée, la seconde repart de son
        // défaut
        assert_eq!(bindings.accelerator("tab:new").as_deref(), Some("Cmd+KeyJ"));
        assert_eq!(
            bindings.accelerator("tab:close").as_deref(),
            Some("Cmd+KeyW")
        );
    }

    #[test]
    fn given_bindings_that_cannot_be_written_when_one_is_captured_then_it_still_applies() {
        // Given — `~/.ash` non inscriptible : refuser la liaison pour cette raison serait
        // incompréhensible pour qui vient de la poser. Même règle que le thème
        let bindings = two_tabs()
            .store(Arc::new(FakeStore::read_only()) as Arc<dyn BindingStore>)
            .build();

        // When
        bindings
            .bind("tab:new", &stroke("j", "KeyJ", true, false))
            .unwrap();

        // Then
        assert_eq!(bindings.accelerator("tab:new").as_deref(), Some("Cmd+KeyJ"));
    }

    #[test]
    fn given_the_family_of_tab_positions_when_it_is_listed_then_it_is_one_line_that_cannot_be_captured(
    ) {
        // Given — neuf positions rendues réglables une par une feraient neuf lignes presque
        // identiques, et une famille à demi rebindée ne veut rien dire (spec §4.4)
        let bindings = BindingsBuilder::new()
            .family("tab:select:1", "Tab 1", "Cmd+1", "tab:select:9")
            .action("tab:select:9", "Tab 9", Some("Cmd+9"))
            .build();

        // When
        let report = bindings.report();
        let refused = bindings.bind("tab:select:1", &stroke("j", "KeyJ", true, false));

        // Then
        let family = row(&report, "tab:select:1");
        assert_eq!(family.label, "Tab 1 … Tab 9");
        assert_eq!(family.keys, "⌘1 … ⌘9");
        assert!(!family.rebindable);
        assert!(matches!(refused, Err(ShortcutError::FixedBinding { .. })));
    }

    #[test]
    fn given_a_french_keyboard_when_a_tab_position_key_is_captured_then_the_refusal_names_the_key_pressed_and_the_family_holding_it(
    ) {
        // Given — sur un AZERTY, la touche qui joue `⌘2` est marquée `é`, et c'est macOS qui
        // en décide : la liaison, elle, reste `Cmd+Digit2`. Un refus qui parlerait de `⌘2`
        // nommerait une touche que l'utilisateur n'a nulle part, et `Tab 2` une ligne que
        // l'écran ne montre pas — elle se lit sous la famille
        let bindings = BindingsBuilder::new()
            .action("tab:new", "New Tab", Some("Cmd+T"))
            .family("tab:select:1", "Tab 1", "Cmd+1", "tab:select:9")
            .hidden("tab:select:2", "Tab 2", "Cmd+2")
            .hidden("tab:select:9", "Tab 9", "Cmd+9")
            .build();

        // When
        bindings
            .bind("tab:new", &stroke("é", "Digit2", true, false))
            .unwrap();

        // Then — le refus parle de la touche pressée, nomme la famille, et n'a qu'une issue
        let refusal = bindings.report().conflict.unwrap();
        assert_eq!(refusal.keys, "⌘É");
        assert_eq!(refusal.holder_label, "Tab 1 … Tab 9");
        assert_eq!(
            refusal.diagnosis,
            "⌘É belongs to Tab 1 … Tab 9 — that row is not rebindable, and ash will not take it away"
        );
        assert_eq!(refusal.give, None);
        assert_eq!(refusal.keep, "keep the old one");
    }

    #[test]
    fn given_an_action_with_no_shortcut_to_offer_when_the_rows_are_listed_then_it_has_no_line() {
        // Given — les trois entrées de thème : un thème se change une fois par saison, et
        // chaque raccourci pris ici est un raccourci perdu pour le shell
        let bindings = BindingsBuilder::new()
            .action("tab:new", "New Tab", Some("Cmd+T"))
            .build();
        let hidden = Bindings::restore(
            Arc::new(FakeStore::default()),
            vec![ActionBinding {
                action: "view:theme:dark".to_owned(),
                group: "view".to_owned(),
                label: "Dark".to_owned(),
                default: None,
                listing: Listing::Hidden,
            }],
        );

        // When / Then
        assert_eq!(bindings.report().rows.len(), 1);
        assert!(hidden.report().rows.is_empty());
    }

    #[test]
    fn given_a_combination_macos_takes_when_it_is_captured_then_it_is_posed_and_announced() {
        // Given — la règle de la planche : « une combinaison prise par macOS ou avalée par
        // le terminal n'est pas interdite — elle est annoncée comme inefficace »
        let bindings = two_tabs().build();
        let taken = KeyStroke {
            key: "f".to_owned(),
            code: "KeyF".to_owned(),
            command: true,
            control: true,
            option: false,
            shift: false,
        };

        // When
        let previewed = bindings.preview(&taken);
        bindings.bind("tab:new", &taken).unwrap();

        // Then — elle est posable, et la ligne comme la capture le disent
        assert!(previewed.accepted);
        assert!(previewed.reservation.is_some());
        let listed = bindings.report();
        assert_eq!(row(&listed, "tab:new").keys, "⌃⌘F");
        assert!(row(&listed, "tab:new").reservation.is_some());
    }

    #[test]
    fn given_a_bare_key_when_it_is_previewed_then_the_capture_says_why_it_cannot_be_confirmed() {
        // Given — la raison vient du backend, comme la règle : une phrase écrite dans la
        // webview aurait fini par expliquer un refus que le backend ne fait plus
        let bindings = two_tabs().build();

        // When
        let previewed = bindings.preview(&stroke("j", "KeyJ", false, false));

        // Then
        assert!(!previewed.accepted);
        assert_eq!(previewed.keys, "");
        assert!(previewed.why.is_some());
    }

    #[test]
    fn given_a_line_when_its_own_combination_is_captured_again_then_it_is_neither_a_change_nor_a_conflict(
    ) {
        // Given — on ouvre la capture et on refrappe la même touche
        let bindings = two_tabs().build();

        // When
        let rebound = bindings
            .bind("tab:new", &stroke("t", "KeyT", true, false))
            .unwrap();

        // Then — sans ça, une ligne serait en conflit avec elle-même, et le menu se
        // referait pour rien
        assert_eq!(rebound, Rebound(false));
        assert_eq!(bindings.report().conflict, None);
        assert_eq!(bindings.report().changed, 0);
    }
}
