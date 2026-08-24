use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::features::agents::{
    AgentState, Presence, ProgramIdentity, RecognizedAgent, RecognizedProvider, SessionUsage,
    Subagent,
};
use crate::shared::time::UnixMillis;

use super::agent_states::AgentStates;
use super::compose::{ComposeDesk, ComposeOutcome, Foreground};
use super::error::PtyError;
use super::flow::Credits;
use super::locate::{TabLocation, WorktreeLocator};
use super::recognition::AgentRecognition;
use super::session::{PtySession, PtySpawner, PtySpec};
use super::terminal_env::terminal_env;
use crate::features::probe::{Pid, Probe, ProcessControl, TabObservation, TabWatch};

/// Identifiant d'onglet — un ulid, posé dans `ASH_TAB_ID` au lancement du shell.
///
/// C'est par lui, et par rien d'autre, que les events d'agent seront corrélés
/// ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)) : ni par le `cwd`, ni par
/// un horodatage.
pub type TabId = String;

/// Morceaux qui peuvent être en vol sans acquittement de la webview.
///
/// Huit lectures de 64 Kio font 512 Kio, très loin des 50 Mo au-delà desquels xterm.js
/// jette la sortie (voir [`super::flow`] et `docs/spike-xterm.md`).
const WINDOW: usize = 8;

/// Les PTY vivants, **dans l'ordre**, et rien d'autre.
///
/// Le registre détient l'état : le frontend l'affiche, il ne le possède pas
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). L'ordre en fait
/// partie — c'est lui que `Cmd+1..9` désigne (spec §4.4), et une table de hachage n'en
/// a pas. Un `Vec` en donne un stable et une suppression qui préserve le reste ; la
/// recherche linéaire est sans objet à cette échelle, un utilisateur n'ouvre pas mille
/// onglets.
pub struct PtyRegistry {
    spawner: Box<dyn PtySpawner>,
    /// La sonde d'ADR-0005, injectée : c'est elle qui donne son `cwd` vivant à un onglet.
    probe: Arc<dyn Probe>,
    /// La résolution `cwd` → worktree + dépôt, injectée elle aussi (voir [`super::locate`]).
    locator: Arc<dyn WorktreeLocator>,
    /// Qui reconnaît l'outil qui tient l'avant-plan d'un onglet (voir [`super::recognition`]).
    ///
    /// Injectée pour la même raison que l'état : la table des outils connus appartient à
    /// `agents` et les entrées déclarées à `settings`
    /// ([ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)). Le
    /// registre demande, il ne déduit pas — il ne connaît pas un seul nom d'outil.
    recognition: Arc<dyn AgentRecognition>,
    /// Qui décide de l'état d'agent d'un onglet (voir [`super::agent_states`]).
    ///
    /// Le registre pose la question et transporte la réponse ; il ne la calcule pas. C'est
    /// ce qui l'empêche de connaître les hooks, les adaptateurs et l'horloge des trente
    /// secondes — trois choses qu'un tenancier de PTY n'a pas à savoir.
    agents: Arc<dyn AgentStates>,
    /// De quoi arrêter et reprendre le groupe en avant-plan d'un onglet (ADR-0015).
    ///
    /// Injecté comme la sonde, et pour la même raison : la règle qui compte — on n'arrête
    /// pas un shell à son invite, et un onglet arrêté se reprend — se vérifie sans envoyer
    /// un seul signal réel. Un `cargo test` qui posterait un vrai `SIGSTOP` arrêterait la
    /// machine de qui le lance.
    control: Arc<dyn ProcessControl>,
    /// L'âge des localisations retenues (voir [`Self::invalidate_locations`]).
    revision: AtomicU64,
    tabs: Mutex<Vec<Tab>>,
}

struct Tab {
    id: TabId,
    session: Box<dyn PtySession>,
    credits: Arc<Credits>,
    /// La grille que le PTY porte **en ce moment**, en colonnes et en lignes.
    ///
    /// Retenue pour ne pas la reposer : redimensionner un PTY poste un `SIGWINCH` au groupe
    /// en avant-plan, et une TUI plein écran s'y redessine entièrement
    /// ([ADR-0003](../../../../docs/adr/0003-zone-terminal-unique.md), reformulation du
    /// 2026-08-10). C'est le dernier filtre avant le signal, et c'est ici qu'il doit être :
    /// le panneau bas (spec §4.3) donne au terminal une **seconde** raison de changer de
    /// boîte, en plus de la fenêtre et de la colonne de gauche, et rien ne garantit que
    /// trois sources indépendantes n'annoncent pas la même grille l'une après l'autre.
    grid: (u16, u16),
    /// Répertoire de départ du shell, retenu à l'ouverture.
    ///
    /// Ce n'est plus ce que l'onglet montre : c'est le repli quand la sonde ne sait pas
    /// répondre. Un onglet doit toujours avoir un répertoire à afficher, même sur un
    /// système qui refuse de parler.
    start_dir: PathBuf,
    /// Le nom du shell de l'onglet, retenu à l'ouverture.
    ///
    /// C'est ce que la sidebar affiche tant que la sonde n'a rien dit : un onglet sans
    /// nom serait pire qu'un onglet nommé d'après son shell.
    shell_name: String,
    /// La sonde de cet onglet, quand le système le rend observable.
    watch: SharedWatch,
    /// La dernière localisation résolue, et le répertoire pour lequel elle l'a été.
    place: SharedPlace,
    /// Le groupe de processus arrêté par [`PtyRegistry::pause`], quand il y en a un.
    ///
    /// Le pgid est **retenu** plutôt que redemandé au moment de reprendre : un groupe arrêté
    /// ne rend plus la main, donc `tcgetpgrp` continuerait de le désigner — mais si le
    /// terminal a été fermé entre-temps, le descripteur ne dit plus rien, et l'agent
    /// resterait arrêté sans personne pour le réveiller. Ce champ est le fil auquel on tire.
    paused: SharedPause,
    /// Ce que le registre retient de la **saisie** de cet onglet — voir [`super::compose`].
    compose: SharedDesk,
    /// Le dernier `TabInfo` que la boucle a poussé vers la webview.
    ///
    /// C'est lui qui garde la frontière Tauri muette au repos : un onglet n'est annoncé que
    /// si quelque chose de ce qu'il montre a changé. Le comparer entier — et non le seul
    /// `cwd` de la sonde — est ce qui laisse un état venu d'un **hook** traverser sans que
    /// l'onglet ait bougé d'un caractère.
    announced: SharedAnnouncement,
}

/// La sonde d'un onglet, tenue à part du verrou du registre.
///
/// Deux raisons, et les deux comptent :
///
/// - **elle se prend hors du registre.** Une passe de sonde fait deux appels système par
///   onglet, trois fois par seconde ([ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md)).
///   Sous le verrou global, chaque frappe clavier attendrait derrière elle — `write`,
///   `resize` et `ack` prennent ce verrou-là. Le registre n'est donc verrouillé que le
///   temps de recopier les poignées, et la sonde tourne dehors.
/// - **`None` veut dire « onglet fermé ».** Le descripteur du master part avec la
///   session ; un `fd` recyclé se relit sans erreur, et une sonde qui survit à son onglet
///   ne se tromperait pas bruyamment, elle se tromperait en silence.
type SharedWatch = Arc<Mutex<Option<TabWatch>>>;

/// La localisation d'un onglet, retenue avec ce qui l'a produite.
///
/// C'est le dédoublonnage de la résolution : elle lit des fichiers de contrôle git, et le
/// `cwd` d'un onglet ne change pas trois fois par seconde. Tant que rien n'a bougé, la
/// réponse retenue est la bonne — donc on ne redemande rien au disque.
///
/// Tenue à part du verrou du registre, pour la même raison que la sonde : une frappe
/// clavier n'a pas à attendre derrière une lecture de fichier.
type SharedPlace = Arc<Mutex<Option<Located>>>;

/// Ce que la webview sait déjà d'un onglet, tenu à part du verrou du registre pour la même
/// raison que la sonde et la localisation.
type SharedAnnouncement = Arc<Mutex<Option<TabInfo>>>;

/// Le groupe arrêté d'un onglet, tenu à part du verrou du registre pour la même raison.
type SharedPause = Arc<Mutex<Option<Pid>>>;

/// Le pupitre de composition d'un onglet, tenu à part du verrou du registre pour la même
/// raison que la sonde : il est touché à **chaque frappe**, et une frappe clavier n'a pas
/// à attendre derrière la description de tous les onglets.
type SharedDesk = Arc<Mutex<ComposeDesk>>;

struct Located {
    cwd: PathBuf,
    /// L'âge de cette réponse.
    ///
    /// Le `cwd` ne suffit pas à la mémoriser : la réponse dépend aussi de l'**état du
    /// dépôt**, qui décide entre la forme à plat et la forme groupée d'ADR-0012. Un
    /// `git worktree add` change donc la bonne réponse sans que le `cwd` bouge d'un
    /// caractère — c'est le second terme de la clé.
    revision: u64,
    location: Option<TabLocation>,
}

