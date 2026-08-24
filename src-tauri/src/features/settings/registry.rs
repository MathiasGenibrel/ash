use std::sync::{Arc, Mutex};

use super::error::SettingsError;
use super::hooks::{report, BlockAt, HookAction};
use super::persisted::PersistedTools;
use super::ports::{HookBlocks, Launch};
use super::store::ToolStore;
use super::tool::{NewTool, ToolDeclaration};
use super::values::{optional, Command, ConfigTarget};
use super::verification::{FirstPass, Verification, Verifier};
use super::withdrawal::{self, Outcome, PlannedRemoval, RemovalPlan, RemovalReport, RemovedFile};
use crate::features::agents::Instrumented;
use crate::features::hooks::{Presence, Removal};

/// Les commandes déclarées, ce qu'elles ont prouvé, et les adaptateurs qu'on peut leur
/// donner.
///
/// **Le registre est en Rust, et pas dans un état de la webview**, pour la même raison que
/// les onglets et le thème : le frontend rend ce que le backend détient
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ici la raison est
/// même plus forte qu'ailleurs — ces déclarations sont ce qui fera reconnaître un agent
/// dans la sidebar (ADR-0006) et ce qui décidera où poser des hooks (ADR-0007) ; ni l'une
/// ni l'autre de ces lectures ne passe par la fenêtre de réglages.
///
/// **La vérification y vit aussi, et c'est ce qui garde la règle d'écriture unique.** Le
/// oui/non qui autorise les hooks est calculé par [`Verifier`] et transporté tel quel ; la
/// fenêtre l'annonce sans le rejouer.
///
/// **Ce qui est déclaré est gardé dans `~/.ash/tools.json`**, et relu au lancement suivant.
/// Déclarer un outil est le geste qui fait qu'un onglet devient un agent (ADR-0006) et qui
/// décide où poser les hooks (ADR-0007) : le perdre au redémarrage laissait l'écran dire
/// « no tools declared » pendant que les hooks, eux, étaient toujours posés sur le disque.
///
/// **Ce qui est gardé est la déclaration, jamais ce qu'elle a prouvé.** Une entrée relue
/// repart *non vérifiée* et se revérifie comme une entrée saisie : un dossier peut avoir
/// disparu entre deux lancements, et la ligne `hooks` relit le fichier de l'utilisateur à
/// chaque affichage plutôt que de s'en souvenir (ADR-0007). Ce que le fichier porte en plus
/// de la saisie est le **dernier dossier valide**, sans lequel « réinitialiser une entrée »
/// ramènerait après un redémarrage au défaut de l'adaptateur (spec §9.1).
pub struct ToolRegistry {
    verifier: Arc<Verifier>,
    /// Le seul chemin par lequel Ash écrit chez l'utilisateur, et il est derrière un trait :
    /// la feature ne connaît aucun adaptateur concret (ADR-0008).
    blocks: Arc<dyn HookBlocks>,
    /// Là où les déclarations survivent à la fermeture de la fenêtre.
    store: Arc<dyn ToolStore>,
    tools: Mutex<Vec<ToolDeclaration>>,
    /// Ce que le disque porte déjà, tel que ce registre l'a écrit ou relu.
    ///
    /// Il évite de réécrire le fichier à chaque vérification : les quatre tests changent
    /// l'entrée en mémoire sans rien changer de ce qui est gardé, et `re-verify all` sur
    /// six entrées ne doit pas produire six écritures identiques.
    saved: Mutex<PersistedTools>,
}

/// Ce qu'il reste à lancer après un premier temps, et pour quelle entrée.
///
/// Il porte l'adaptateur et le chemin **tels qu'ils étaient au moment du premier temps** :
/// une commande peut mettre cinq secondes à répondre, et l'utilisateur peut avoir changé de
/// chemin entre-temps. Sans cette empreinte, le résultat d'une vérification périmée
/// écraserait celle qui la remplace.
#[derive(Debug, Clone)]
pub struct SecondPass {
    pub command: Command,
    pub adapter: String,
    pub config: Option<String>,
    pub launch: Launch,
    /// L'entrée est-elle au registre ? Une saisie du formulaire d'ajout n'y est pas encore,
    /// et son résultat ne doit se poser sur aucune entrée existante.
    pub stored: bool,
}

/// Un fichier que le retrait global vise : où il est, et ce qu'on y a vu.
///
/// Il ne traverse aucune frontière — seul son [`PlannedRemoval`] va à l'écran. Ce qu'il
/// porte en plus est ce dont le second temps a besoin pour écrire au même endroit que
/// celui qu'il a annoncé.
struct Aimed {
    adapter: String,
    folder: ConfigTarget,
    planned: PlannedRemoval,
}

/// Ce qu'une modification du registre rend : la liste entière, et ce qui reste à lancer.
pub struct Changed {
    pub tools: Vec<ToolDeclaration>,
    pub pending: Vec<SecondPass>,
}

impl ToolRegistry {
    /// Le registre tel que la session précédente l'a laissé.
    ///
    /// **Rien n'est vérifié ici, et rien n'est lancé.** Les quatre tests de la spec §9.1
    /// lisent des dossiers et lancent une commande ; les faire au démarrage retarderait
    /// l'ouverture de la fenêtre pour un résultat que personne ne regarde encore, et
    /// afficherait un verdict daté du lancement. Les entrées relues sont *non vérifiées*,
    /// et c'est la fenêtre qui relance la séquence en s'ouvrant.
    ///
    /// Les entrées passent par les **mêmes constructeurs** qu'une saisie du formulaire —
    /// [`NewTool::restore`], donc [`Command`] et [`Verifier::target`] : un fichier édité à
    /// la main ne peut pas faire entrer dans le registre ce que le formulaire aurait refusé.
    /// Ce qui ne passe pas est ignoré ; ce qui est à côté est chargé.
    pub fn restore(
        verifier: Arc<Verifier>,
        blocks: Arc<dyn HookBlocks>,
        store: Arc<dyn ToolStore>,
    ) -> Self {
        let found = store.load();
        let mut tools: Vec<ToolDeclaration> = Vec::new();
        for stored in &found.tools {
            let restored = NewTool {
                command: stored.command.clone(),
                label: stored.label.clone(),
                adapter: stored.adapter.clone(),
                config: stored.config.clone(),
            }
            .restore(&tools);
            let Ok(tool) = restored else {
                continue;
            };
            // La mémoire est un **dossier**, et il n'y a qu'un producteur de ce type :
            // le chemin gardé est relu comme s'il venait du champ de l'entrée, `~` compris.
            let remembered = optional(stored.last_valid_config.as_deref())
                .and_then(|raw| verifier.target(&tool.adapter, Some(&raw)));
            tools.push(tool.remembering(remembered));
        }
        // Ce qu'on vient de lire fait foi : une entrée ignorée ne disparaît du fichier qu'au
        // prochain geste qui l'écrit, et jamais parce qu'Ash s'est lancé.
        let saved = PersistedTools::of(&tools);

        Self {
            verifier,
            blocks,
            store,
            tools: Mutex::new(tools),
            saved: Mutex::new(saved),
        }
    }

    /// Les adaptateurs proposés par le formulaire d'ajout.
    pub fn adapters(&self) -> Vec<String> {
        self.verifier.adapters()
    }

    /// Le dossier que le formulaire d'ajout propose pour cet adaptateur, s'il existe.
    ///
    /// Un passe-plat vers [`Verifier::proposed_config`], comme [`Self::adapters`] : les
    /// profils appartiennent au vérificateur, et le registre est ce que la fenêtre a sous la
    /// main. Elle **lit un dossier** — un seul, celui que l'adaptateur nomme — et son
    /// appelant est le geste qui ouvre l'écran, pas une boucle.
    pub fn proposed_config(&self, adapter: &str) -> Option<String> {
        self.verifier.proposed_config(adapter)
    }

    /// Les entrées telles qu'elles sont retenues, **sans rien demander au disque**.
    ///
    /// C'est ce que [`Self::tools`] enrichit avant de le rendre à la fenêtre. La
    /// reconnaissance d'ADR-0006, elle, n'a besoin que de la déclaration brute — et elle est
    /// consultée à chaque passe de la boucle de sonde, pour chaque onglet : lui faire
    /// relire un `settings.json` par passe serait un fichier ouvert trois fois par seconde
    /// et par onglet, pour une réponse qui ne change qu'à la minute.
    pub fn declarations(&self) -> Result<Vec<ToolDeclaration>, SettingsError> {
        Ok(self.lock()?.clone())
    }

    /// La configuration de cet outil porte-t-elle le marqueur d'Ash ?
    ///
    /// **Trois réponses et non un oui/non**, parce que « rien n'est posé » et « rien ne peut
    /// l'être » ne se corrigent pas du tout de la même façon : la première mène au flux
    /// d'installation qui existe déjà, la seconde n'a pas de geste — aucun adaptateur de
    /// cette version ne sait instrumenter cet outil (ADR-0008). Les confondre ferait
    /// proposer un bouton qui n'écrirait jamais rien.
    ///
    /// La question est bien « le marqueur est-il là ? » et pas « le bloc est-il celui qu'on
    /// écrirait » : un bloc d'une version antérieure, ou modifié à la main, **est** une
    /// instrumentation — l'outil parle, et c'est la fenêtre de réglages qui dit dans quel
    /// état elle est. La sidebar, elle, ne signale que ce qui explique une absence de
    /// `waiting` ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
    ///
    /// **Elle lit un fichier** : son appelant est seul responsable de ne pas la poser trois
    /// fois par seconde (voir [`super::recognition`]).
    pub fn instrumentation(&self, adapter: &str, config: Option<&str>) -> Instrumented {
        let Some(target) = self.verifier.target(adapter, config) else {
            return Instrumented::Unsupported;
        };
        match self.blocks.inspect(adapter, &target) {
            None => Instrumented::Unsupported,
            Some(BlockAt { presence, .. }) => match presence {
                Presence::Current { .. }
                | Presence::Superseded { .. }
                | Presence::HandEdited { .. } => Instrumented::Installed,
                Presence::Missing { .. } | Presence::NotAnObject | Presence::Unreadable { .. } => {
                    Instrumented::Missing
                }
            },
        }
    }

