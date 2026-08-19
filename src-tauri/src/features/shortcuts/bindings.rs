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

/// Les deux lignes d'un conflit, et les deux issues qui le referment.
///
/// Il naît d'une capture qui viserait une combinaison déjà prise, et **rien n'est appliqué
/// tant qu'il vit** : c'est la règle de la planche, « ash ne réattribue jamais en silence ».
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ShortcutConflict {
    /// La combinaison disputée, en glyphes.
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
    pub give: String,
    /// Le libellé de l'issue secondaire — `keep the old one`.
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
    holder: String,
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

        // 2. Une combinaison n'est tenue que par une action. Le parcours est dans l'ordre du
        //    **menu** et non celui du fichier : c'est ce qui rend l'arbitrage déterministe
        //    d'une relecture à l'autre. Sans lui, un fichier bricolé poserait deux fois la
        //    même touche dans le menu natif, où c'est la dernière posée qui gagne, en
        //    silence — exactement ce que le bloc de conflit existe pour éviter.
        let mut claimed: Vec<Combination> = Vec::new();
        for declared in self.actions.iter().filter(|one| one.rebindable()) {
            // Une ligne sans raccourci ne dispute rien à personne.
            let Some(in_use) = effective(declared, &kept) else {
                continue;
            };
            if !claimed.contains(&in_use) {
                claimed.push(in_use);
                continue;
            }
            match &declared.default {
                // Son défaut est libre : elle y repart, comme si le fichier n'avait rien dit
                // d'elle.
                Some(free) if !claimed.contains(free) => {
                    kept.remove(&declared.action);
                    claimed.push(free.clone());
                }
                // Son défaut est pris lui aussi — ou elle n'en a pas : elle reste **sans**
                // raccourci plutôt que d'en porter un que le menu donnerait à une autre.
                _ => {
                    kept.insert(declared.action.clone(), None);
                }
            }
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
            Listing::Row => (declared.label.clone(), written(&in_use)),
            Listing::Family { through } => {
                let last = self.actions.iter().find(|one| &one.action == through)?;
                (
                    format!("{} … {}", declared.label, last.label),
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

    fn conflict(&self, pending: &Pending) -> ShortcutConflict {
        let name = |action: &str| {
            self.actions
                .iter()
                .find(|one| one.action == action)
                .map(|one| one.label.clone())
                .unwrap_or_else(|| action.to_owned())
        };
        let keys = pending.combination.glyphs();
        let asked_label = name(&pending.action);
        ShortcutConflict {
            holder: pending.holder.clone(),
            holder_label: name(&pending.holder),
            asked: pending.action.clone(),
            // Le diagnostic dit ce qui se passerait **sans** ce bloc, et c'est le point :
            // dans un menu natif, deux entrées sur la même touche laissent gagner la
            // dernière posée, sans rien dire à personne.
            diagnosis: format!("two actions on {keys} — the last one set would silently win"),
            give: format!("give {keys} to {asked_label}"),
            keep: "keep the old one".to_owned(),
            asked_label,
            keys,
        }
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

        let mut chosen = self.locked();
        // Une capture qui redonne à une ligne ce qu'elle a déjà n'est pas un changement, et
        // surtout pas un conflit avec elle-même.
        if effective(declared, &chosen.overrides).as_ref() == Some(&combination) {
            chosen.pending = None;
            return Ok(Rebound(false));
        }

        let holder = self.actions.iter().find(|other| {
            other.action != declared.action
                && other.rebindable()
                && effective(other, &chosen.overrides).as_ref() == Some(&combination)
        });
        match holder {
            Some(held) => {
                chosen.pending = Some(Pending {
                    action: declared.action.clone(),
                    combination,
                    holder: held.action.clone(),
                });
                Ok(Rebound(false))
            }
            None => {
                chosen.pending = None;
                chosen
                    .overrides
                    .insert(declared.action.clone(), Some(combination));
                self.keep(chosen);
                Ok(Rebound(true))
            }
        }
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
        let mut chosen = self.locked();
        if effective(declared, &chosen.overrides).is_none() {
            return Ok(Rebound(false));
        }
        chosen.pending = None;
        chosen.overrides.insert(declared.action.clone(), None);
        self.keep(chosen);
        Ok(Rebound(true))
    }

    /// Rend son défaut à une ligne — l'icône de retour, qui n'existe que sur les lignes
    /// changées.
    pub fn reset(&self, action: &str) -> Result<Rebound, ShortcutError> {
        let declared = self.declared(action)?;
        let mut chosen = self.locked();
        if chosen.overrides.remove(&declared.action).is_none() {
            return Ok(Rebound(false));
        }
        chosen.pending = None;
        self.keep(chosen);
        Ok(Rebound(true))
    }

    /// `reset all` — toutes les lignes reprennent leur défaut.
    pub fn reset_all(&self) -> Rebound {
        let mut chosen = self.locked();
        if chosen.overrides.is_empty() && chosen.pending.is_none() {
            return Rebound(false);
        }
        let rebound = !chosen.overrides.is_empty();
        chosen.overrides.clear();
        chosen.pending = None;
        self.keep(chosen);
        Rebound(rebound)
    }

    /// Referme un conflit par l'une de ses deux issues.
    ///
    /// `Give` ne se contente pas de poser la combinaison sur la nouvelle action : elle
    /// **retire** celle de l'ancienne. Les laisser toutes les deux serait rouvrir le conflit
    /// que le geste vient de fermer.
    pub fn resolve(&self, choice: ConflictChoice) -> Rebound {
        let mut chosen = self.locked();
        let Some(pending) = chosen.pending.take() else {
            return Rebound(false);
        };
        match choice {
            ConflictChoice::Keep => Rebound(false),
            ConflictChoice::Give => {
                chosen.overrides.insert(pending.holder, None);
                chosen
                    .overrides
                    .insert(pending.action, Some(pending.combination));
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

    fn stroke(code: &str, command: bool, control: bool) -> KeyStroke {
        KeyStroke {
            code: code.to_owned(),
            command,
            control,
            option: false,
            shift: false,
        }
    }

    fn row<'a>(report: &'a ShortcutsReport, action: &str) -> &'a ShortcutRow {
        report
            .rows
            .iter()
            .find(|row| row.action == action)
            .expect("la ligne demandée")
    }

    #[test]
    fn given_a_line_at_its_default_when_a_free_combination_is_captured_then_the_line_carries_it() {
        // Given
        let bindings = two_tabs().build();

        // When
        let rebound = bindings
            .bind("tab:new", &stroke("KeyJ", true, false))
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
            .bind("tab:new", &stroke("KeyJ", true, false))
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
            .bind("tab:new", &stroke("KeyW", true, false))
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
        assert_eq!(conflict.give, "give ⌘W to New Tab");
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
            .bind("tab:new", &stroke("KeyW", true, false))
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
            .bind("tab:new", &stroke("KeyW", true, false))
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
            .bind("tab:new", &stroke("KeyJ", true, false))
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
    fn given_several_changed_lines_when_everything_is_reset_then_all_of_them_are_back_to_default() {
        // Given
        let bindings = two_tabs().build();
        bindings
            .bind("tab:new", &stroke("KeyJ", true, false))
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
            .bind("tab:new", &stroke("KeyJ", true, false))
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
            .bind("tab:new", &stroke("KeyW", true, false))
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
            .bind("tab:new", &stroke("KeyW", true, false))
            .unwrap();
        chosen.resolve(ConflictChoice::Give);
        chosen
            .bind("tab:close", &stroke("KeyT", true, false))
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
            .bind("tab:new", &stroke("KeyJ", true, false))
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
        let refused = bindings.bind("tab:select:1", &stroke("KeyJ", true, false));

        // Then
        let family = row(&report, "tab:select:1");
        assert_eq!(family.label, "Tab 1 … Tab 9");
        assert_eq!(family.keys, "⌘1 … ⌘9");
        assert!(!family.rebindable);
        assert!(matches!(refused, Err(ShortcutError::FixedBinding { .. })));
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
        let previewed = bindings.preview(&stroke("KeyJ", false, false));

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
            .bind("tab:new", &stroke("KeyT", true, false))
            .unwrap();

        // Then — sans ça, une ligne serait en conflit avec elle-même, et le menu se
        // referait pour rien
        assert_eq!(rebound, Rebound(false));
        assert_eq!(bindings.report().conflict, None);
        assert_eq!(bindings.report().changed, 0);
    }
}