/// Ce qu'un onglet montre de lui-même au frontend.
///
/// C'est aussi ce que la boucle de sonde annonce quand un onglet a **bougé** : un même
/// type pour la liste et pour l'event, parce que rien ne justifie que le frontend
/// apprenne un onglet de deux façons différentes. Un onglet posé à son invite ne traverse
/// pas la frontière — voir [`PtyRegistry::changes`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    pub tab_id: TabId,
    /// Le répertoire **courant** de l'onglet, sondé à la demande.
    ///
    /// C'est lui que « nouvel onglet dans le worktree courant » (spec §4.4) reprend : le
    /// répertoire où l'onglet en est, pas celui d'où il est parti.
    pub cwd: String,
    /// Le programme qui tient l'avant-plan — le nom que la sidebar et la barre affichent.
    ///
    /// C'est le nom de **l'outil** quand il en est un : un Claude Code posé par son
    /// installateur officiel tourne sous un exécutable nommé `2.1.234`, et l'onglet doit
    /// dire `claude` — aujourd'hui comme après la mise à jour suivante
    /// ([ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)).
    pub process: String,
    /// L'outil reconnu dans l'avant-plan, et ce que sa configuration porte. `None` pour un
    /// shell à son invite comme pour un `vim`.
    ///
    /// Il ne bouge **pas** d'une passe de sonde à l'autre tant que le même programme tient
    /// l'avant-plan : la fiche est comparée entière pour décider d'émettre (voir
    /// [`Self::state_since`]), et un champ qui changerait toutes les 300 ms réveillerait la
    /// sidebar entière en permanence.
    ///
    /// Il ne porte **aucun état d'agent** : reconnaître un outil n'est pas savoir ce qu'il
    /// fait ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)). Ce qu'il dit en plus
    /// du nom, c'est si la configuration de l'outil porte le marqueur d'Ash — donc pourquoi
    /// cet onglet ne montrera jamais `waiting` tant qu'elle ne le porte pas.
    pub agent: Option<RecognizedAgent>,
    pub state: AgentState,
    /// Quand l'onglet est **entré** dans cet état, en millisecondes depuis l'époque Unix.
    ///
    /// Une date, jamais une durée, et c'est ce qui garde cette fiche **stable** : le
    /// registre compare le `TabInfo` entier pour décider s'il faut annoncer quoi que ce
    /// soit (voir [`Self::changes`]). Une durée vivante ferait donc changer la fiche de
    /// chaque onglet actif à chaque seconde, et l'event ponctuel deviendrait un flux — on
    /// paierait un rendu complet de la sidebar par seconde pour animer un compteur.
    ///
    /// Le `working · 15m22s` de la maquette se calcule donc à l'affichage, à partir de
    /// cette date et de l'horloge du frontend.
    ///
    /// **`number` et non `bigint`** : `ts-rs` prête un `bigint` à tout `u64`, par prudence
    /// sur les valeurs qui dépassent 2⁵³. Ce ne serait pas seulement pénible — ce serait
    /// faux : `serde_json` écrit un nombre JSON, que la webview lit en `number`, et un
    /// `bigint` déclaré ici mentirait sur ce qui arrive vraiment. La borne de 2⁵³
    /// millisecondes tombe en l'an 287396.
    #[cfg_attr(test, ts(type = "number"))]
    pub state_since: UnixMillis,
    /// Les sous-agents qui tournent **dans** cet onglet, en ce moment (spec §6.5).
    ///
    /// Vide dans le cas courant, et vide pour toujours chez un outil qui n'expose pas ses
    /// sous-tâches. Ils voyagent avec l'onglet et non par un event à eux : ce sont des lignes
    /// filles de la sienne, elles n'ont pas de terminal
    /// ([ADR-0003](../../../../docs/adr/0003-zone-terminal-unique.md)) et rien ne les
    /// sélectionne.
    ///
    /// Chacun porte sa **date d'entrée**, pour la même raison que [`Self::state_since`] : une
    /// durée vivante ferait changer cette fiche à chaque seconde, et l'event ponctuel
    /// deviendrait un flux.
    pub subagents: Vec<Subagent>,
    /// La place que la conversation de cet onglet occupe dans sa fenêtre de contexte.
    ///
    /// `None` dans l'écrasante majorité des cas, et **`None` pour toujours** chez un outil
    /// qui ne tient pas de transcript : l'adaptateur `generic` déclare `UsageSupport::None`,
    /// donc aucun onglet servi par lui ne portera jamais ce champ. C'est ce qui permet à
    /// l'écran de ne rien afficher plutôt que d'afficher un vide — pas de jauge à zéro, pas
    /// de `ctx —` (voir `features::agents::SessionUsage`).
    ///
    /// **Elle ne fait pas repartir l'event, et c'est la même mécanique que
    /// [`Self::state_since`]** : la fiche est comparée entière pour décider d'émettre (voir
    /// [`Self::changes`]), et cette valeur ne change qu'à l'arrivée d'un hook portant un
    /// transcript — jamais à une passe de sonde. Le superviseur ne relit rien toutes les
    /// 300 ms : il rend la dernière mesure lue, à l'octet près identique tant que l'agent
    /// n'a pas reparlé, donc `ash://tab-changed` ne part pas plus souvent qu'avant.
    ///
    /// Ce n'est **pas** un état d'agent, et rien ici n'a de chemin vers [`AgentState`] : un
    /// contexte plein ne rend pas un onglet `error`
    /// ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
    pub usage: Option<SessionUsage>,
    /// Où cet onglet se range dans la hiérarchie d'ADR-0012. `None` quand le répertoire
    /// n'a pas pu être situé.
    pub location: Option<TabLocation>,
    /// Le groupe en avant-plan de cet onglet est **arrêté** — `SIGSTOP`
    /// ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
    ///
    /// Ce n'est **pas** un sixième état d'agent, et il ne passe pas par `agents` : un état
    /// d'agent vient d'un hook (ADR-0007), et un processus arrêté n'en émet aucun — c'est
    /// justement ce qui le rend invisible autrement. Le registre, lui, sait qu'il a posté le
    /// signal, et c'est le seul à le savoir.
    ///
    /// Il voyage dans la fiche parce qu'un agent laissé arrêté sans rien qui le dise est un
    /// piège : il paraîtrait `working` pour toujours, et personne ne saurait qu'il attend un
    /// `SIGCONT`.
    pub paused: bool,
}

/// Ce qu'`open` rend au-delà de l'identifiant : de quoi lancer le lecteur.
pub struct Opened {
    pub tab_id: TabId,
    pub reader: Box<dyn Read + Send>,
    pub credits: Arc<Credits>,
}

impl PtyRegistry {
    pub fn new(
        spawner: Box<dyn PtySpawner>,
        probe: Arc<dyn Probe>,
        locator: Arc<dyn WorktreeLocator>,
        recognition: Arc<dyn AgentRecognition>,
        agents: Arc<dyn AgentStates>,
        control: Arc<dyn ProcessControl>,
    ) -> Self {
        Self {
            spawner,
            probe,
            locator,
            recognition,
            agents,
            control,
            revision: AtomicU64::new(0),
            tabs: Mutex::new(Vec::new()),
        }
    }

    /// Les localisations retenues ont pu vieillir : la prochaine passe les redemandera.
    ///
    /// Le registre ne sait pas *pourquoi* — il ne connaît de la résolution que son port. Ce
    /// qu'il sait, c'est qu'une réponse n'est pas fonction du seul `cwd` : un dépôt qui
    /// gagne son premier worktree lié passe de la forme à plat à la forme groupée
    /// ([ADR-0012](../../../../docs/adr/0012-worktree-unite-de-travail.md)) alors que tous
    /// ses onglets sont restés où ils étaient.
    ///
    /// Ce signal est ce qui **remplace** le sondage : il arrive d'une écriture observée dans
    /// `.git`, jamais d'un minuteur. Entre deux signaux, un onglet immobile ne coûte
    /// toujours aucune lecture de disque.
    pub fn invalidate_locations(&self) {
        self.revision.fetch_add(1, Ordering::Release);
    }

    pub fn open(&self, mut spec: PtySpec, tab_id: TabId) -> Result<Opened, PtyError> {
        spec.env.push(("ASH_TAB_ID".to_owned(), tab_id.clone()));
        // Ce que le shell doit savoir du terminal qu'on lui offre. C'est ici, à côté
        // d'`ASH_TAB_ID`, parce que c'est la même nature de chose : une identité posée par
        // Ash, pas une préférence de l'utilisateur — et parce que le registre est la
        // dernière étape avant le spawner à rester derrière le trait, donc testable. Ce
        // qu'Ash déclare et ce qu'il laisse à l'utilisateur se décide là-bas, pas ici.
        spec.env.extend(terminal_env());

        let (session, reader) = self.spawner.spawn(&spec)?;
        let credits = Arc::new(Credits::new(WINDOW));

        let watch =
            Arc::new(Mutex::new(session.terminal().map(|terminal| {
                TabWatch::new(terminal.master_fd, terminal.shell_pid)
            })));

        // Un onglet neuf va à la fin : c'est l'ordre que la barre d'onglets montre, et
        // celui que `Cmd+1..9` numérote.
        self.lock()?.push(Tab {
            id: tab_id.clone(),
            session,
            credits: Arc::clone(&credits),
            grid: (spec.cols, spec.rows),
            start_dir: spec.cwd.clone(),
            shell_name: shell_name(&spec),
            watch,
            place: Arc::new(Mutex::new(None)),
            announced: Arc::new(Mutex::new(None)),
            paused: Arc::new(Mutex::new(None)),
            compose: Arc::new(Mutex::new(ComposeDesk::default())),
        });

        Ok(Opened {
            tab_id,
            reader,
            credits,
        })
    }

    /// Les onglets vivants, dans leur ordre d'affichage, avec leur répertoire courant.
    ///
    /// Le `cwd` est sondé à l'appel, et non recopié d'une passe précédente : c'est ce qui
    /// fait que « nouvel onglet dans le worktree courant » part du répertoire où l'onglet
    /// en est, et non de celui de la dernière ouverture.
    pub fn tabs(&self) -> Result<Vec<TabInfo>, PtyError> {
        Ok(self
            .snapshot()?
            .into_iter()
            .map(|tab| {
                let seen = self.observe(&tab.watch);
                self.describe(tab, seen)
            })
            .collect())
    }