    pub fn tools(&self) -> Result<Vec<ToolDeclaration>, SettingsError> {
        let stored = self.lock()?.clone();
        Ok(self.enrich(stored))
    }

    /// Retient une saisie, ou dit pourquoi elle n'en est pas une, puis la vérifie.
    ///
    /// La vérification part **dans la foulée** et non sur demande : une entrée qui vient
    /// d'être ajoutée sans rien avoir prouvé afficherait `unverified` alors que rien
    /// n'attend l'utilisateur — et la maquette relance justement « à tout changement de
    /// chemin ou d'adaptateur ».
    pub fn declare(&self, draft: NewTool) -> Result<Changed, SettingsError> {
        let command = {
            let mut tools = self.lock()?;
            // Le verrou est tenu pendant le jugement **et** l'ajout : deux ajouts
            // concurrents de la même commande passeraient tous les deux si l'unicité était
            // vérifiée en dehors, et le registre porterait alors deux entrées homonymes.
            let declared = draft.declare(&self.adapters(), &tools)?;
            let command = declared.command.clone();
            tools.push(declared);
            command
        };
        self.verify(&command)
    }

    /// Change le dossier ou l'adaptateur d'une entrée, et la re-vérifie.
    ///
    /// Les deux ensemble parce qu'ils se relancent ensemble : appliquer la correction
    /// proposée par un état invalide change l'un **ou** l'autre, et la vérification qui
    /// suit est la même.
    pub fn retarget(
        &self,
        command: &Command,
        adapter: &str,
        config: Option<&str>,
    ) -> Result<Changed, SettingsError> {
        {
            let mut tools = self.lock()?;
            let Some(tool) = tools.iter_mut().find(|tool| &tool.command == command) else {
                return Err(SettingsError::UnknownTool(command.clone()));
            };
            if !self.adapters().iter().any(|known| known == adapter) {
                return Err(SettingsError::UnknownAdapter(adapter.to_owned()));
            }
            let folder = optional(config);
            // L'entrée retombe à `unverified` **avant** que quoi que ce soit ne soit
            // relancé : entre la frappe et la réponse, ce que l'écran montrait décrivait
            // l'ancien chemin.
            let refreshed = tool
                .clone()
                .retargeted(adapter, folder)
                .verified_by(Verification::unverified(), None);
            *tool = refreshed;
        }
        self.verify(command)
    }

    /// Ramène une entrée à **son** dernier dossier valide (spec §9.1).
    ///
    /// Pas au défaut de son adaptateur : deux entrées partagent souvent un adaptateur, et y
    /// revenir les rendrait identiques — le doublon deviendrait la conséquence mécanique du
    /// geste au lieu d'un accident. Une entrée qui n'a jamais rien prouvé n'a donc rien à
    /// restaurer, et le dire vaut mieux que de la renvoyer quelque part au hasard.
    pub fn reset(&self, command: &Command) -> Result<Changed, SettingsError> {
        {
            let mut tools = self.lock()?;
            let Some(tool) = tools.iter_mut().find(|tool| &tool.command == command) else {
                return Err(SettingsError::UnknownTool(command.clone()));
            };
            let Some(memory) = tool.last_valid_config.clone() else {
                return Err(SettingsError::NothingToRestore(command.clone()));
            };
            let adapter = tool.adapter.clone();
            // Ce qu'on remplace est le dossier **désigné**, défaut de l'adaptateur compris :
            // la ligne `was` montre un chemin, et « rien » ne se barre pas d'un trait.
            let previous = self.verifier.target(&adapter, tool.config.as_deref());
            let mut restored = tool
                .clone()
                .retargeted(&adapter, Some(memory.declared().to_owned()))
                .verified_by(Verification::unverified(), None);
            // Rien à annuler quand rien n'a changé : offrir « annuler » ferait chercher ce
            // que le geste a fait. « Rien n'a changé » est la question du doublon, posée sur
            // la même valeur — deux écritures du même dossier ne sont pas un déplacement.
            restored.reset_from = previous.filter(|before| *before != memory);
            *tool = restored;
        }
        self.verify(command)
    }

    /// Annule la réinitialisation, tant qu'elle est la dernière chose qui s'est passée.
    ///
    /// C'est le `undo the reset` de la bannière de doublon : le geste qui vient de créer la
    /// collision est celui qu'on défait, et il reste **à portée** plutôt que d'obliger à
    /// retaper un chemin qu'on n'a plus sous les yeux.
    pub fn undo_reset(&self, command: &Command) -> Result<Changed, SettingsError> {
        {
            let mut tools = self.lock()?;
            let Some(tool) = tools.iter_mut().find(|tool| &tool.command == command) else {
                return Err(SettingsError::UnknownTool(command.clone()));
            };
            let Some(before) = tool.reset_from.clone() else {
                return Err(SettingsError::NothingToUndo(command.clone()));
            };
            let adapter = tool.adapter.clone();
            *tool = tool
                .clone()
                .retargeted(&adapter, Some(before.declared().to_owned()))
                .verified_by(Verification::unverified(), None);
        }
        self.verify(command)
    }

    /// Pose ou met à jour le bloc de hooks d'une entrée.
    ///
    /// **La garde est la ligne `hooks` elle-même**, et pas une seconde règle écrite ici :
    /// une entrée que la séquence n'autorise pas, un doublon, un bloc édité à la main ou un
    /// fichier qui porte déjà d'autres hooks ont tous éteint le bouton — et le backend
    /// refuse pour la même raison qu'il l'a éteint. Recopier la condition en ferait deux,
    /// dont une seule protège vraiment le fichier de l'utilisateur.
    ///
    /// **Elle garde l'écriture d'entrées nouvelles, pas leur reprise** :
    /// [`Self::remove_everything`] ne passe pas par ici, et c'est délibéré — l'y faire
    /// passer abandonnerait sur le disque les entrées d'une déclaration devenue invalide
    /// depuis. La raison est écrite là-bas, avec le geste qu'elle autorise.
    pub fn install_hooks(&self, command: &Command) -> Result<Vec<ToolDeclaration>, SettingsError> {
        self.write_hooks(command, HookAction::Install)
    }

    /// Retire le bloc et ses marqueurs — le `remove` de l'état `installed`.
    pub fn remove_hooks(&self, command: &Command) -> Result<Vec<ToolDeclaration>, SettingsError> {
        self.write_hooks(command, HookAction::Remove)
    }

    /// Tous les fichiers où Ash a écrit, et ce qu'un retrait y emporterait. **Lit, n'écrit
    /// pas.**
    ///
    /// C'est la première moitié de « retirer Ash de tous les fichiers » (spec §10) : le
    /// geste dit ce qu'il va faire avant de le faire, et l'écran ne peut le dire que si le
    /// backend le sait. Un fichier où il n'y a rien à retirer n'y figure pas.
    pub fn removal_plan(&self) -> Result<RemovalPlan, SettingsError> {
        Ok(withdrawal::plan(
            self.foresee()?
                .into_iter()
                .map(|aimed| aimed.planned)
                .collect(),
        ))
    }

    /// Retire les entrées d'Ash de tous ces fichiers, et rapporte ce qui a eu lieu.
    ///
    /// **Elle relit tout au moment d'écrire**, et ne croit pas l'annonce qui l'a précédée :
    /// un fichier que l'utilisateur a édité entre les deux ne se retrouve pas amputé de
    /// lignes qui ne sont plus là où on les avait vues. Le compte rendu dit alors ce qui a
    /// eu lieu, c'est-à-dire rien pour ce fichier-là.
    ///
    /// **Aucune vérification n'y donne droit, et c'est délibéré.** La séquence des quatre
    /// tests garde l'*écriture d'entrées nouvelles* : elle empêche Ash de poser des hooks
    /// dans un dossier qu'il n'a pas reconnu. Le retrait, lui, ne touche que ce qui porte
    /// déjà son marqueur — le refuser à une entrée devenue invalide entre-temps
    /// abandonnerait sur le disque exactement ce que ce geste existe pour reprendre.
    pub fn remove_everything(
        &self,
    ) -> Result<(RemovalReport, Vec<ToolDeclaration>), SettingsError> {
        let removed: Vec<RemovedFile> = self
            .foresee()?
            .into_iter()
            .map(|aimed| RemovedFile {
                entries: aimed.planned.entries,
                file: aimed.planned.file,
                outcome: match self.blocks.remove(&aimed.adapter, &aimed.folder) {
                    Ok(Removal::Removed {
                        deleted_the_file: true,
                        ..
                    }) => Outcome::RemovedTheFile,
                    Ok(Removal::Removed { .. }) => Outcome::Removed,
                    Ok(Removal::NothingToRemove { .. }) => Outcome::NothingLeft,
                    Err(why) => Outcome::Refused { why },
                },
            })
            .collect();

        Ok((withdrawal::report(removed), self.tools()?))
    }