    /// Une passe de la boucle d'ADR-0005 : ce qui a **changé** depuis la précédente.
    ///
    /// Rien pour un onglet dont rien n'a changé — ni son répertoire, ni son avant-plan, ni
    /// sa place, ni son état. C'est ce que la boucle émet vers le frontend, et ce qui fait suivre
    /// le titre d'un onglet à travers les `cd` — y compris pendant qu'un programme tourne,
    /// là où OSC 7 se tairait. C'est aussi ce qui fait migrer un onglet d'un dépôt à
    /// l'autre dans la sidebar : la localisation voyage avec l'onglet.
    ///
    /// Un onglet peut changer **sans bouger** : c'est le cas quand son dépôt gagne ou perd
    /// un worktree lié (voir [`Self::invalidate_locations`]), et c'est aussi le cas d'un
    /// agent dont un hook vient de déclarer l'état alors que rien du tout n'a remué dans le
    /// terminal ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)). Ces onglets-là
    /// sont annoncés eux aussi.
    ///
    /// Ce qui décide, c'est donc la **description entière** : un onglet passe la frontière
    /// si et seulement si ce qu'il montre diffère de ce que la webview a déjà. Comparer la
    /// seule sonde laisserait un `waiting` sans producteur visible attendre indéfiniment le
    /// prochain `cd`.
    pub fn changes(&self) -> Result<Vec<TabInfo>, PtyError> {
        let mut changed = Vec::new();
        for tab in self.snapshot()? {
            let announced = Arc::clone(&tab.announced);
            let desk = Arc::clone(&tab.compose);
            let tab_id = tab.id.clone();
            let seen = self.observe(&tab.watch);
            let now = self.describe(tab, seen);

            // Le tour de l'agent vient peut-être de finir : c'est ici, et nulle part
            // ailleurs, qu'un texte retenu rejoint le prompt (voir [`super::compose`]).
            // La passe de sonde est le seul endroit qui repose la question de l'état trois
            // fois par seconde ; un minuteur à part ferait vivre la même cadence deux fois.
            self.flush_composed(&tab_id, &desk, now.state);

            let Ok(mut announced) = announced.lock() else {
                continue;
            };
            if announced.as_ref() != Some(&now) {
                *announced = Some(now.clone());
                changed.push(now);
            }
        }
        Ok(changed)
    }

    /// Pose dans le prompt le texte que le pupitre retenait, quand le tour est fini.
    ///
    /// La règle — rien ne sort tant que le tour dure — est celle de
    /// [`ComposeDesk::release_after_turn`]. Ici, il n'y a que la traduction de l'état
    /// d'agent en « un tour est en cours », et l'écriture.
    fn flush_composed(&self, tab_id: &str, desk: &SharedDesk, state: AgentState) {
        let released = match desk.lock() {
            Ok(mut desk) => desk.release_after_turn(state == AgentState::Working),
            Err(_) => None,
        };
        if let Some(text) = released {
            // Échouer à écrire signifie que l'onglet vient de disparaître : il n'y a plus
            // de prompt où poser quoi que ce soit.
            let _ = self.write(tab_id, text.as_bytes());
        }
    }

    /// Les racines de worktree où vit au moins un onglet, sans doublon et **sans rien
    /// redemander au disque**.
    ///
    /// C'est la dernière localisation retenue par [`Self::locate`] qui répond : la
    /// question est posée à chaque passe de la boucle de sonde, et y répondre par une
    /// résolution serait exactement le sondage que la spec §5.3 écarte. Un onglet que
    /// personne n'a encore su situer n'y figure pas.
    pub fn worktree_roots(&self) -> Result<Vec<String>, PtyError> {
        let mut roots: Vec<String> = self
            .snapshot()?
            .iter()
            .filter_map(|tab| {
                let place = tab.place.lock().ok()?;
                let located = place.as_ref()?;
                Some(located.location.as_ref()?.worktree_root.clone())
            })
            .collect();
        roots.sort();
        roots.dedup();
        Ok(roots)
    }

    /// Les outils reconnus dans l'avant-plan des onglets, **tels que la dernière passe de
    /// sonde les a annoncés** (ADR-0006).
    ///
    /// Elle ne sonde rien, comme [`Self::worktree_roots`] et pour la même raison de fond :
    /// ce qu'elle rend est déjà là. La sonde pose la question trois fois par seconde et
    /// range la réponse dans la fiche qu'elle a annoncée ; la redemander ici ferait deux
    /// appels système par onglet **sur le fil de l'interface**, pour une réponse que le
    /// registre vient d'écrire. Un onglet ouvert il y a moins d'une passe n'y figure pas
    /// encore, et c'est sans conséquence : la passe suivante l'y met.
    ///
    /// Ce qui en sort est le couple nom + adaptateur, sans l'instrumentation que la fiche
    /// porte : celui qui demande relit le fichier lui-même, et en tire cinq états là où la
    /// fiche n'en porte que trois (voir `settings::RunningTools`).
    ///
    /// Sans doublon, dans l'ordre des onglets : trois onglets sur `claude` sont **un** outil
    /// reconnu, et l'ordre est celui de la colonne — donc stable d'un appel à l'autre.
    pub fn recognized_tools(&self) -> Result<Vec<RecognizedProvider>, PtyError> {
        let mut found: Vec<RecognizedProvider> = Vec::new();
        for tab in self.snapshot()? {
            let Ok(announced) = tab.announced.lock() else {
                continue;
            };
            let Some(agent) = announced.as_ref().and_then(|info| info.agent.as_ref()) else {
                continue;
            };
            if found.iter().any(|seen| seen.command == agent.command) {
                continue;
            }
            found.push(RecognizedProvider {
                command: agent.command.clone(),
                adapter: agent.adapter.clone(),
            });
        }
        Ok(found)
    }

    /// Cet onglet existe-t-il encore ?
    ///
    /// La question que pose le socket d'events d'ADR-0007 avant de livrer quoi que ce soit.
    /// Elle ne sonde rien, contrairement à [`Self::tabs`] : un hook arrive sur le fil d'une
    /// écoute, pas sur celui de l'interface, et un événement ne justifie pas deux appels
    /// système par onglet. Un registre empoisonné répond « non » — se taire vaut mieux que
    /// livrer un événement à un onglet dont on ne sait plus rien.
    pub fn knows(&self, tab_id: &str) -> bool {
        self.lock()
            .is_ok_and(|tabs| tabs.iter().any(|tab| tab.id == tab_id))
    }

    /// Vrai si quelque chose d'autre que le shell tient l'avant-plan de l'onglet.
    ///
    /// C'est la question que `Cmd+W` pose avant de détruire quoi que ce soit (spec §4.4).
    pub fn has_foreground_process(&self, tab_id: &str) -> Result<bool, PtyError> {
        self.with_tab(tab_id, |tab| tab.session.has_foreground_process())
    }

    /// Arrête le groupe en avant-plan de cet onglet — la « pause » d'ADR-0015.
    ///
    /// **`SIGSTOP` sur le groupe en avant-plan, et rien d'autre.** Pas une touche écrite
    /// dans le PTY, pas un `Esc` supposé interrompre, pas une lecture de ce que l'outil
    /// affiche : Ash n'interprète pas l'interface d'un agent
    /// ([ADR-0010](../../../../docs/adr/0010-la-sidebar-informe-le-terminal-agit.md)), et
    /// composer un texte serait le geste de l'utilisateur, pas le sien
    /// ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
    ///
    /// **Un shell à son invite n'est pas mis en pause**, et ce refus est la règle qui compte
    /// ici : le groupe en avant-plan serait alors celui du shell lui-même, l'onglet
    /// deviendrait muet au clavier, et rien dans la fenêtre ne ressemblerait à une panne
    /// autant que ça. Il n'y a rien à arrêter dans un onglet où rien ne tourne.
    ///
    /// Idempotent : mettre en pause un onglet déjà arrêté ne poste pas un second signal.
    pub fn pause(&self, tab_id: &str) -> Result<(), PtyError> {
        let (terminal, running, held) = self.with_tab(tab_id, |tab| {
            Ok((
                tab.session.terminal(),
                tab.session.has_foreground_process()?,
                Arc::clone(&tab.paused),
            ))
        })?;

        let mut held = held
            .lock()
            .map_err(|_| PtyError::Io("verrou de pause empoisonné".to_owned()))?;
        if held.is_some() {
            return Ok(());
        }
        if !running {
            return Err(PtyError::NothingToPause(tab_id.to_owned()));
        }
        let terminal = terminal.ok_or_else(|| PtyError::NothingToPause(tab_id.to_owned()))?;

        let pgid = self
            .probe
            .foreground_pgid(terminal.master_fd)
            .map_err(|why| PtyError::Io(why.to_string()))?;
        self.control
            .pause(pgid)
            .map_err(|why| PtyError::Io(why.to_string()))?;

        *held = Some(pgid);
        Ok(())
    }

    /// Reprend le groupe arrêté — `SIGCONT`.
    ///
    /// C'est la moitié sans laquelle la pause serait un piège : un agent arrêté n'émet plus
    /// aucun hook, donc plus aucun état, et rien d'autre qu'Ash ne sait qu'il attend un
    /// signal. Le pgid vient de ce que [`Self::pause`] a retenu et non d'une nouvelle sonde :
    /// un onglet dont le terminal s'est refermé entre-temps doit **quand même** pouvoir
    /// rendre la main à son groupe.
    ///
    /// Idempotent : reprendre un onglet qui tourne ne fait rien.
    pub fn resume(&self, tab_id: &str) -> Result<(), PtyError> {
        let held = self.with_tab(tab_id, |tab| Ok(Arc::clone(&tab.paused)))?;
        let mut held = held
            .lock()
            .map_err(|_| PtyError::Io("verrou de pause empoisonné".to_owned()))?;
        let Some(pgid) = *held else {
            return Ok(());
        };

        self.control
            .resume(pgid)
            .map_err(|why| PtyError::Io(why.to_string()))?;
        // Oublié **après** le succès : un `SIGCONT` refusé laisse le groupe arrêté, et
        // effacer la mémoire ferait perdre le seul fil qui permette de réessayer.
        *held = None;
        Ok(())
    }

    /// Envoie des octets au shell — une frappe de l'utilisateur, ou un texte composé.
    ///
    /// Le **pupitre** en prend note au passage : c'est la seule source d'Ash sur ce que
    /// contient la ligne de saisie d'un onglet, et elle ne doit rien à la sortie du PTY
    /// ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), et voir
    /// [`super::compose`]).
    pub fn write(&self, tab_id: &str, bytes: &[u8]) -> Result<(), PtyError> {
        self.with_tab(tab_id, |tab| {
            if let Ok(mut desk) = tab.compose.lock() {
                desk.wrote(bytes);
            }
            tab.session.write(bytes)
        })
    }

    /// Rédige un texte dans un onglet — **sans jamais l'envoyer**
    /// ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
    ///
    /// La règle elle-même — les quatre issues et leur ordre — appartient au **pupitre**
    /// ([`ComposeDesk::arbitrate`]), et se lit là-bas d'un seul tenant. Le registre ne fait
    /// ici que trois choses qu'il est seul à savoir faire : dire ce que l'onglet montre,
    /// tenir le verrou, et poser les octets quand le pupitre le lui dit.
    ///
    /// Le `\n` n'est filtré ni ici ni là-bas mais **à la source** : le texte vient de
    /// `features::git`, dont le compositeur ne rend qu'une seule ligne — dans un PTY, un
    /// saut de ligne *est* la touche `⏎`.
    ///
    /// L'état d'agent consulté est le **dernier annoncé**, jamais un état redemandé : la
    /// question d'`AgentStates` fait avancer le temps des états qui expirent, et un clic
    /// n'a pas à faire vieillir une machine que la boucle de sonde arbitre déjà.
    pub fn compose(&self, tab_id: &str, text: &str) -> Result<ComposeOutcome, PtyError> {
        let (desk, announced) = self.with_tab(tab_id, |tab| {
            Ok((Arc::clone(&tab.compose), Arc::clone(&tab.announced)))
        })?;

        let announced = announced.lock().ok().and_then(|known| known.clone());
        let foreground = Foreground {
            // `agent` est ce que le port de reconnaissance a rendu : `pty` ne connaît
            // toujours aucun nom d'outil.
            agent_is_running: announced.as_ref().is_some_and(|tab| tab.agent.is_some()),
            turn_in_progress: announced.is_some_and(|tab| tab.state == AgentState::Working),
        };

        let outcome = {
            let Ok(mut desk) = desk.lock() else {
                return Err(PtyError::Io("pupitre de composition empoisonné".to_owned()));
            };
            desk.arbitrate(foreground, text)
        };

        if outcome == ComposeOutcome::Written {
            self.write(tab_id, text.as_bytes())?;
        }
        Ok(outcome)
    }

    /// Pose une grille sur le PTY d'un onglet — et **seulement si c'en est une autre**.
    ///
    /// Voir [`Tab::grid`] : ce qui est en jeu n'est pas le coût de l'appel, c'est le
    /// `SIGWINCH` qu'il poste.
    pub fn resize(&self, tab_id: &str, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.with_tab(tab_id, |tab| {
            if tab.grid == (cols, rows) {
                return Ok(());
            }
            tab.session.resize(cols, rows)?;
            tab.grid = (cols, rows);
            Ok(())
        })
    }

    /// La webview a fini d'écrire un morceau : le lecteur peut en émettre un de plus.
    pub fn ack(&self, tab_id: &str) -> Result<(), PtyError> {
        self.with_tab(tab_id, |tab| {
            tab.credits.release();
            Ok(())
        })
    }

    /// Ferme un onglet : le processus est terminé et le lecteur réveillé.
    ///
    /// Idempotent. Fermer un onglet dont le shell vient de sortir de lui-même est le cas
    /// nominal, pas une erreur à remonter à l'utilisateur.
    pub fn close(&self, tab_id: &str) -> Result<(), PtyError> {
        // `remove` sur un `Vec` décale la suite : c'est exactement ce qu'on veut, l'ordre
        // des onglets restants ne doit pas bouger quand on en ferme un au milieu.
        let Some(mut tab) = self.take(tab_id)? else {
            return Ok(());
        };
        // Fermer les crédits d'abord : un lecteur bloqué en attente doit être réveillé
        // pour constater l'arrêt, sinon son thread survit au shell.
        tab.credits.close();
        tab.session.kill()
    }

    /// Retire l'onglet dont le shell est sorti tout seul.
    pub fn forget(&self, tab_id: &str) {
        if let Ok(Some(tab)) = self.take(tab_id) {
            tab.credits.close();
        }
    }

    /// Les poignées des onglets, recopiées sous le verrou et rendues **dehors**.
    ///
    /// C'est tout ce que le registre garde de verrouillé pendant une passe de sonde :
    /// trois `clone` par onglet, et le verrou est rendu avant le premier appel système.
    fn snapshot(&self) -> Result<Vec<TabHandle>, PtyError> {
        Ok(self
            .lock()?
            .iter()
            .map(|tab| TabHandle {
                id: tab.id.clone(),
                start_dir: tab.start_dir.clone(),
                shell_name: tab.shell_name.clone(),
                watch: Arc::clone(&tab.watch),
                place: Arc::clone(&tab.place),
                announced: Arc::clone(&tab.announced),
                paused: Arc::clone(&tab.paused),
                compose: Arc::clone(&tab.compose),
            })
            .collect())
    }

    /// Ce qu'on dit d'un onglet au frontend, à partir de ce que la sonde a vu.
    ///
    /// Le seul endroit où un `TabInfo` est fabriqué : la liste et l'event doivent décrire
    /// un onglet de la même façon, sans quoi une migration annoncée par la boucle
    /// contredirait la prochaine relecture.
    fn describe(&self, tab: TabHandle, seen: Option<TabObservation>) -> TabInfo {
        let paused = tab
            .paused
            .lock()
            .map(|held| held.is_some())
            .unwrap_or(false);
        let cwd = seen
            .as_ref()
            .map_or_else(|| tab.start_dir.clone(), |seen| seen.cwd.clone());

        // Un onglet que la sonde ne sait pas décrire garde le nom de son shell : rien ne
        // permet d'affirmer qu'un programme y tourne, ni le contraire.
        let (mut process, presence, program) = seen.map_or_else(
            || (tab.shell_name.clone(), Presence::Unknown, None),
            |seen| {
                let foreground = seen.foreground;
                if foreground.is_shell {
                    return (foreground.name, Presence::Prompt, None);
                }
                let program = ProgramIdentity {
                    executable: foreground.executable,
                    name: foreground.name.clone(),
                    argv0: foreground.argv0,
                };
                (foreground.name, Presence::Program, Some(program))
            },
        );

        // Le registre **demande** aussi l'identité de l'outil : il ne connaît pas un seul
        // nom de commande, et la table qui les porte vit dans `agents` (ADR-0006). Un shell
        // à son invite n'est jamais un agent, donc on ne demande rien pour lui.
        let agent = program
            .as_ref()
            .and_then(|program| self.recognition.recognize(program));
        if let Some(recognized) = &agent {
            // `claude`, et non `2.1.234` : c'est l'outil que la ligne nomme, pas le fichier
            // que son installateur a posé.
            process.clone_from(&recognized.command);
        }

        // Le registre **demande** l'état, il ne le déduit pas : ce que la sonde voit est une
        // présence, et une présence n'est pas un état d'agent (ADR-0007). La date d'entrée
        // vient avec, pour la même raison : le registre la transporte, il ne la fabrique pas.
        let agents = self.agents.state(&tab.id, presence);

        TabInfo {
            tab_id: tab.id,
            location: self.locate(&tab.place, &cwd),
            cwd: cwd.display().to_string(),
            process,
            agent,
            state: agents.status.state,
            state_since: agents.status.since,
            subagents: agents.subagents,
            usage: agents.usage,
            paused,
        }
    }

    /// La localisation d'un répertoire, **résolue seulement quand la réponse a pu changer**.
    ///
    /// La résolution lit des fichiers sur le disque ; la boucle de sonde passe trois fois
    /// par seconde et la liste est relue à chaque ouverture d'onglet. Sans ce
    /// dédoublonnage, le `.git` de chaque worktree serait ouvert des milliers de fois par
    /// heure pour rendre invariablement la même réponse.
    ///
    /// La clé est le couple `(cwd, révision)`, et pas le `cwd` seul : la réponse dépend
    /// aussi de l'état du dépôt, qu'un `git worktree add` change sous les pieds d'un onglet
    /// immobile. Voir [`Self::invalidate_locations`].
    ///
    /// Le verrou pris ici est celui de l'onglet, jamais celui du registre : une frappe
    /// clavier n'attend pas derrière une lecture de fichier.
    fn locate(&self, place: &SharedPlace, cwd: &Path) -> Option<TabLocation> {
        // La révision est lue **avant** la résolution, et c'est l'ordre qui compte : un
        // signal qui arrive pendant la lecture du disque fait retenir une révision déjà
        // périmée, donc redemander à la passe suivante. La lire après ferait retenir la
        // nouvelle avec une réponse d'avant — le signal serait avalé en silence, et
        // l'onglet resterait dans le mauvais groupe jusqu'à la prochaine écriture dans
        // `.git`, c'est-à-dire le défaut que tout ceci corrige.
        let revision = self.revision.load(Ordering::Acquire);
        let Ok(mut place) = place.lock() else {
            return None;
        };

        if let Some(known) = place.as_ref() {
            if known.cwd == cwd && known.revision == revision {
                return known.location.clone();
            }
        }

        let location = self.locator.locate(cwd);
        *place = Some(Located {
            cwd: cwd.to_owned(),
            revision,
            location: location.clone(),
        });
        location
    }

    /// Le répertoire courant d'un onglet, sondé hors du verrou du registre.
    fn observe(&self, watch: &SharedWatch) -> Option<TabObservation> {
        let mut watch = watch.lock().ok()?;
        watch.as_mut()?.observe(self.probe.as_ref()).ok()
    }

    fn take(&self, tab_id: &str) -> Result<Option<Tab>, PtyError> {
        let removed = {
            let mut tabs = self.lock()?;
            tabs.iter()
                .position(|tab| tab.id == tab_id)
                .map(|at| tabs.remove(at))
        };

        // La sonde s'éteint **avant** que la session — donc le descripteur du master — ne
        // parte. Prendre le verrou ici attend qu'une passe en vol se termine : après ce
        // point, aucune sonde ne peut plus lire un `fd` que le système est libre de
        // recycler. Le verrou du registre, lui, est déjà rendu : les deux ne sont jamais
        // tenus ensemble.
        if let Some(tab) = removed.as_ref() {
            if let Ok(mut watch) = tab.watch.lock() {
                *watch = None;
            }
            // L'état d'agent de l'onglet part avec lui : rien n'est restauré, et un
            // identifiant réattribué ne doit pas hériter d'un agent fantôme (ADR-0009).
            self.agents.forget(&tab.id);
        }

        Ok(removed)
    }

    fn with_tab<T>(
        &self,
        tab_id: &str,
        action: impl FnOnce(&mut Tab) -> Result<T, PtyError>,
    ) -> Result<T, PtyError> {
        let mut tabs = self.lock()?;
        let tab = tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id)
            .ok_or_else(|| PtyError::UnknownTab(tab_id.to_owned()))?;
        action(tab)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Vec<Tab>>, PtyError> {
        self.tabs
            .lock()
            .map_err(|_| PtyError::Io("registre de PTY empoisonné".to_owned()))
    }
}