    /// Ce que le retrait viserait, dans l'ordre des entrées déclarées.
    ///
    /// Elle lit les déclarations **brutes** ([`Self::declarations`]) et non [`Self::tools`] :
    /// la ligne `hooks` d'une entrée relit déjà chaque fichier pour l'écran, et le retrait
    /// n'a besoin ni de son verdict ni de son diff d'installation.
    fn foresee(&self) -> Result<Vec<Aimed>, SettingsError> {
        let mut aimed: Vec<Aimed> = Vec::new();
        for tool in self.declarations()? {
            let Some(folder) = self.verifier.target(&tool.adapter, tool.config.as_deref()) else {
                continue;
            };
            // Deux entrées sur le même dossier et le même adaptateur visent le même
            // fichier : un seul retrait, et les deux noms sur la même ligne.
            if let Some(seen) = aimed
                .iter_mut()
                .find(|seen| seen.adapter == tool.adapter && seen.folder == folder)
            {
                seen.planned.also_aimed_by(&tool.command);
                continue;
            }
            let Some(found) = self.blocks.foresee_removal(&tool.adapter, &folder) else {
                continue;
            };
            aimed.push(Aimed {
                adapter: tool.adapter.clone(),
                folder,
                planned: PlannedRemoval::foreseen(found, &tool.command),
            });
        }
        Ok(aimed)
    }

    fn write_hooks(
        &self,
        command: &Command,
        asked: HookAction,
    ) -> Result<Vec<ToolDeclaration>, SettingsError> {
        let tools = self.tools()?;
        let Some(tool) = tools.iter().find(|tool| &tool.command == command) else {
            return Err(SettingsError::UnknownTool(command.clone()));
        };
        // La ligne offre un geste, et le diff en offre d'autres : les deux comptent, et
        // c'est **la même** liste que celle qui a allumé les boutons. Un conflit propose
        // `see the diff` sur la ligne et `merge` dans le diff ; refuser le second parce
        // qu'il n'est pas le premier rendrait le choix inatteignable — le défaut même que
        // l'amendement du 2026-08-12 d'ADR-0007 est venu lever.
        let offered = std::iter::once(tool.hooks.action)
            .chain(tool.hooks.choices.iter().map(|choice| choice.action));
        let allowed = offered.into_iter().any(|offer| match asked {
            // `update` est une installation : le geste est le même, seul le mot change.
            HookAction::Install => matches!(offer, HookAction::Install | HookAction::Update),
            other => offer == other,
        });
        if !allowed || !tool.hooks.enabled {
            return Err(SettingsError::HooksRefused(tool.hooks.summary.clone()));
        }
        let Some(folder) = self.verifier.target(&tool.adapter, tool.config.as_deref()) else {
            return Err(SettingsError::NoConfigFolder(command.clone()));
        };

        match asked {
            HookAction::Remove => self.blocks.remove(&tool.adapter, &folder).map(|_| ()),
            _ => self.blocks.install(&tool.adapter, &folder),
        }
        .map_err(SettingsError::HooksRefused)?;

        self.tools()
    }

    /// Relance la séquence sur une entrée — le bouton `re-verify` d'une carte.
    pub fn verify(&self, command: &Command) -> Result<Changed, SettingsError> {
        let (stored, pending) = {
            let mut tools = self.lock()?;
            let Some(index) = tools.iter().position(|tool| &tool.command == command) else {
                return Err(SettingsError::UnknownTool(command.clone()));
            };
            let pending = verify_at(&self.verifier, &mut tools, index);
            (tools.clone(), pending.into_iter().collect())
        };
        self.persist(&stored);
        Ok(Changed {
            tools: self.enrich(stored),
            pending,
        })
    }

    /// Relance la séquence sur **toute** la liste — le bouton `re-verify all`.
    ///
    /// Les premiers temps se font ici, l'un après l'autre : ils ne lisent qu'un dossier et
    /// le `PATH`, et les faire en parallèle coûterait plus en fils qu'ils ne prennent de
    /// temps. Ce sont les seconds temps qui partent en parallèle — et c'est là que
    /// [`super::permits`] les borne.
    pub fn verify_all(&self) -> Result<Changed, SettingsError> {
        let (stored, pending) = {
            let mut tools = self.lock()?;
            let mut pending = Vec::new();
            for index in 0..tools.len() {
                pending.extend(verify_at(&self.verifier, &mut tools, index));
            }
            (tools.clone(), pending)
        };
        self.persist(&stored);
        Ok(Changed {
            tools: self.enrich(stored),
            pending,
        })
    }

    /// Lance le second temps. **Ne prend aucun verrou** : la commande peut mettre des
    /// secondes à répondre, et le registre doit rester lisible pendant ce temps.
    pub fn second_pass(&self, next: &SecondPass) -> Verification {
        self.verifier.second_pass(&next.command, &next.launch)
    }

    /// Pose le résultat du second temps sur l'entrée, **si elle décrit toujours la même
    /// chose**.
    ///
    /// Un résultat périmé est jeté sans bruit : il ne décrit pas ce que l'écran montre, et
    /// l'afficher ferait clignoter une réponse à une question qu'on ne pose plus.
    pub fn settle(
        &self,
        next: &SecondPass,
        verification: Verification,
    ) -> Result<Option<Vec<ToolDeclaration>>, SettingsError> {
        if !next.stored {
            return Ok(None);
        }
        let stored = {
            let mut tools = self.lock()?;
            let Some(tool) = tools.iter_mut().find(|tool| tool.command == next.command) else {
                return Ok(None);
            };
            if tool.adapter != next.adapter || tool.config != next.config {
                return Ok(None);
            }
            let declared = self.verifier.target(&tool.adapter, tool.config.as_deref());
            *tool = tool.clone().verified_by(verification, declared);
            tools.clone()
        };
        self.persist(&stored);
        Ok(Some(self.enrich(stored)))
    }

    /// Vérifie une saisie qui n'est pas encore déclarée — le formulaire d'ajout.
    ///
    /// Elle ne touche pas le registre : le bouton `add` a besoin de savoir ce que les
    /// quatre tests disent **avant** qu'il n'y ait quoi que ce soit à ajouter.
    pub fn verify_draft(
        &self,
        draft: &NewTool,
    ) -> Result<(Verification, Option<SecondPass>), SettingsError> {
        let command = Command::parse(&draft.command)?;
        let config = optional(draft.config.as_deref());
        let first = self
            .verifier
            .first_pass(&command, draft.adapter.trim(), config.as_deref());
        let shown = first.shown().clone();
        let pending = match first {
            FirstPass::Pending { launch, .. } => Some(SecondPass {
                command,
                adapter: draft.adapter.trim().to_owned(),
                config,
                launch,
                stored: false,
            }),
            FirstPass::Settled(_) => None,
        };
        Ok((shown, pending))
    }

    /// Oublie une entrée — le `✕` de l'en-tête de carte.
    ///
    /// C'est aussi ce qui rend l'état vide atteignable autrement qu'au premier démarrage :
    /// une liste où l'on ajoute sans pouvoir revenir en arrière ferait d'une faute de
    /// frappe une entrée définitive.
    pub fn forget(&self, command: &Command) -> Result<Vec<ToolDeclaration>, SettingsError> {
        let stored = {
            let mut tools = self.lock()?;
            let before = tools.len();
            tools.retain(|tool| &tool.command != command);
            if tools.len() == before {
                return Err(SettingsError::UnknownTool(command.clone()));
            }
            tools.clone()
        };
        // Oubliée du registre **et** du fichier : une entrée qui reviendrait au redémarrage
        // ferait d'un `✕` un geste sans effet, et c'est justement ce geste qui rend l'état
        // vide atteignable après une faute de frappe.
        self.persist(&stored);
        // La liste enrichie, et pas la liste stockée : oublier une entrée peut lever le
        // doublon d'une autre, et sa ligne `hooks` avec.
        Ok(self.enrich(stored))
    }

    /// Ce qu'une entrée ne peut pas savoir seule : ses homonymes de dossier, et l'état du
    /// fichier qu'elle vise.
    ///
    /// **Dérivé à chaque fois plutôt que retenu**, parce que les deux sources changent sans
    /// que l'entrée bouge : une autre entrée peut prendre son dossier, et l'utilisateur peut
    /// éditer son `settings.json` pendant que la fenêtre est ouverte. Un état de hooks mis
    /// en cache serait exactement le genre de vérité périmée sur laquelle on finirait par
    /// écrire.
    ///
    /// **Hors du verrou** : la lecture d'un fichier de configuration n'a pas à figer le
    /// registre pour les autres fils, dont ceux du second temps de la vérification.
    fn enrich(&self, mut tools: Vec<ToolDeclaration>) -> Vec<ToolDeclaration> {
        let aimed: Vec<(Command, Option<ConfigTarget>)> = tools
            .iter()
            .map(|tool| {
                (
                    tool.command.clone(),
                    self.verifier.target(&tool.adapter, tool.config.as_deref()),
                )
            })
            .collect();

        for (index, tool) in tools.iter_mut().enumerate() {
            let mine = aimed.get(index).and_then(|(_, dir)| dir.clone());
            let mut duplicates = Vec::new();
            // Qui tient le fichier : **la première entrée déclarée**. Il en faut une, et
            // celle-là ne dépend ni de l'ordre des clics ni du contenu du disque — donc
            // l'écran ne change pas d'avis d'un affichage à l'autre.
            let mut taken_by: Option<Command> = None;
            for (position, (name, dir)) in aimed.iter().enumerate() {
                if position == index || dir.is_none() || *dir != mine {
                    continue;
                }
                duplicates.push(name.clone());
                if position < index && taken_by.is_none() {
                    taken_by = Some(name.clone());
                }
            }

            let found: Option<BlockAt> = match (&mine, taken_by.is_none()) {
                (Some(folder), true) if tool.verification.allows_hooks => {
                    self.blocks.inspect(&tool.adapter, folder)
                }
                _ => None,
            };

            tool.hooks = report(&tool.verification, &tool.adapter, taken_by.as_ref(), found);
            tool.duplicates = duplicates;
        }
        tools
    }