/// De quoi décrire un onglet sans tenir le verrou du registre.
///
/// `start_dir` voyage avec la sonde parce que se taire n'est pas une erreur à remonter :
/// un onglet dont le shell vient de mourir est encore affiché le temps que le frontend
/// l'apprenne, et le faire disparaître de la liste pour cette raison serait pire que de
/// montrer un répertoire un peu vieux.
struct TabHandle {
    id: TabId,
    start_dir: PathBuf,
    shell_name: String,
    watch: SharedWatch,
    place: SharedPlace,
    announced: SharedAnnouncement,
    paused: SharedPause,
    compose: SharedDesk,
}

/// Le nom du shell d'un onglet — `zsh`, `bash`. Le chemin entier pour seul repli.
fn shell_name(spec: &PtySpec) -> String {
    spec.shell
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| spec.shell.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::Instrumented;
    use crate::features::probe::{Pid, ProbeError, ProcessInfo};
    use crate::features::pty::fakes::{
        composing_registry, located_registry, observed_registry, pausable_registry,
        recognizing_registry, registry, spec, supervised_registry, Composing, CountingLocator,
        FakeAgentStates, FakeSpawner, SpecBuilder, LAUNCHED,
    };
    use std::os::fd::RawFd;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;

    fn ids(registry: &PtyRegistry) -> Vec<TabId> {
        registry
            .tabs()
            .unwrap()
            .into_iter()
            .map(|tab| tab.tab_id)
            .collect()
    }

    /// Un onglet réduit à ce que le scénario regarde : qui il est, et où il est.
    fn described(tab: TabInfo) -> (TabId, String) {
        (tab.tab_id, tab.cwd)
    }

    /// Un onglet où un agent tient l'avant-plan — le seul cas où la pause a un sens.
    fn tab_running_an_agent() -> (
        PtyRegistry,
        TabId,
        Arc<crate::features::pty::fakes::FakeProcessControl>,
    ) {
        let (registry, spawner, probe, control) = pausable_registry();
        registry.open(spec(), "01J0AGENT".to_owned()).unwrap();
        probe.hand_over_to("claude");
        spawner.foreground.store(true, Ordering::SeqCst);
        (registry, "01J0AGENT".to_owned(), control)
    }

    #[test]
    fn given_an_agent_is_writing_when_the_user_pauses_it_then_its_foreground_group_is_stopped() {
        // Given
        let (registry, tab, control) = tab_running_an_agent();

        // When
        registry.pause(&tab).unwrap();

        // Then — un `SIGSTOP` sur le groupe en avant-plan, et rien d'autre (ADR-0015)
        assert_eq!(control.posted(), vec![("SIGSTOP", LAUNCHED)]);
    }

    #[test]
    fn given_a_paused_agent_when_it_is_resumed_then_the_same_group_gets_a_sigcont() {
        // Given
        let (registry, tab, control) = tab_running_an_agent();
        registry.pause(&tab).unwrap();

        // When
        registry.resume(&tab).unwrap();

        // Then — le groupe retenu par la pause, pas une nouvelle sonde : un onglet dont le
        // terminal s'est refermé doit quand même pouvoir rendre la main à son agent
        assert_eq!(
            control.posted(),
            vec![("SIGSTOP", LAUNCHED), ("SIGCONT", LAUNCHED)]
        );
    }

    #[test]
    fn given_a_paused_agent_when_the_tab_is_described_then_its_card_says_it_is_paused() {
        // Given
        let (registry, tab, _) = tab_running_an_agent();

        // When
        registry.pause(&tab).unwrap();

        // Then — sans ça, l'agent paraîtrait `working` pour toujours et personne ne saurait
        // qu'il attend un `SIGCONT`
        let described = registry.tabs().unwrap();
        assert!(described.iter().all(|shown| shown.paused));

        // And — reprendre l'efface
        registry.resume(&tab).unwrap();
        assert!(registry.tabs().unwrap().iter().all(|shown| !shown.paused));
    }

    #[test]
    fn given_a_shell_sitting_at_its_prompt_when_asked_to_pause_it_then_nothing_is_signalled() {
        // Given — rien ne tourne : le groupe en avant-plan est celui du shell lui-même
        let (registry, _, _, control) = pausable_registry();
        registry.open(spec(), "01J0SHELL".to_owned()).unwrap();

        // When
        let refused = registry.pause("01J0SHELL");

        // Then — l'arrêter rendrait l'onglet muet au clavier, sans que rien ne l'explique
        assert!(matches!(refused, Err(PtyError::NothingToPause(_))));
        assert!(control.posted().is_empty());
    }

    #[test]
    fn given_an_already_paused_agent_when_pausing_it_again_then_no_second_signal_is_posted() {
        // Given
        let (registry, tab, control) = tab_running_an_agent();
        registry.pause(&tab).unwrap();

        // When
        registry.pause(&tab).unwrap();

        // Then
        assert_eq!(control.posted(), vec![("SIGSTOP", LAUNCHED)]);
    }

    #[test]
    fn given_a_resume_the_system_refuses_when_it_is_retried_then_the_tab_is_still_known_as_paused()
    {
        // Given
        let (registry, tab, control) = tab_running_an_agent();
        registry.pause(&tab).unwrap();
        control.refuse.store(true, Ordering::SeqCst);

        // When
        let failed = registry.resume(&tab);

        // Then — oublier le groupe ici ferait perdre le seul fil qui permette de réessayer
        assert!(failed.is_err());
        assert!(registry.tabs().unwrap().iter().all(|shown| shown.paused));
    }

    #[test]
    fn given_a_tab_that_was_never_paused_when_it_is_resumed_then_nothing_is_signalled() {
        // Given
        let (registry, tab, control) = tab_running_an_agent();

        // When
        registry.resume(&tab).unwrap();

        // Then
        assert!(control.posted().is_empty());
    }

    /// Test Data Builder : un onglet où un agent reconnu tient l'avant-plan.
    ///
    /// C'est le décor de la spec §7.4 — « passer le travail à l'agent qui tourne déjà là »
    /// — et le défaut valide de tous les scénarios de composition : `claude` en avant-plan,
    /// instrumenté, à l'invite (donc pas en plein tour), et un prompt vide.
    struct ComposingTab {
        registry: PtyRegistry,
        agents: Arc<FakeAgentStates>,
        written: Arc<Mutex<Vec<u8>>>,
        tab_id: TabId,
    }

    impl ComposingTab {
        fn new() -> Self {
            let fakes = composing_registry("/dev/ash");
            fakes
                .recognition
                .knows("/usr/local/bin/claude", "claude", Instrumented::Installed);
            fakes.probe.hand_over_to_binary("/usr/local/bin/claude");
            fakes.registry.open(spec(), "01J0TAB".to_owned()).unwrap();
            fakes.agents.declare(AgentState::Waiting);
            // La première passe est ce qui fait connaître l'avant-plan de l'onglet : la
            // composition lit le dernier `TabInfo` annoncé, jamais une sonde relancée.
            fakes.registry.changes().unwrap();
            Self::from(fakes)
        }

        /// Un shell à son invite : aucun outil reconnu ne tient l'avant-plan.
        fn at_a_shell_prompt() -> Self {
            let fakes = composing_registry("/dev/ash");
            fakes.registry.open(spec(), "01J0TAB".to_owned()).unwrap();
            fakes.registry.changes().unwrap();
            Self::from(fakes)
        }

        fn from(fakes: Composing) -> Self {
            Self {
                registry: fakes.registry,
                agents: fakes.agents,
                written: fakes.written,
                tab_id: "01J0TAB".to_owned(),
            }
        }

        fn compose(&self, text: &str) -> ComposeOutcome {
            self.registry.compose(&self.tab_id, text).unwrap()
        }

        fn typed(&self, keys: &str) -> &Self {
            self.registry.write(&self.tab_id, keys.as_bytes()).unwrap();
            self
        }

        /// L'agent entre dans un tour, ou en sort.
        fn now(&self, state: AgentState) -> &Self {
            self.agents.declare(state);
            self.registry.changes().unwrap();
            self
        }

        fn terminal(&self) -> String {
            String::from_utf8(self.written.lock().unwrap().clone()).unwrap()
        }
    }

    #[test]
    fn given_an_agent_tab_with_an_empty_prompt_when_ash_composes_then_the_text_is_in_the_terminal_and_no_return_was_sent(
    ) {
        // Given — la première condition d'ADR-0015 : le texte apparaît dans le terminal,
        // à sa place, tel qu'il sera envoyé
        let tab = ComposingTab::new();

        // When
        let outcome = tab.compose("resolve the conflicts in src/probe.rs");

        // Then — la troisième condition : Ash ne presse jamais `⏎`. Dans un PTY, un saut
        // de ligne *est* la validation : le chercher dans les octets écrits est la seule
        // façon de le prouver.
        assert_eq!(outcome, ComposeOutcome::Written);
        assert_eq!(tab.terminal(), "resolve the conflicts in src/probe.rs");
        assert!(!tab.terminal().contains('\n'));
        assert!(!tab.terminal().contains('\r'));
    }

    #[test]
    fn given_a_user_typing_in_the_tab_when_ash_wants_to_compose_then_it_refuses_rather_than_inserting_in_the_middle(
    ) {
        // Given — le cas qu'ADR-0015 demande explicitement de traiter
        let tab = ComposingTab::new();
        tab.typed("explique-moi le ");

        // When
        let outcome = tab.compose("resolve the conflicts");

        // Then — l'utilisateur enverrait sinon un mélange de son texte et de celui d'Ash
        assert_eq!(outcome, ComposeOutcome::PromptNotEmpty);
        assert_eq!(tab.terminal(), "explique-moi le ");
    }

    #[test]
    fn given_a_prompt_that_ash_has_already_filled_when_it_is_asked_again_then_it_does_not_write_twice(
    ) {
        // Given — le texte composé est du texte comme un autre une fois posé : le
        // recomposer par-dessus donnerait un prompt en double, illisible et envoyable
        let tab = ComposingTab::new();
        tab.compose("resolve the conflicts");

        // When
        let outcome = tab.compose("resolve the conflicts");

        // Then
        assert_eq!(outcome, ComposeOutcome::PromptNotEmpty);
        assert_eq!(tab.terminal(), "resolve the conflicts");
    }

    #[test]
    fn given_a_tab_where_no_recognized_tool_holds_the_foreground_when_ash_wants_to_compose_then_it_refuses(
    ) {
        // Given — un shell à son invite. Y poser du texte préparerait une **commande**
        // dans le terminal de quelqu'un, ce que l'ADR-0015 n'autorise nulle part : elle
        // parle de passer le travail à l'agent qui tourne déjà là.
        let tab = ComposingTab::at_a_shell_prompt();

        // When
        let outcome = tab.compose("resolve the conflicts");

        // Then
        assert_eq!(outcome, ComposeOutcome::NoAgent);
        assert_eq!(tab.terminal(), "");
    }

    #[test]
    fn given_an_agent_in_the_middle_of_a_turn_when_ash_composes_then_the_text_waits_for_the_end_of_the_turn(
    ) {
        // Given — le corollaire de file d'attente d'ADR-0015 : écrire maintenant ferait
        // atterrir la frappe au milieu d'une sortie, pas dans le prompt
        let tab = ComposingTab::new();
        tab.now(AgentState::Working);

        // When
        let outcome = tab.compose("resolve the conflicts");

        // Then — rien n'est encore dans le terminal, et l'écran a de quoi dire pourquoi
        assert_eq!(outcome, ComposeOutcome::Queued);
        assert_eq!(tab.terminal(), "");

        // When — le tour se termine, et la passe de sonde suivante s'en aperçoit
        tab.now(AgentState::Waiting);

        // Then — le texte est posé, et toujours pas envoyé
        assert_eq!(tab.terminal(), "resolve the conflicts");
        assert!(!tab.terminal().contains('\r'));
    }

    #[test]
    fn given_a_text_waiting_for_the_end_of_a_turn_when_the_turn_ends_then_it_is_written_once_and_not_at_every_pass(
    ) {
        // Given — la passe de sonde repasse trois fois par seconde : un texte relâché à
        // chaque passe remplirait le prompt de copies
        let tab = ComposingTab::new();
        tab.now(AgentState::Working);
        tab.compose("resolve the conflicts");

        // When
        tab.now(AgentState::Waiting);
        tab.registry.changes().unwrap();
        tab.registry.changes().unwrap();

        // Then
        assert_eq!(tab.terminal(), "resolve the conflicts");
    }

    #[test]
    fn given_a_tab_is_opened_when_the_shell_starts_then_it_carries_its_own_ash_tab_id() {
        // Given
        let spawner = FakeSpawner::default();
        let env = Arc::clone(&spawner.last_env);
        let registry = registry(spawner);

        // When
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();

        // Then
        let env = env.lock().unwrap().clone();
        assert!(env.contains(&("ASH_TAB_ID".to_owned(), "01J0TAB".to_owned())));
        assert!(env.contains(&("ASH_SOCK".to_owned(), "/tmp/ash.sock".to_owned())));
        assert_eq!(opened.tab_id, "01J0TAB");
    }

    #[test]
    fn given_a_tab_is_opened_when_the_shell_starts_then_it_is_told_which_terminal_it_speaks_to() {
        // Given — un environnement quelconque, y compris celui, vide, qu'un `.app` lancé
        // par le Finder reçoit de launchd.
        let spawner = FakeSpawner::default();
        let env = Arc::clone(&spawner.last_env);
        let registry = registry(spawner);

        // When
        registry.open(spec(), "01J0TAB".to_owned()).unwrap();

        // Then — sans ça, zsh ne sait pas adresser le curseur et ZLE ajoute au lieu de
        // remplacer : taper `ll` affiche `llll`.
        let env = env.lock().unwrap().clone();
        assert!(env.contains(&("TERM".to_owned(), "xterm-256color".to_owned())));
        assert!(env.contains(&("COLORTERM".to_owned(), "truecolor".to_owned())));
    }

    #[test]
    fn given_a_pty_already_on_a_grid_when_the_same_grid_is_announced_then_no_sigwinch_is_posted() {
        // Given — un onglet ouvert en 80×24, et une TUI plein écran qui y tourne. Le panneau
        // bas (spec §4.3) donne au terminal une seconde raison de changer de boîte, en plus
        // de la fenêtre et de la colonne : trois sources indépendantes peuvent annoncer la
        // même grille l'une après l'autre.
        let spawner = FakeSpawner::default();
        let resized = Arc::clone(&spawner.resized);
        let registry = registry(spawner);
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();

        // When — le panneau s'ouvre puis se referme sans franchir une ligne entière
        registry.resize(&opened.tab_id, 80, 24).unwrap();

        // Then — redimensionner poste un `SIGWINCH`, et une TUI s'y redessine entièrement
        // (ADR-0003, reformulation du 2026-08-10)
        assert!(resized.lock().unwrap().is_empty());
    }

    #[test]
    fn given_a_pty_when_the_grid_really_changes_then_it_is_posted_once_and_the_new_one_is_kept() {
        // Given
        let spawner = FakeSpawner::default();
        let resized = Arc::clone(&spawner.resized);
        let registry = registry(spawner);
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();

        // When — le panneau prend six lignes au terminal, puis les lui rend
        registry.resize(&opened.tab_id, 80, 18).unwrap();
        registry.resize(&opened.tab_id, 80, 18).unwrap();
        registry.resize(&opened.tab_id, 80, 24).unwrap();

        // Then — une grille par changement, et le filtre ne fige pas la grille au passage
        assert_eq!(*resized.lock().unwrap(), vec![(80, 18), (80, 24)]);
    }

    #[test]
    fn given_an_open_tab_when_it_is_closed_then_the_process_is_killed_and_the_reader_released() {
        // Given
        let spawner = FakeSpawner::default();
        let killed = Arc::clone(&spawner.killed);
        let registry = registry(spawner);
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();

        // When
        registry.close(&opened.tab_id).unwrap();

        // Then
        assert!(killed.load(Ordering::SeqCst), "le shell doit être terminé");
        assert!(
            !opened.credits.acquire(),
            "le lecteur doit être réveillé avec un ordre d'arrêt"
        );
    }

    #[test]
    fn given_a_closed_tab_when_it_is_closed_again_then_it_is_not_an_error() {
        // Given
        let registry = registry(FakeSpawner::default());
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();
        registry.close(&opened.tab_id).unwrap();

        // When
        let again = registry.close(&opened.tab_id);

        // Then
        assert!(again.is_ok());
    }

    #[test]
    fn given_a_tab_that_no_longer_exists_when_writing_to_it_then_it_fails_without_panicking() {
        // Given
        let registry = registry(FakeSpawner::default());
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();
        registry.close(&opened.tab_id).unwrap();

        // When
        let written = registry.write(&opened.tab_id, b"ls\n");

        // Then
        assert!(matches!(written, Err(PtyError::UnknownTab(_))));
    }

    #[test]
    fn given_three_tabs_opened_in_turn_when_the_middle_one_closes_then_the_others_keep_their_order()
    {
        // Given — l'ordre est ce que `Cmd+1..9` numérote : il ne doit pas se réarranger
        // à la fermeture d'un onglet.
        let registry = registry(FakeSpawner::default());
        for id in ["A", "B", "C"] {
            registry.open(spec(), id.to_owned()).unwrap();
        }

        // When
        registry.close("B").unwrap();

        // Then
        assert_eq!(ids(&registry), vec!["A".to_owned(), "C".to_owned()]);
    }

    #[test]
    fn given_a_tab_closed_in_the_middle_when_a_new_one_is_opened_then_it_lands_last() {
        // Given
        let registry = registry(FakeSpawner::default());
        for id in ["A", "B", "C"] {
            registry.open(spec(), id.to_owned()).unwrap();
        }
        registry.close("A").unwrap();

        // When
        registry.open(spec(), "D".to_owned()).unwrap();

        // Then — et non pas dans le trou laissé par « A »
        assert_eq!(
            ids(&registry),
            vec!["B".to_owned(), "C".to_owned(), "D".to_owned()]
        );
    }

    #[test]
    fn given_a_tab_running_a_recognized_tool_when_the_frontend_lists_the_tabs_then_the_line_names_the_tool_and_not_its_binary(
    ) {
        // Given — l'installateur officiel de Claude Code pose un binaire dont le nom de
        // fichier est le numéro de version, et c'est ce nom que l'onglet affichait
        // (ADR-0006). Le registre **demande** l'identité, il ne la déduit pas
        let (registry, probe, _locator, _agents, recognition) = recognizing_registry("/dev/ash");
        let binary = "/Users/ash/.local/share/claude/versions/2.1.234";
        recognition.knows(binary, "claude", Instrumented::Missing);
        registry.open(spec(), "A".to_owned()).unwrap();
        probe.hand_over_to_binary(binary);

        // When
        let described = registry.tabs().unwrap().into_iter().next();

        // Then — la ligne dit `claude`, et l'écran sait que rien ne l'instrumente
        let tab = described.expect("l'onglet existe");
        assert_eq!(tab.process, "claude".to_owned());
        assert_eq!(
            tab.agent,
            Some(RecognizedAgent {
                command: "claude".to_owned(),
                adapter: "claude-code".to_owned(),
                instrumented: Instrumented::Missing,
            })
        );
    }

    #[test]
    fn given_a_tab_running_a_recognized_tool_when_the_probe_loop_passes_again_then_nothing_is_announced(
    ) {
        // Given — la fiche d'onglet est comparée **entière** pour décider d'émettre
        // `ash://tab-changed`. Une reconnaissance qui changerait d'une passe à l'autre —
        // parce qu'elle relirait un fichier, ou daterait sa réponse — ferait redessiner la
        // sidebar entière trois fois par seconde
        let (registry, probe, _locator, _agents, recognition) = recognizing_registry("/dev/ash");
        let binary = "/Users/ash/.local/share/claude/versions/2.1.234";
        recognition.knows(binary, "claude", Instrumented::Missing);
        registry.open(spec(), "A".to_owned()).unwrap();
        probe.hand_over_to_binary(binary);
        registry.changes().unwrap();

        // When
        let again: Vec<TabInfo> = (0..10).flat_map(|_| registry.changes().unwrap()).collect();

        // Then
        assert_eq!(again, Vec::new());
    }

    #[test]
    fn given_three_tabs_running_the_same_tool_when_the_settings_window_asks_what_runs_then_it_is_named_once(
    ) {
        // Given — la fenêtre de réglages propose de déclarer ce qu'Ash a vu tourner
        // (ADR-0006). Trois onglets sur `claude` sont **un** outil, pas trois suggestions
        let (registry, probe, _locator, _agents, recognition) = recognizing_registry("/dev/ash");
        let binary = "/Users/ash/.local/share/claude/versions/2.1.234";
        recognition.knows(binary, "claude", Instrumented::Missing);
        for id in ["A", "B", "C"] {
            registry.open(spec(), id.to_owned()).unwrap();
        }
        probe.hand_over_to_binary(binary);
        // La boucle de sonde est passée : c'est elle qui range la reconnaissance dans la
        // fiche annoncée, et c'est cette fiche-là qu'on relit — sans resonder.
        registry.changes().unwrap();

        // When
        let running = registry.recognized_tools().unwrap();

        // Then
        assert_eq!(
            running,
            vec![RecognizedProvider {
                command: "claude".to_owned(),
                adapter: "claude-code".to_owned(),
            }]
        );
    }

    #[test]
    fn given_tabs_where_no_tool_has_ever_been_seen_when_the_settings_window_asks_what_runs_then_nothing_is_proposed(
    ) {
        // Given — des shells à leur invite. Rien ne doit sortir d'ici : la fenêtre ne
        // propose que ce qu'Ash a **vu**, et jamais ce qu'un parcours du `PATH` trouverait
        let (registry, _probe, _locator, _agents, recognition) = recognizing_registry("/dev/ash");
        recognition.knows("/opt/claude", "claude", Instrumented::Installed);
        registry.open(spec(), "A".to_owned()).unwrap();
        registry.changes().unwrap();

        // When
        let running = registry.recognized_tools().unwrap();

        // Then
        assert_eq!(running, Vec::new());
    }

    #[test]
    fn given_a_tab_at_its_prompt_when_the_frontend_lists_the_tabs_then_no_tool_is_named() {
        // Given — un onglet posé à son invite n'est pas un agent, et ne le devient qu'en
        // lançant quelque chose (ADR-0006)
        let (registry, _probe, _locator, _agents, recognition) = recognizing_registry("/dev/ash");
        recognition.knows("/bin/bash", "claude", Instrumented::Installed);
        registry.open(spec(), "A".to_owned()).unwrap();

        // When
        let described = registry.tabs().unwrap().into_iter().next();

        // Then — la reconnaissance n'est même pas consultée pour un shell
        assert_eq!(described.and_then(|tab| tab.agent), None);
    }

    #[test]
    fn given_a_tab_whose_shell_has_moved_when_the_frontend_lists_the_tabs_then_it_learns_the_current_directory(
    ) {
        // Given — l'onglet est parti de /dev/ash, la sonde le voit dans un worktree
        let (registry, _probe) = observed_registry("/dev/ash/worktrees/probe");

        // When
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();

        // Then — c'est ce répertoire-là que « nouvel onglet dans le worktree courant »
        // (spec §4.4) reprend, pas celui de lancement
        assert_eq!(
            registry.tabs().unwrap().into_iter().next().map(described),
            Some(("A".to_owned(), "/dev/ash/worktrees/probe".to_owned()))
        );
    }

    #[test]
    fn given_a_tab_the_probe_cannot_observe_when_the_frontend_lists_the_tabs_then_it_falls_back_to_the_start_directory(
    ) {
        // Given — un système qui ne répond pas ne doit pas produire un onglet sans nom
        let registry = registry(FakeSpawner::observable());

        // When
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();

        // Then
        assert_eq!(
            registry.tabs().unwrap().into_iter().next().map(described),
            Some(("A".to_owned(), "/dev/ash".to_owned()))
        );
    }

    #[test]
    fn given_a_tab_that_changed_repository_when_the_loop_sweeps_then_the_announced_tab_carries_its_new_location(
    ) {
        // Given — un `cd` d'un dépôt vers un autre. C'est ce qui fait migrer l'onglet
        // d'un groupe à l'autre dans la sidebar ([ADR-0012]), et la sidebar ne résout
        // rien elle-même ([ADR-0009]) : la localisation doit voyager avec l'onglet.
        let (registry, probe, _) = located_registry("/dev/ash");
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();
        registry.changes().unwrap(); // la passe qui découvre l'onglet
        probe.move_to("/dev/omelette-web/src");

        // When
        let announced = registry.changes().unwrap();

        // Then
        let location = announced
            .into_iter()
            .next()
            .and_then(|tab| tab.location)
            .expect("l'onglet annoncé doit être situé");
        assert_eq!(
            location.repo.map(|repo| repo.name),
            Some("omelette-web".to_owned())
        );
        assert_eq!(location.worktree_name, "src");
    }

    #[test]
    fn given_a_tab_that_has_not_moved_when_it_is_listed_again_then_its_location_is_not_resolved_again(
    ) {
        // Given — la résolution lit des fichiers sur le disque, et la liste est relue à
        // chaque ouverture d'onglet pendant que la boucle passe trois fois par seconde.
        let (registry, probe, locator) = located_registry("/dev/ash");
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();

        // When — quatre lectures pour un onglet immobile, puis un `cd`
        for _ in 0..4 {
            registry.tabs().unwrap();
        }
        let while_still = locator.calls();
        probe.move_to("/dev/omelette-web");
        registry.tabs().unwrap();

        // Then — une résolution par répertoire visité, et pas une par passe
        assert_eq!(while_still, 1);
        assert_eq!(locator.calls(), 2);
    }

    /// Un onglet ouvert dans `/dev/omelette`, un dépôt **sans worktree lié** : à plat.
    ///
    /// Le décor des trois scénarios de regroupement, où c'est le dépôt qui change de forme
    /// pendant que l'onglet, lui, ne bouge pas.
    fn tab_in_a_flat_repository() -> (PtyRegistry, Arc<CountingLocator>) {
        let (registry, _probe, locator) = located_registry("/dev/omelette");
        locator.flatten("/dev/omelette");
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/omelette").build(),
                "A".to_owned(),
            )
            .unwrap();
        // La passe qui découvre l'onglet : c'est elle qui retient sa localisation.
        registry.changes().unwrap();
        (registry, locator)
    }

    fn announced_repo(announced: Vec<TabInfo>) -> Option<Option<String>> {
        announced.into_iter().next().map(|tab| {
            tab.location
                .and_then(|place| place.repo)
                .map(|repo| repo.name)
        })
    }

    #[test]
    fn given_a_tab_in_a_flat_repository_when_the_repository_gains_a_linked_worktree_then_the_tab_is_announced_under_its_group(
    ) {
        // Given — un `git worktree add` lancé depuis un autre terminal : le dépôt passe de
        // la forme à plat à la forme groupée (ADR-0012) sans que le `cwd` de l'onglet ne
        // bouge d'un caractère. Sans redémarrage, l'onglet doit rejoindre le groupe —
        // sinon le même projet s'affiche deux fois, à plat d'un côté et groupé de l'autre.
        let (registry, locator) = tab_in_a_flat_repository();
        locator.group("/dev/omelette");

        // When — la surveillance de `.git` a vu l'entrée apparaître
        registry.invalidate_locations();
        let announced = registry.changes().unwrap();

        // Then
        assert_eq!(announced_repo(announced), Some(Some("omelette".to_owned())));
    }

    #[test]
    fn given_a_tab_in_a_grouped_repository_when_the_last_linked_worktree_disappears_then_the_tab_falls_back_flat(
    ) {
        // Given — le cas inverse, et il compte autant : un dépôt qui a perdu ses frères
        // resterait affiché sur deux niveaux avec un seul enfant jusqu'au redémarrage.
        let (registry, _probe, locator) = located_registry("/dev/omelette");
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/omelette").build(),
                "A".to_owned(),
            )
            .unwrap();
        registry.changes().unwrap();
        locator.flatten("/dev/omelette");

        // When
        registry.invalidate_locations();
        let announced = registry.changes().unwrap();

        // Then — situé, mais sans dépôt au-dessus : c'est la forme à plat d'ADR-0012
        assert_eq!(announced_repo(announced), Some(None));
    }

    #[test]
    fn given_a_repository_whose_shape_did_not_change_when_a_write_is_signalled_then_the_tab_is_resolved_once_and_never_announced(
    ) {
        // Given — une écriture dans `worktrees/` ne veut pas dire que *cet* onglet-ci a
        // changé de groupe. Deux garde-fous en un : la webview n'est pas réveillée pour un
        // état identique, et le signal ne rouvre pas la résolution à chaque passe — ce
        // serait le sondage que la spec §5.3 écarte, revenu par la porte de derrière.
        let (registry, locator) = tab_in_a_flat_repository();
        let resolutions_before = locator.calls();

        // When
        registry.invalidate_locations();
        let announced: Vec<TabInfo> = (0..10).flat_map(|_| registry.changes().unwrap()).collect();

        // Then
        assert!(announced.is_empty());
        assert_eq!(locator.calls(), resolutions_before + 1);
    }

    #[test]
    fn given_tabs_that_nobody_disturbs_when_the_loop_sweeps_then_their_locations_are_never_resolved_again(
    ) {
        // Given — la boucle passe trois fois par seconde. Au repos, aucune de ces passes
        // n'a le droit de toucher au disque : c'est le critère « avec 5 dépôts ouverts, la
        // consommation CPU reste négligeable ».
        let (registry, locator) = tab_in_a_flat_repository();
        let resolutions_after_discovery = locator.calls();

        // When — cent passes sans le moindre signal
        for _ in 0..100 {
            registry.changes().unwrap();
        }

        // Then
        assert_eq!(resolutions_after_discovery, 1);
        assert_eq!(locator.calls(), resolutions_after_discovery);
    }

    #[test]
    fn given_a_shell_at_its_prompt_when_a_program_takes_the_foreground_then_the_tab_stops_being_idle(
    ) {
        // Given — l'onglet sait qui tient son avant-plan, et c'est tout ce que la sonde a le
        // droit de dire ([ADR-0007]). L'état, lui, est **demandé** : le registre n'en déduit
        // plus aucun, et la sidebar n'en déduit pas davantage de son côté ([ADR-0009]).
        let (registry, probe, _) = located_registry("/dev/ash");
        registry.open(spec(), "A".to_owned()).unwrap();
        let at_prompt = registry.tabs().unwrap();
        probe.hand_over_to("claude");

        // When
        let running = registry.tabs().unwrap();

        // Then
        assert_eq!(
            at_prompt
                .first()
                .map(|tab| (tab.state, tab.process.clone())),
            Some((AgentState::Idle, "bash".to_owned()))
        );
        assert_eq!(
            running.first().map(|tab| (tab.state, tab.process.clone())),
            Some((AgentState::Working, "claude".to_owned()))
        );
    }

    #[test]
    fn given_a_tab_that_nothing_disturbs_when_a_hook_declares_an_agent_state_then_the_loop_announces_it(
    ) {
        // Given — c'est le chemin qu'ADR-0007 ouvre, et le seul qui produise `waiting` : un
        // hook parle alors que rien ne remue dans le terminal — même `cwd`, même processus
        // en avant-plan. Sans ceci, l'état décidé par la feature `agents` attendrait le
        // prochain `cd` pour atteindre l'écran, c'est-à-dire indéfiniment.
        let (registry, _probe, _locator, agents) = supervised_registry("/dev/ash");
        registry.open(spec(), "A".to_owned()).unwrap();
        registry.changes().unwrap(); // la passe qui découvre l'onglet

        // When
        agents.declare(AgentState::Waiting);
        let announced = registry.changes().unwrap();
        let settled = registry.changes().unwrap();

        // Then — annoncé une fois, et pas trois fois par seconde
        assert_eq!(
            announced.iter().map(|tab| tab.state).collect::<Vec<_>>(),
            vec![AgentState::Waiting]
        );
        assert_eq!(settled, vec![]);
    }

    #[test]
    fn given_a_tab_whose_agent_keeps_working_when_the_clock_runs_for_an_hour_then_nothing_crosses_the_frontier(
    ) {
        // Given — le piège de cette tranche, vu d'ici : la fiche d'un onglet est comparée
        // **entière** pour décider s'il faut annoncer quelque chose. Si elle portait une
        // durée plutôt qu'une date, elle changerait chaque seconde pour chaque onglet actif,
        // et l'event ponctuel deviendrait un flux — un rendu complet de la sidebar par
        // seconde, pour animer un compteur qui se calcule à l'affichage.
        let (registry, _probe, _locator, agents) = supervised_registry("/dev/ash");
        registry.open(spec(), "A".to_owned()).unwrap();
        agents.declare(AgentState::Working);
        // Et un sous-agent sous lui : c'est la fiche qui a **le plus** de chances de bouger,
        // puisqu'elle porte une seconde date. Le `working · 15m22s` d'une ligne fille se
        // calcule à l'affichage, exactement comme celui de l'onglet.
        agents.declare_subagent("explore", AgentState::Working, 0);
        // Et sa jauge de contexte : elle ne se relit qu'à l'arrivée d'un hook, jamais à une
        // passe de sonde. Une mesure qui se rafraîchirait toutes les 300 ms serait le même
        // piège que la durée, et il coûterait en plus une lecture de disque par passe.
        agents.declare_usage(SessionUsage {
            used_tokens: 146_273,
            window_tokens: Some(200_000),
            model: Some("Opus 5".to_owned()),
        });
        let discovered = registry.changes().unwrap(); // la passe qui découvre l'onglet

        // When — une heure de boucle, et rien d'autre que le temps qui passe
        let announced: Vec<TabInfo> = (0..3600)
            .flat_map(|_| {
                agents.advance(1_000);
                registry.changes().unwrap()
            })
            .collect();

        // Then — et la date d'entrée annoncée une seule fois est restée celle de l'entrée,
        // pour l'onglet comme pour sa ligne fille
        assert_eq!(
            discovered
                .iter()
                .map(|tab| (tab.state_since, tab.subagents.len(), tab.usage.clone()))
                .collect::<Vec<_>>(),
            vec![(
                0,
                1,
                Some(SessionUsage {
                    used_tokens: 146_273,
                    window_tokens: Some(200_000),
                    model: Some("Opus 5".to_owned()),
                })
            )]
        );
        assert_eq!(announced, vec![]);
    }

    #[test]
    fn given_a_tab_that_is_closed_when_it_leaves_the_registry_then_its_agent_state_is_forgotten_too(
    ) {
        // Given — l'état d'un agent ne survit pas à son onglet ([ADR-0009]) : un ulid
        // réattribué hériterait sinon d'un agent que plus aucun processus ne porte.
        let (registry, _probe, _locator, agents) = supervised_registry("/dev/ash");
        registry.open(spec(), "A".to_owned()).unwrap();

        // When
        registry.close("A").unwrap();

        // Then
        assert_eq!(agents.forgotten(), vec!["A".to_owned()]);
    }

    #[test]
    fn given_a_shell_that_handed_the_terminal_over_when_the_tab_is_questioned_then_it_reports_a_running_process(
    ) {
        // Given
        let spawner = FakeSpawner::default();
        let foreground = Arc::clone(&spawner.foreground);
        let registry = registry(spawner);
        registry.open(spec(), "A".to_owned()).unwrap();

        // When
        foreground.store(true, Ordering::SeqCst);

        // Then — le frontend n'a plus qu'à demander confirmation avant de fermer
        assert!(registry.has_foreground_process("A").unwrap());
    }

    #[test]
    fn given_an_open_tab_when_the_webview_acks_then_the_reader_gets_a_credit_back() {
        // Given — la fenêtre est vidée, le lecteur serait bloqué
        let registry = registry(FakeSpawner::default());
        let opened = registry.open(spec(), "01J0TAB".to_owned()).unwrap();
        for _ in 0..WINDOW {
            assert!(opened.credits.acquire());
        }

        // When
        registry.ack(&opened.tab_id).unwrap();

        // Then
        assert!(
            opened.credits.acquire(),
            "l'acquittement doit débloquer une émission"
        );
    }

    #[test]
    fn given_a_tab_that_moved_since_the_last_listing_when_the_tabs_are_listed_again_then_the_new_directory_is_reported(
    ) {
        // Given — un `cd` après une première lecture de la liste
        let (registry, probe) = observed_registry("/dev/ash");
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();
        assert_eq!(registry.tabs().unwrap()[0].cwd, "/dev/ash");
        probe.move_to("/tmp");

        // When
        let listed = registry.tabs().unwrap();

        // Then — chaque lecture sonde à nouveau. Rendre la valeur de la lecture
        // précédente ferait partir `Cmd+T` du répertoire d'il y a une ouverture d'onglet.
        assert_eq!(listed[0].cwd, "/tmp");
    }

    #[test]
    fn given_a_tab_that_moved_when_the_loop_sweeps_then_the_change_is_reported_once_and_not_again()
    {
        // Given
        let (registry, probe) = observed_registry("/dev/ash");
        registry
            .open(
                SpecBuilder::new().starting_in("/dev/ash").build(),
                "A".to_owned(),
            )
            .unwrap();
        registry.changes().unwrap(); // la passe qui découvre l'onglet
        probe.move_to("/tmp");

        // When
        let moved = registry.changes().unwrap();
        let settled = registry.changes().unwrap();

        // Then — c'est ce qui fait suivre le titre de l'onglet, sans réveiller la webview
        // trois fois par seconde pour un onglet posé à son invite
        assert_eq!(
            moved.into_iter().map(described).collect::<Vec<_>>(),
            vec![("A".to_owned(), "/tmp".to_owned())]
        );
        assert_eq!(settled, vec![]);
    }

    #[test]
    fn given_a_probe_pass_in_flight_when_a_keystroke_arrives_then_it_does_not_wait_behind_the_probe(
    ) {
        // Given — une sonde qui bloque, comme un `proc_pidinfo` sur un système chargé.
        // À 3 Hz par onglet, une passe qui tient le verrou du registre met chaque frappe
        // de l'utilisateur derrière elle.
        let (entered, has_entered) = mpsc::channel();
        let (release, wait_for_release) = mpsc::channel::<()>();
        let registry = Arc::new(PtyRegistry::new(
            Box::new(FakeSpawner::observable()),
            Arc::new(BlockingProbe {
                entered,
                release: Mutex::new(wait_for_release),
            }),
            Arc::new(CountingLocator::default()),
            Arc::new(super::super::recognition::NoRecognition),
            Arc::new(FakeAgentStates::default()),
            Arc::new(crate::features::pty::fakes::FakeProcessControl::default()),
        ));
        registry.open(spec(), "A".to_owned()).unwrap();

        let sweeping = {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || registry.changes())
        };
        has_entered.recv().unwrap();

        // When — la frappe part pendant que la passe de sonde est bloquée
        let (typed, keystroke) = mpsc::channel();
        let typing = {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || {
                let written = registry.write("A", b"ls\n");
                typed.send(written).unwrap();
            })
        };

        // Then — elle aboutit sans attendre la fin de la passe
        let written = keystroke.recv_timeout(std::time::Duration::from_secs(5));
        release.send(()).unwrap();
        sweeping.join().unwrap().unwrap();
        typing.join().unwrap();
        assert!(
            matches!(written, Ok(Ok(()))),
            "la frappe a attendu la fin de la sonde : {written:?}"
        );
    }

    /// Une sonde qui prévient qu'elle est entrée, puis attend qu'on la libère.
    ///
    /// Elle ne décrit aucun système réel : ce qu'elle rend visible, c'est la durée d'une
    /// passe, et ce qui reste bloqué pendant.
    struct BlockingProbe {
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl Probe for BlockingProbe {
        fn foreground_pgid(&self, _terminal: RawFd) -> Result<Pid, ProbeError> {
            self.entered.send(()).unwrap();
            let _ = self.release.lock().unwrap().recv();
            Ok(100)
        }

        fn inspect(&self, pid: Pid) -> Result<ProcessInfo, ProbeError> {
            Ok(ProcessInfo {
                pid,
                name: "bash".to_owned(),
                executable: PathBuf::from("/bin/bash"),
                cwd: PathBuf::from("/dev/ash"),
            })
        }

        fn argv0(&self, _pid: Pid) -> Option<String> {
            None
        }
    }
}