    /// Garde ces déclarations pour la prochaine session, **si elles ont changé**.
    ///
    /// Un échec d'écriture ne remet pas le geste en cause — disque plein, `~/.ash` non
    /// inscriptible : l'entrée est déclarée, elle ne survivra simplement pas au redémarrage.
    /// Refuser une déclaration pour cette raison serait incompréhensible, et c'est la même
    /// conduite que `features::theme` tient pour le thème.
    ///
    /// La comparaison n'est pas une optimisation prématurée : c'est ce qui fait que
    /// `re-verify all` n'écrit rien. Une vérification ne change que ce qui n'est pas gardé,
    /// à une exception près — le dernier dossier valide, qu'un test réussi met à jour.
    fn persist(&self, tools: &[ToolDeclaration]) {
        let now = PersistedTools::of(tools);
        let Ok(mut saved) = self.saved.lock() else {
            return;
        };
        if *saved == now {
            return;
        }
        saved.clone_from(&now);
        drop(saved);
        let _ = self.store.save(&now);
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Vec<ToolDeclaration>>, SettingsError> {
        self.tools.lock().map_err(|_| SettingsError::Poisoned)
    }
}

/// Le premier temps d'une entrée du registre, résultat posé sur place.
fn verify_at(
    verifier: &Verifier,
    tools: &mut [ToolDeclaration],
    index: usize,
) -> Option<SecondPass> {
    let tool = tools.get_mut(index)?;
    let first = verifier.first_pass(&tool.command, &tool.adapter, tool.config.as_deref());
    let shown = first.shown().clone();
    let pending = match first {
        FirstPass::Pending { launch, .. } => Some(SecondPass {
            command: tool.command.clone(),
            adapter: tool.adapter.clone(),
            config: tool.config.clone(),
            launch,
            stored: true,
        }),
        FirstPass::Settled(_) => None,
    };
    let declared = verifier.target(&tool.adapter, tool.config.as_deref());
    *tool = tool.clone().verified_by(shown, declared);
    pending
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::features::agents::{hook_mark, HookEntry, Instrumentation};
    use crate::features::hooks::fakes::FakeConfigFiles;
    use crate::features::hooks::Presence;
    use crate::features::settings::fakes::{FakeBlocks, FakeCommands, FakeFolders, FakeToolStore};
    use crate::features::settings::hooks::HookState;
    use crate::features::settings::verification::{AdapterProfile, VerificationState};

    fn profiles() -> Vec<AdapterProfile> {
        vec![
            AdapterProfile {
                id: "claude-code".to_owned(),
                default_config: Some("/home/.claude".to_owned()),
                signature: vec!["settings.json".to_owned()],
                config_env: Some("CLAUDE_CONFIG_DIR".to_owned()),
                probe_args: vec!["--version".to_owned()],
            },
            AdapterProfile {
                id: "generic".to_owned(),
                default_config: None,
                signature: Vec::new(),
                config_env: None,
                probe_args: vec!["--version".to_owned()],
            },
        ]
    }

    /// Test Data Builder : un registre dont on ne décrit que le monde qui compte.
    struct RegistryBuilder {
        files: FakeFolders,
        commands: FakeCommands,
        blocks: FakeBlocks,
        store: Arc<FakeToolStore>,
    }

    impl RegistryBuilder {
        fn new() -> Self {
            Self {
                files: FakeFolders::new("/home"),
                commands: FakeCommands::new().answering(true),
                // `generic` n'instrumente rien, ici comme dans l'application : c'est ce qui
                // fait qu'une entrée sur cet adaptateur ne se voit jamais proposer `install`.
                blocks: FakeBlocks::new().without_hooks("generic"),
                store: Arc::new(FakeToolStore::empty()),
            }
        }

        /// Ce qu'un `~/.ash/tools.json` de la session précédente porte.
        fn stored(mut self, store: Arc<FakeToolStore>) -> Self {
            self.store = store;
            self
        }

        fn folder(mut self, path: &str, entries: &[&str]) -> Self {
            self.files = self.files.folder(path, entries);
            self
        }

        fn in_path(mut self, command: &str, program: &str) -> Self {
            self.commands = self.commands.in_path(command, program);
            self
        }

        fn carrying(mut self, config_dir: &str, presence: Presence) -> Self {
            self.blocks = self.blocks.at(config_dir, presence);
            self
        }

        fn build(self) -> ToolRegistry {
            self.assemble().0
        }

        /// Le registre **et** ce qu'il écrit : les tests qui comptent ici sont ceux qui
        /// affirment qu'aucun fichier n'a été touché.
        fn assemble(self) -> (ToolRegistry, Arc<FakeBlocks>) {
            let Self {
                files,
                commands,
                blocks,
                store,
            } = self;
            let blocks = Arc::new(blocks);
            let registry = Self {
                files,
                commands,
                blocks: FakeBlocks::new(),
                store,
            }
            .over(Arc::clone(&blocks) as Arc<dyn HookBlocks>);
            (registry, blocks)
        }

        /// Le même registre, branché sur la **vraie** écriture de `features::hooks`.
        ///
        /// C'est le seul montage qui puisse répondre à la question de la spec §10 — « le
        /// fichier est-il rendu à l'octet près ? » — parce que c'est le seul où il y a des
        /// octets. Le double ordinaire raconte ce qu'on lui a dit ; celui-ci passe par la
        /// fusion, le marqueur et le `.bak`, comme la composition root.
        fn on_real_files(self, files: Arc<FakeConfigFiles>) -> ToolRegistry {
            self.over(Arc::new(RealBlocks { files }))
        }

        fn over(self, blocks: Arc<dyn HookBlocks>) -> ToolRegistry {
            let store = Arc::clone(&self.store) as Arc<dyn ToolStore>;
            ToolRegistry::restore(
                Arc::new(Verifier::new(
                    Arc::new(self.files),
                    Arc::new(self.commands),
                    profiles(),
                )),
                blocks,
                store,
            )
        }
    }

    /// Le port branché sur `features::hooks`, exactement comme `AdapterHooks` le fait dans
    /// `lib.rs` — un identifiant d'adaptateur traduit en instrumentation, et rien d'autre.
    struct RealBlocks {
        files: Arc<FakeConfigFiles>,
    }

    impl RealBlocks {
        /// Ce qu'un adaptateur voudrait voir écrit. **Composée ici et non empruntée à
        /// `ClaudeCodeAdapter`** : `settings` ne connaît aucun adaptateur concret
        /// (ADR-0008), et cette règle ne s'assouplit pas parce qu'on est dans un test.
        fn describing(&self, adapter: &str, config_dir: &Path) -> Option<Instrumentation> {
            if adapter != "claude-code" {
                return None;
            }
            Some(Instrumentation {
                file: config_dir.join("settings.json"),
                entries: ["Stop", "Notification"]
                    .iter()
                    .map(|hook| HookEntry {
                        path: vec!["hooks".to_owned(), (*hook).to_owned()],
                        item: format!(
                            "{{\"hooks\": [{{\"command\": \"ash-event waiting {}\", \"type\": \"command\"}}]}}",
                            hook_mark(1)
                        ),
                    })
                    .collect(),
                version: 1,
            })
        }
    }

    impl HookBlocks for RealBlocks {
        fn inspect(&self, adapter: &str, config_dir: &ConfigTarget) -> Option<BlockAt> {
            let instrumentation = self.describing(adapter, config_dir.resolved())?;
            Some(BlockAt {
                file: instrumentation.file.clone(),
                presence: crate::features::hooks::inspect(&*self.files, &instrumentation),
            })
        }

        fn install(&self, adapter: &str, config_dir: &ConfigTarget) -> Result<(), String> {
            let instrumentation = self
                .describing(adapter, config_dir.resolved())
                .ok_or_else(|| "rien à installer".to_owned())?;
            crate::features::hooks::install(&*self.files, &instrumentation)
                .map(|_| ())
                .map_err(|why| why.to_string())
        }

        fn remove(&self, adapter: &str, config_dir: &ConfigTarget) -> Result<Removal, String> {
            let instrumentation = self
                .describing(adapter, config_dir.resolved())
                .ok_or_else(|| "rien à retirer".to_owned())?;
            crate::features::hooks::uninstall(&*self.files, &instrumentation)
                .map_err(|why| why.to_string())
        }

        fn foresee_removal(
            &self,
            adapter: &str,
            config_dir: &ConfigTarget,
        ) -> Option<crate::features::hooks::Withdrawal> {
            let instrumentation = self.describing(adapter, config_dir.resolved())?;
            crate::features::hooks::foresee(&*self.files, &instrumentation)
        }
    }

    /// Les deux comptes de la spec §9, chacun avec son `settings.json` bien à lui.
    fn two_instrumented_accounts(files: &Arc<FakeConfigFiles>) -> ToolRegistry {
        two_instrumented_accounts_stored(files, &Arc::new(FakeToolStore::empty()))
    }

    /// Les mêmes, sur un `~/.ash/tools.json` que le scénario garde sous la main.
    fn two_instrumented_accounts_stored(
        files: &Arc<FakeConfigFiles>,
        store: &Arc<FakeToolStore>,
    ) -> ToolRegistry {
        let registry = two_claude_accounts()
            .stored(Arc::clone(store))
            .on_real_files(Arc::clone(files));
        for (command, folder) in [
            ("claude", "/home/.claude"),
            ("claude-perso", "/home/.claude-perso"),
        ] {
            registry
                .declare(draft(command, "claude-code", Some(folder)))
                .expect("la saisie est valide");
            registry
                .install_hooks(&named(command))
                .expect("les deux entrées sont vérifiées");
        }
        registry
    }

    /// Un nom de commande valide, tel que `NewTool::declare` en produit.
    fn named(command: &str) -> Command {
        Command::parse(command).unwrap_or_else(|why| panic!("{command} est un nom valide : {why}"))
    }

    fn draft(command: &str, adapter: &str, config: Option<&str>) -> NewTool {
        NewTool {
            command: command.to_owned(),
            label: None,
            adapter: adapter.to_owned(),
            config: config.map(str::to_owned),
        }
    }

    #[test]
    fn given_a_fresh_declaration_when_it_lands_in_the_registry_then_it_already_carries_what_the_tests_said(
    ) {
        // Given — la maquette relance « à tout changement de chemin ou d'adaptateur » ; un
        // ajout est le premier de ces changements
        let registry = RegistryBuilder::new()
            .folder("/home/.claude", &["settings.json"])
            .build();

        // When
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");

        // Then — le test 3 échoue (rien dans le `PATH`), donc une réserve, pas un `unverified`
        let tool = changed.tools.first().expect("l'entrée est là");
        assert_eq!(tool.verification.state, VerificationState::Caveat);
        assert!(tool.verified);
    }

    #[test]
    fn given_an_entry_pointing_at_a_folder_that_is_not_a_config_when_it_is_declared_then_it_is_kept_and_shown_invalid(
    ) {
        // Given — la maquette montre une entrée invalide **dans la liste** (`3e`) : Ash
        // n'empêche pas de déclarer, il refuse d'écrire. Confondre les deux ferait
        // disparaître l'entrée qu'on essaie justement de corriger
        let registry = RegistryBuilder::new()
            .folder("/home/notes", &["a.md"])
            .build();

        // When
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/notes")))
            .expect("la saisie est valide");

        // Then
        let tool = changed.tools.first().expect("l'entrée est là");
        assert_eq!(tool.verification.state, VerificationState::Invalid);
        assert!(!tool.verified);
    }

    #[test]
    fn given_an_invalid_entry_when_the_suggested_fix_retargets_it_then_it_is_verified_again_on_the_spot(
    ) {
        // Given — appliquer la correction proposée n'a de sens que si l'écran répond
        let registry = RegistryBuilder::new()
            .folder("/home/notes", &["a.md"])
            .build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/notes")))
            .expect("la saisie est valide");

        // When — `generic` ne signe rien : le même dossier lui convient
        let changed = registry
            .retarget(&named("claude"), "generic", Some("/home/notes"))
            .expect("l'entrée existe");

        // Then
        let tool = changed.tools.first().expect("l'entrée est là");
        assert_eq!(tool.verification.state, VerificationState::Caveat);
        assert_eq!(tool.adapter, "generic");
    }

    #[test]
    fn given_a_path_changed_while_the_command_was_still_answering_when_the_late_result_comes_back_then_it_is_dropped(
    ) {
        // Given — le test 4 peut mettre cinq secondes ; l'utilisateur, lui, tape. Un
        // résultat périmé posé sur l'entrée décrirait un chemin qui n'est plus à l'écran
        let registry = RegistryBuilder::new()
            .folder("/home/.claude", &["settings.json"])
            .folder("/home/other", &["settings.json"])
            .in_path("claude", "/bin/claude")
            .build();
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let stale = changed
            .pending
            .first()
            .cloned()
            .expect("le test 4 reste à lancer");
        registry
            .retarget(&named("claude"), "claude-code", Some("/home/other"))
            .expect("l'entrée existe");

        // When
        let settled = registry
            .settle(&stale, Verification::unverified())
            .expect("le registre répond");

        // Then
        assert!(settled.is_none());
    }

    #[test]
    fn given_two_entries_to_re_verify_when_the_whole_list_is_relaunched_then_each_one_that_needs_the_fourth_test_asks_for_it(
    ) {
        // Given — `re-verify all` relance la liste ; ce sont les seconds temps qui partent
        // en parallèle, et eux seuls ont un processus à lancer
        let registry = RegistryBuilder::new()
            .folder("/home/.claude", &["settings.json"])
            .in_path("claude", "/bin/claude")
            .build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        registry
            .declare(draft("kimi", "generic", Some("/home/.claude")))
            .expect("la saisie est valide");

        // When
        let changed = registry.verify_all().expect("le registre répond");

        // Then — `kimi` est sur `generic`, qui ne sait imposer aucun dossier : rien à lancer
        assert_eq!(changed.tools.len(), 2);
        assert_eq!(
            changed
                .pending
                .iter()
                .map(|next| next.command.as_str())
                .collect::<Vec<_>>(),
            vec!["claude"]
        );
    }

    #[test]
    fn given_a_command_already_declared_when_the_same_one_is_declared_again_then_the_list_is_unchanged(
    ) {
        // Given — un refus ne doit pas laisser le registre à moitié modifié
        let registry = RegistryBuilder::new().build();
        registry
            .declare(draft("claude", "generic", Some("/home/x")))
            .expect("la saisie est valide");

        // When
        let refused = registry.declare(draft("claude", "generic", Some("/home/x")));

        // Then
        assert!(refused.is_err());
        assert_eq!(registry.tools().unwrap().len(), 1);
    }

    #[test]
    fn given_the_only_declared_command_when_it_is_forgotten_then_the_list_is_empty_again() {
        // Given — c'est ce qui rend l'état vide atteignable après un ajout
        let registry = RegistryBuilder::new().build();
        registry
            .declare(draft("claude", "generic", Some("/home/x")))
            .expect("la saisie est valide");

        // When
        let after = registry.forget(&named("claude")).unwrap();

        // Then
        assert_eq!(after, vec![]);
    }

    /// Deux entrées `claude-code` que tout sépare sauf l'adaptateur — le cas de la maquette.
    fn two_claude_accounts() -> RegistryBuilder {
        RegistryBuilder::new()
            .folder("/home/.claude", &["settings.json"])
            .folder("/home/.claude-perso", &["settings.json"])
            .in_path("claude", "/bin/claude")
            .in_path("claude-perso", "/bin/claude-perso")
    }

    #[test]
    fn given_two_entries_pointing_at_the_same_folder_when_the_list_is_read_then_both_rows_carry_the_flag(
    ) {
        // Given — « le doublon est signalé sur les deux lignes, pas seulement sur celle
        // qu'on vient de toucher » (spec §9.1). Ne marquer que la seconde ferait chercher
        // laquelle est l'autre
        let registry = two_claude_accounts().build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let tools = registry
            .declare(draft("claude-perso", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide")
            .tools;

        // When
        let flagged: Vec<(&str, Vec<&str>)> = tools
            .iter()
            .map(|tool| {
                (
                    tool.command.as_str(),
                    tool.duplicates.iter().map(Command::as_str).collect(),
                )
            })
            .collect();

        // Then
        assert_eq!(
            flagged,
            vec![
                ("claude", vec!["claude-perso"]),
                ("claude-perso", vec!["claude"]),
            ]
        );
    }

    #[test]
    fn given_two_entries_that_write_the_same_folder_differently_when_the_list_is_read_then_the_duplicate_still_shows(
    ) {
        // Given — `~/.claude` et `/home/.claude` sont le même dossier, donc le même fichier
        // de hooks. C'est le cas que la bannière de doublon existe pour attraper, et celui
        // qu'elle raterait si un lecteur comparait les chemins tels qu'ils sont écrits
        // pendant qu'un autre comparait les chemins résolus
        let registry = two_claude_accounts().build();
        registry
            .declare(draft("claude", "claude-code", Some("~/.claude")))
            .expect("la saisie est valide");

        // When
        let tools = registry
            .declare(draft("claude-perso", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide")
            .tools;

        // Then — signalé sur les deux lignes, et la seconde n'écrit pas par-dessus la première
        assert_eq!(tools[0].duplicates, vec![named("claude-perso")]);
        assert_eq!(tools[1].duplicates, vec![named("claude")]);
        assert_eq!(tools[1].hooks.state, HookState::Blocked);
    }

    #[test]
    fn given_two_entries_on_the_same_folder_when_the_second_looks_at_its_hooks_then_only_it_is_blocked(
    ) {
        // Given — le doublon n'invalide rien : la première garde ses hooks, la seconde ne
        // les écrit pas une deuxième fois dans le même fichier
        let registry = two_claude_accounts().build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let tools = registry
            .declare(draft("claude-perso", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide")
            .tools;

        // When
        let states: Vec<HookState> = tools.iter().map(|tool| tool.hooks.state).collect();

        // Then
        assert_eq!(states, vec![HookState::Missing, HookState::Blocked]);
        assert_eq!(
            tools[1].hooks.summary,
            "already written by claude in this file"
        );
    }

    #[test]
    fn given_an_entry_a_duplicate_blocks_when_its_hooks_are_asked_for_then_nothing_is_written() {
        // Given — la garde est en Rust, pas dans le bouton éteint : une fenêtre qui
        // appellerait quand même ne doit pas faire écrire Ash chez l'utilisateur
        let (registry, blocks) = two_claude_accounts().assemble();
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        registry
            .declare(draft("claude-perso", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");

        // When
        let refused = registry.install_hooks(&named("claude-perso"));

        // Then
        assert_eq!(
            refused.unwrap_err(),
            SettingsError::HooksRefused("already written by claude in this file".to_owned())
        );
        assert_eq!(blocks.written(), Vec::<String>::new());
    }

    #[test]
    fn given_an_entry_whose_folder_is_not_verified_when_its_hooks_are_asked_for_then_nothing_is_written(
    ) {
        // Given — « ash n'écrit dans aucun fichier tant qu'une entrée n'est pas vérifiée ».
        // C'est le garde-fou d'ADR-0007, et il vit du côté qui écrit
        let (registry, blocks) = RegistryBuilder::new()
            .folder("/home/notes", &["a.md"])
            .assemble();
        registry
            .declare(draft("claude", "claude-code", Some("/home/notes")))
            .expect("la saisie est valide");

        // When
        let refused = registry.install_hooks(&named("claude"));

        // Then
        assert!(matches!(refused, Err(SettingsError::HooksRefused(_))));
        assert_eq!(blocks.written(), Vec::<String>::new());
    }

    #[test]
    fn given_a_verified_entry_whose_block_is_missing_when_its_hooks_are_installed_then_they_land_in_its_own_folder(
    ) {
        // Given — `claude` et `claude-perso` sont deux dossiers, donc deux blocs (ADR-0007) :
        // ce qui est écrit doit l'être là où l'entrée pointe, pas au défaut de l'adaptateur
        let (registry, blocks) = two_claude_accounts().assemble();
        registry
            .declare(draft(
                "claude-perso",
                "claude-code",
                Some("/home/.claude-perso"),
            ))
            .expect("la saisie est valide");

        // When
        let after = registry
            .install_hooks(&named("claude-perso"))
            .expect("elle est vérifiée");

        // Then
        assert_eq!(
            blocks.written(),
            vec!["install claude-code /home/.claude-perso"]
        );
        assert_eq!(after[0].hooks.state, HookState::Missing);
    }

    #[test]
    fn given_a_block_someone_edited_by_hand_when_its_entry_is_shown_then_the_line_offers_the_diff_instead_of_a_write(
    ) {
        // Given — le conflit ne se déduit pas d'un souvenir : Ash relit le fichier, parce
        // que l'utilisateur a pu l'éditer entre deux ouvertures de la fenêtre
        let (registry, blocks) = two_claude_accounts()
            .carrying(
                "/home/.claude",
                Presence::HandEdited {
                    diff: "- moi\n+ ash".to_owned(),
                },
            )
            .assemble();

        // When
        let tools = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide")
            .tools;

        // Then — la ligne n'écrit pas d'elle-même : elle ouvre le diff
        assert_eq!(tools[0].hooks.state, HookState::Conflict);
        assert_eq!(tools[0].hooks.action, HookAction::SeeTheDiff);
        assert_eq!(tools[0].hooks.diff.as_deref(), Some("- moi\n+ ash"));
        assert_eq!(blocks.written(), Vec::<String>::new());

        // Et depuis le diff, l'utilisateur tranche : c'est son clic qui écrit, pas Ash
        registry
            .install_hooks(&named("claude"))
            .expect("le diff offre de remettre les entrées d'Ash");
        assert_eq!(blocks.written(), vec!["install claude-code /home/.claude"]);
    }

    #[test]
    fn given_an_entry_that_never_passed_the_four_tests_when_it_is_reset_then_it_says_there_is_nothing_to_restore(
    ) {
        // Given — « réinitialiser ramène à la dernière valeur valide » (spec §9.1) : quand
        // il n'y en a pas, renvoyer l'entrée au défaut de l'adaptateur fabriquerait
        // justement le doublon que cette règle existe pour éviter
        let registry = RegistryBuilder::new()
            .folder("/home/notes", &["a.md"])
            .build();
        registry
            .declare(draft("claude", "claude-code", Some("/home/notes")))
            .expect("la saisie est valide");

        // When
        let refused = registry.reset(&named("claude"));

        // Then
        assert_eq!(
            refused.err(),
            Some(SettingsError::NothingToRestore(named("claude")))
        );
    }

    #[test]
    fn given_an_entry_that_moved_away_from_a_folder_that_worked_when_it_is_reset_then_it_goes_back_to_that_folder(
    ) {
        // Given — et pas au défaut de l'adaptateur : `claude-perso` reviendrait alors sur
        // `~/.claude`, c'est-à-dire sur l'entrée d'à côté
        let registry = two_claude_accounts().build();
        let changed = registry
            .declare(draft(
                "claude-perso",
                "claude-code",
                Some("/home/.claude-perso"),
            ))
            .expect("la saisie est valide");
        // Le test 4 répond : c'est lui qui rend l'entrée `valid`, donc mémorisable.
        let pending = changed
            .pending
            .first()
            .cloned()
            .expect("le test 4 reste à lancer");
        let verification = registry.second_pass(&pending);
        registry
            .settle(&pending, verification)
            .expect("le registre répond");
        registry
            .retarget(&named("claude-perso"), "claude-code", Some("/home/notes"))
            .expect("l'entrée existe");

        // When
        let after = registry
            .reset(&named("claude-perso"))
            .expect("elle a été valide");

        // Then
        let tool = after.tools.first().expect("l'entrée est là");
        assert_eq!(tool.config.as_deref(), Some("/home/.claude-perso"));
        assert_eq!(
            tool.reset_from.as_ref().map(ConfigTarget::declared),
            Some("/home/notes")
        );
    }

    #[test]
    fn given_a_reset_that_landed_on_another_entrys_folder_when_it_is_undone_then_the_previous_folder_comes_back(
    ) {
        // Given — c'est le `undo the reset` de la bannière : le geste qui a créé la
        // collision est celui qu'on défait, et il reste à portée
        let registry = two_claude_accounts().build();
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let pending = changed
            .pending
            .first()
            .cloned()
            .expect("le test 4 reste à lancer");
        let verification = registry.second_pass(&pending);
        registry
            .settle(&pending, verification)
            .expect("le registre répond");
        registry
            .retarget(&named("claude"), "claude-code", Some("/home/.claude-perso"))
            .expect("l'entrée existe");
        registry.reset(&named("claude")).expect("elle a été valide");

        // When
        let after = registry
            .undo_reset(&named("claude"))
            .expect("elle vient d'être réinitialisée");

        // Then
        let tool = after.tools.first().expect("l'entrée est là");
        assert_eq!(tool.config.as_deref(), Some("/home/.claude-perso"));
        assert_eq!(tool.reset_from, None);
    }

    #[test]
    fn given_an_entry_the_user_retyped_after_a_reset_when_the_undo_is_asked_for_then_it_is_refused()
    {
        // Given — « annuler la réinitialisation » ne veut plus rien dire une fois le chemin
        // retapé : proposer le geste ferait revenir un dossier que personne ne demande plus
        let registry = two_claude_accounts().build();
        let changed = registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        let pending = changed
            .pending
            .first()
            .cloned()
            .expect("le test 4 reste à lancer");
        let verification = registry.second_pass(&pending);
        registry
            .settle(&pending, verification)
            .expect("le registre répond");
        registry
            .retarget(&named("claude"), "claude-code", Some("/home/.claude-perso"))
            .expect("l'entrée existe");
        registry.reset(&named("claude")).expect("elle a été valide");

        // When
        registry
            .retarget(&named("claude"), "claude-code", Some("/home/notes"))
            .expect("l'entrée existe");
        let refused = registry.undo_reset(&named("claude"));

        // Then
        assert_eq!(
            refused.err(),
            Some(SettingsError::NothingToUndo(named("claude")))
        );
    }

    /// Le même monde, une session plus tard : un registre neuf sur le même fichier.
    fn restarted(store: &Arc<FakeToolStore>) -> ToolRegistry {
        two_claude_accounts().stored(Arc::clone(store)).build()
    }

    #[test]
    fn given_a_tool_declared_in_one_session_when_ash_starts_again_then_its_card_is_there_without_a_new_typing(
    ) {
        // Given — le geste qui fait qu'un onglet devient un agent (ADR-0006) était perdu au
        // redémarrage : l'écran disait « no tools declared » pendant que les hooks, eux,
        // étaient toujours posés sur le disque de l'utilisateur
        let store = Arc::new(FakeToolStore::empty());
        let first_session = two_claude_accounts().stored(Arc::clone(&store)).build();
        first_session
            .declare(NewTool {
                command: "claude-perso".to_owned(),
                label: Some("Perso".to_owned()),
                adapter: "claude-code".to_owned(),
                config: Some("/home/.claude-perso".to_owned()),
            })
            .expect("la saisie est valide");

        // When
        let tools = restarted(&store).tools().expect("le registre répond");

        // Then
        let tool = tools.first().expect("l'entrée a survécu");
        assert_eq!(
            (
                tool.command.as_str(),
                tool.label.as_deref(),
                tool.adapter.as_str(),
                tool.config.as_deref()
            ),
            (
                "claude-perso",
                Some("Perso"),
                "claude-code",
                Some("/home/.claude-perso")
            )
        );
    }

    #[test]
    fn given_an_entry_read_back_from_the_file_when_it_is_shown_then_it_is_unverified_and_writes_nothing(
    ) {
        // Given — une vérification est un fait **daté sur la machine** : le dossier a pu être
        // renommé entre les deux lancements. La relire du fichier serait un souvenir présenté
        // comme une lecture, et c'est ce souvenir-là qui autoriserait une écriture chez
        // l'utilisateur (ADR-0007)
        let store = Arc::new(FakeToolStore::carrying(vec![FakeToolStore::entry(
            "claude",
            "claude-code",
            Some("/home/.claude"),
        )]));
        let (registry, blocks) = two_claude_accounts()
            .stored(Arc::clone(&store))
            .carrying("/home/.claude", Presence::Current { version: 1 })
            .assemble();

        // When
        let tools = registry.tools().expect("le registre répond");

        // Then — le bouton reste visible et éteint, avec sa raison (spec §9.1)
        let tool = tools.first().expect("l'entrée a survécu");
        assert_eq!(tool.verification.state, VerificationState::Unverified);
        assert!(!tool.verified);
        assert!(!tool.hooks.enabled);
        assert!(!tool.hooks.summary.is_empty());
        assert_eq!(blocks.written(), Vec::<String>::new());
    }

    #[test]
    fn given_an_entry_read_back_from_the_file_when_its_sequence_runs_again_then_its_hooks_line_says_what_the_file_carries(
    ) {
        // Given — le scénario de la tâche : l'entrée dit `installed` parce qu'elle a **relu
        // le fichier**, et non parce qu'elle s'en souvenait. Rien de l'état des hooks n'est
        // gardé dans `~/.ash/tools.json`
        let store = Arc::new(FakeToolStore::carrying(vec![FakeToolStore::entry(
            "claude",
            "claude-code",
            Some("/home/.claude"),
        )]));
        let registry = two_claude_accounts()
            .stored(Arc::clone(&store))
            .carrying("/home/.claude", Presence::Current { version: 1 })
            .build();

        // When — ce que la fenêtre lance en s'ouvrant sur des entrées que rien n'a jugées
        let changed = registry.verify_all().expect("le registre répond");

        // Then
        let tool = changed.tools.first().expect("l'entrée a survécu");
        assert_eq!(tool.hooks.state, HookState::Installed);
    }

    #[test]
    fn given_an_entry_whose_folder_was_removed_between_two_launches_when_it_is_verified_then_it_fails_the_first_test(
    ) {
        // Given — le second scénario de la tâche : ce que le fichier garde est une
        // déclaration, pas une promesse. Le dossier a disparu depuis
        let store = Arc::new(FakeToolStore::carrying(vec![FakeToolStore::entry(
            "claude",
            "claude-code",
            Some("/home/.gone"),
        )]));
        let registry = two_claude_accounts().stored(Arc::clone(&store)).build();

        // When
        let changed = registry.verify_all().expect("le registre répond");

        // Then — et il dit **quel** dossier manque
        let tool = changed.tools.first().expect("l'entrée a survécu");
        assert_eq!(tool.verification.state, VerificationState::Invalid);
        assert_eq!(tool.verification.stopped_at, Some(1));
        assert!(
            tool.verification.summary.contains("/home/.gone"),
            "{}",
            tool.verification.summary
        );
    }

    #[test]
    fn given_an_entry_that_proved_a_folder_before_a_restart_when_it_is_reset_then_it_goes_back_to_that_folder(
    ) {
        // Given — « réinitialiser ramène à la dernière valeur valide, pas au défaut de
        // l'adaptateur » (spec §9.1). Sans la mémoire dans le fichier, le geste ramènerait
        // après un redémarrage `claude-perso` sur `~/.claude`, c'est-à-dire sur l'entrée d'à
        // côté — le doublon deviendrait la conséquence mécanique du geste
        let store = Arc::new(FakeToolStore::empty());
        let first_session = two_claude_accounts().stored(Arc::clone(&store)).build();
        let changed = first_session
            .declare(draft(
                "claude-perso",
                "claude-code",
                Some("/home/.claude-perso"),
            ))
            .expect("la saisie est valide");
        let pending = changed
            .pending
            .first()
            .cloned()
            .expect("le test 4 reste à lancer");
        let verification = first_session.second_pass(&pending);
        first_session
            .settle(&pending, verification)
            .expect("le registre répond");

        // When — une session plus tard, l'entrée est déplacée puis réinitialisée
        let next_session = restarted(&store);
        next_session
            .retarget(&named("claude-perso"), "claude-code", Some("/home/notes"))
            .expect("l'entrée existe");
        let after = next_session
            .reset(&named("claude-perso"))
            .expect("elle a été valide avant le redémarrage");

        // Then
        let tool = after.tools.first().expect("l'entrée est là");
        assert_eq!(tool.config.as_deref(), Some("/home/.claude-perso"));
    }

    #[test]
    fn given_a_declared_tool_when_it_is_forgotten_then_it_does_not_come_back_at_the_next_launch() {
        // Given — un `✕` qui laisserait l'entrée dans le fichier ferait d'une suppression un
        // geste sans effet, et l'utilisateur découvrirait au redémarrage ce qu'il croyait
        // avoir retiré
        let store = Arc::new(FakeToolStore::empty());
        let session = two_claude_accounts().stored(Arc::clone(&store)).build();
        session
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        session
            .declare(draft(
                "claude-perso",
                "claude-code",
                Some("/home/.claude-perso"),
            ))
            .expect("la saisie est valide");

        // When
        session.forget(&named("claude")).expect("elle est déclarée");

        // Then — le fichier ne la nomme plus, et l'autre est intacte
        assert_eq!(store.commands(), vec!["claude-perso".to_owned()]);
        assert_eq!(
            restarted(&store)
                .declarations()
                .expect("le registre répond")
                .len(),
            1
        );
    }

    #[test]
    fn given_a_tools_file_one_entry_of_which_says_nothing_usable_when_ash_starts_then_the_others_are_loaded(
    ) {
        // Given — le fichier s'édite à la main (spec §9), et on revient d'une version à la
        // précédente en changeant de branche. Une entrée qui ne désigne rien — un nom qui
        // n'est pas un nom de processus, une entrée sans adaptateur, un doublon de commande —
        // ne doit pas coûter celles d'à côté
        let store = Arc::new(FakeToolStore::carrying(vec![
            FakeToolStore::entry("/usr/local/bin/claude", "claude-code", None),
            FakeToolStore::entry("claude", "claude-code", Some("/home/.claude")),
            FakeToolStore::entry("kimi", "", None),
            FakeToolStore::entry("claude", "generic", Some("/home/notes")),
            FakeToolStore::entry("claude-perso", "claude-code", Some("/home/.claude-perso")),
        ]));

        // When
        let registry = two_claude_accounts().stored(Arc::clone(&store)).build();

        // Then
        assert_eq!(
            registry
                .declarations()
                .expect("le registre répond")
                .iter()
                .map(|tool| tool.command.as_str().to_owned())
                .collect::<Vec<_>>(),
            vec!["claude".to_owned(), "claude-perso".to_owned()]
        );
    }

    #[test]
    fn given_an_entry_naming_an_adapter_this_build_does_not_embed_when_ash_starts_then_it_is_kept_and_shown_invalid(
    ) {
        // Given — la même conduite que pour une saisie : Ash n'empêche pas de déclarer, il
        // refuse d'écrire. Faire disparaître l'entrée perdrait sans un mot le chemin que
        // l'utilisateur avait tapé, et `first_pass` compose justement la correction qui a une
        // chance pour ce cas
        let store = Arc::new(FakeToolStore::carrying(vec![FakeToolStore::entry(
            "kimi",
            "kimi-code",
            Some("/home/.claude"),
        )]));
        let registry = two_claude_accounts().stored(Arc::clone(&store)).build();

        // When
        let changed = registry.verify_all().expect("le registre répond");

        // Then
        let tool = changed.tools.first().expect("l'entrée a survécu");
        assert_eq!(tool.adapter, "kimi-code");
        assert_eq!(tool.verification.state, VerificationState::Invalid);
        assert!(!tool.verified);
    }

    #[test]
    fn given_entries_read_back_from_the_file_when_the_whole_list_is_re_verified_then_nothing_is_written_to_the_file(
    ) {
        // Given — ce qui est gardé est la déclaration, et une vérification n'en change
        // aucune. `re-verify all` sur des entrées relues réécrirait sinon le même fichier une
        // fois par entrée, à chaque ouverture de la fenêtre
        let store = Arc::new(FakeToolStore::carrying(vec![
            FakeToolStore::entry("claude", "claude-code", Some("/home/.claude")),
            FakeToolStore::entry("kimi", "generic", Some("/home/notes")),
        ]));
        let registry = two_claude_accounts().stored(Arc::clone(&store)).build();

        // When
        registry.verify_all().expect("le registre répond");

        // Then
        assert_eq!(store.writes(), 0);
    }

    #[test]
    fn given_a_command_that_was_never_declared_when_it_is_forgotten_then_it_is_refused() {
        // Given — se taire ferait croire à une suppression qui n'a pas eu lieu
        let registry = RegistryBuilder::new().build();

        // When
        let forgotten = registry.forget(&named("codex"));

        // Then
        assert_eq!(
            forgotten.unwrap_err(),
            SettingsError::UnknownTool(named("codex"))
        );
    }

    /// Le `settings.json` que l'utilisateur avait avant Ash — son ordre, son indentation.
    const THEIRS: &str = "{\n  \"model\": \"opus\",\n  \"hooks\": { \"PreToolUse\": [ { \"matcher\": \"Bash\",\n    \"hooks\": [ { \"type\": \"command\", \"command\": \"rtk hook claude\" } ] } ] }\n}\n";

    #[test]
    fn given_two_instrumented_config_folders_when_ash_is_removed_from_every_file_then_each_one_is_back_to_the_byte(
    ) {
        // Given — la promesse la plus lourde du produit (spec §10), portée cette fois par le
        // geste global : « retirer ash de tous les fichiers » traverse plusieurs fichiers de
        // l'utilisateur, et le seul moyen de le vérifier est de les comparer à eux-mêmes
        let files = Arc::new(
            FakeConfigFiles::new()
                .carrying("/home/.claude/settings.json", THEIRS)
                .carrying("/home/.claude-perso/settings.json", THEIRS),
        );
        let registry = two_instrumented_accounts(&files);
        assert!(files
            .content_of(Path::new("/home/.claude/settings.json"))
            .unwrap_or_default()
            .contains("ash-event"));

        // When
        let (report, _) = registry.remove_everything().expect("le registre répond");

        // Then
        assert_eq!(report.summary, "removed 4 entries from 2 files");
        for folder in ["/home/.claude", "/home/.claude-perso"] {
            assert_eq!(
                files
                    .content_of(&Path::new(folder).join("settings.json"))
                    .as_deref(),
                Some(THEIRS),
                "{folder} n'a pas été rendu tel quel"
            );
        }
    }

    #[test]
    fn given_two_instrumented_accounts_when_ash_is_removed_from_every_file_then_the_declarations_stay_declared(
    ) {
        // Given — « retirer Ash de tous les fichiers » (spec §10) reprend ce qu'Ash a écrit
        // **chez l'utilisateur**. Ce n'est pas le geste qui oublie un outil — celui-là est le
        // `✕` d'une carte — et emporter la liste au passage ferait disparaître de l'écran les
        // cartes sur lesquelles on vient de cliquer, sans que personne ne l'ait demandé
        let files = Arc::new(
            FakeConfigFiles::new()
                .carrying("/home/.claude/settings.json", THEIRS)
                .carrying("/home/.claude-perso/settings.json", THEIRS),
        );
        let store = Arc::new(FakeToolStore::empty());
        let registry = two_instrumented_accounts_stored(&files, &store);

        // When
        registry.remove_everything().expect("le registre répond");

        // Then — le fichier d'Ash dit toujours ce qui reste déclaré, ni plus ni moins
        assert_eq!(
            store.commands(),
            vec!["claude".to_owned(), "claude-perso".to_owned()]
        );
        assert_eq!(
            restarted(&store)
                .declarations()
                .expect("le registre répond")
                .len(),
            2
        );
    }

    #[test]
    fn given_a_removal_that_took_place_when_the_disk_is_looked_at_then_the_backups_are_still_there()
    {
        // Given — « les .bak sont conservés » : ils sont la copie d'avant Ash, et les effacer
        // au moment où l'on désinstalle retirerait le filet juste avant de sauter
        let files = Arc::new(
            FakeConfigFiles::new()
                .carrying("/home/.claude/settings.json", THEIRS)
                .carrying("/home/.claude-perso/settings.json", THEIRS),
        );
        let registry = two_instrumented_accounts(&files);

        // When
        registry.remove_everything().expect("le registre répond");

        // Then
        for folder in ["/home/.claude", "/home/.claude-perso"] {
            assert_eq!(
                files
                    .content_of(&Path::new(folder).join("settings.json.bak"))
                    .as_deref(),
                Some(THEIRS),
                "le .bak de {folder} a disparu"
            );
        }
    }

    #[test]
    fn given_the_whole_removal_when_every_file_gesture_is_replayed_then_none_of_them_left_the_config_folders(
    ) {
        // Given — spec §10 : « rien d'autre — pas de .zshrc, pas de PATH, pas de shim, pas de
        // hook git ». C'est une propriété de non-écriture, donc elle ne se lit pas dans le
        // code : elle se lit dans la liste de ce qu'Ash a touché, du premier geste au dernier
        let files =
            Arc::new(FakeConfigFiles::new().carrying("/home/.claude/settings.json", THEIRS));
        let registry = two_claude_accounts().on_real_files(Arc::clone(&files));
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        registry
            .install_hooks(&named("claude"))
            .expect("l'entrée est vérifiée");

        // When
        registry.removal_plan().expect("le registre répond");
        registry.remove_everything().expect("le registre répond");

        // Then
        let touched: Vec<String> = files
            .journal()
            .into_iter()
            .filter(|step| !step.starts_with("read "))
            .collect();
        assert!(
            touched.iter().all(|step| {
                step.split_whitespace().skip(1).all(|word| {
                    word == "->"
                        || word == "/home/.claude/settings.json"
                        || word == "/home/.claude/settings.json.bak"
                })
            }),
            "ash n'écrit que dans le settings.json visé et sa sauvegarde : {touched:?}"
        );
    }

    #[test]
    fn given_an_entry_whose_folder_no_longer_verifies_when_ash_is_removed_from_every_file_then_its_entries_go_too(
    ) {
        // Given — la séquence garde l'**écriture d'entrées nouvelles**, pas leur reprise. Une
        // entrée dont le dossier a été renommé depuis l'installation ne passe plus les tests ;
        // lui refuser le retrait abandonnerait sur le disque ce que ce geste existe pour
        // reprendre
        let files =
            Arc::new(FakeConfigFiles::new().carrying("/home/.claude/settings.json", THEIRS));
        let registry = two_claude_accounts().on_real_files(Arc::clone(&files));
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        registry
            .install_hooks(&named("claude"))
            .expect("l'entrée est vérifiée");
        // Le dossier n'est plus celui que la séquence avait reconnu : elle refuse désormais
        let unverified = two_claude_accounts()
            .folder("/home/notes", &["a.md"])
            .on_real_files(Arc::clone(&files));
        unverified
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        assert!(unverified
            .install_hooks(&named("claude"))
            .is_err_and(|why| matches!(why, SettingsError::HooksRefused(_))));

        // When
        let (report, _) = unverified.remove_everything().expect("le registre répond");

        // Then
        assert_eq!(report.summary, "removed 2 entries from 1 file");
        assert_eq!(
            files
                .content_of(Path::new("/home/.claude/settings.json"))
                .as_deref(),
            Some(THEIRS)
        );
    }

    #[test]
    fn given_two_entries_sharing_one_config_folder_when_the_removal_is_announced_then_the_file_is_named_once(
    ) {
        // Given — deux comptes déclarés sur le même dossier : c'est un seul fichier, et
        // l'annoncer deux fois promettrait deux fois les mêmes entrées, puis rapporterait un
        // second passage qui n'a rien trouvé
        let files =
            Arc::new(FakeConfigFiles::new().carrying("/home/.claude/settings.json", THEIRS));
        let registry = two_claude_accounts().on_real_files(Arc::clone(&files));
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        registry
            .install_hooks(&named("claude"))
            .expect("l'entrée est vérifiée");
        registry
            .declare(draft("claude-perso", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");

        // When
        let announced = registry.removal_plan().expect("le registre répond");

        // Then
        assert_eq!(announced.files.len(), 1);
        assert_eq!(
            announced.files[0].commands,
            vec!["claude".to_owned(), "claude-perso".to_owned()]
        );
        assert_eq!(
            announced.summary,
            "2 entries in /home/.claude/settings.json"
        );
    }

    #[test]
    fn given_a_settings_file_that_stopped_being_json_after_the_announcement_when_the_removal_runs_then_it_is_left_alone(
    ) {
        // Given — le fichier de l'utilisateur peut changer entre l'annonce et le geste. Le
        // retrait **relit** au moment d'écrire plutôt que de croire l'annonce : on ne devine
        // pas où sont nos entrées dans un fichier qu'on ne sait plus lire, donc rien n'est
        // écrit, rien n'est prétendu, et le fichier reste tel quel plutôt que d'être perdu
        let files =
            Arc::new(FakeConfigFiles::new().carrying("/home/.claude/settings.json", THEIRS));
        let registry = two_claude_accounts().on_real_files(Arc::clone(&files));
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        registry
            .install_hooks(&named("claude"))
            .expect("l'entrée est vérifiée");
        let announced = registry.removal_plan().expect("le registre répond");
        files.replace(Path::new("/home/.claude/settings.json"), "à moitié écrit {");

        // When
        let (report, _) = registry.remove_everything().expect("le registre répond");

        // Then
        assert_eq!(announced.files.len(), 1);
        assert_eq!(report.summary, "nothing was removed");
        assert_eq!(report.files, Vec::new());
        assert_eq!(
            files
                .content_of(Path::new("/home/.claude/settings.json"))
                .as_deref(),
            Some("à moitié écrit {")
        );
    }

    #[test]
    fn given_a_config_file_ash_created_for_itself_when_the_removal_is_announced_then_it_says_the_file_goes_too(
    ) {
        // Given — l'annonce est ce sur quoi l'utilisateur tranche (spec §10), et « ce fichier
        // disparaît » n'est pas la même promesse que « ces lignes disparaissent »
        let files = Arc::new(FakeConfigFiles::new());
        let registry = two_claude_accounts().on_real_files(Arc::clone(&files));
        registry
            .declare(draft("claude", "claude-code", Some("/home/.claude")))
            .expect("la saisie est valide");
        registry
            .install_hooks(&named("claude"))
            .expect("l'entrée est vérifiée");
        files.forget_the_journal();

        // When
        let announced = registry.removal_plan().expect("le registre répond");

        // Then — et l'annonce n'écrit rien : elle lit
        assert!(announced.files[0].deletes_the_file);
        assert_eq!(files.journal(), ["read /home/.claude/settings.json"]);
    }
}
