//! La surface de la feature vers le frontend : onze commandes, un event, et l'ouverture de
//! sa fenêtre.
//!
//! Le frontend ne connaît de `settings` que ces noms et la forme de [`SettingsSnapshot`].
//! Il **rend** la liste et ce qu'elle a prouvé ; il ne les détient pas
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! **Le résultat en deux temps traverse la frontière tel quel** : une commande de
//! vérification répond dès que les tests 1 à 3 ont parlé, et le test 4 arrive plus tard par
//! [`SETTINGS_VERIFIED`]. C'est ce qui permet au bouton d'installation de s'allumer sans
//! attendre le démarrage d'un programme, sans que la fenêtre ait à connaître la règle.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::error::SettingsError;
use super::registry::{Changed, SecondPass, ToolRegistry};
use super::tool::{NewTool, ToolDeclaration};
use super::verification::{ToolTest, Verification};

/// Le label de la seconde fenêtre. Contrat avec `src-tauri/capabilities/settings.json`,
/// qui lui accorde ses permissions par ce nom.
pub const SETTINGS_WINDOW: &str = "settings";

/// L'event du **second temps** : le test 4 a répondu pour une entrée.
pub const SETTINGS_VERIFIED: &str = "ash://settings-verified";

/// La page de la webview. Contrat avec `settings.html` et l'entrée du même nom dans
/// `vite.config.ts` : une seconde fenêtre est une seconde page, pas un second état de la
/// première.
const SETTINGS_PAGE: &str = "settings.html";

/// Un des quatre tests, tel que la fenêtre l'annonce.
///
/// Les libellés voyagent **du backend vers l'écran**, et ne sont pas écrits des deux côtés :
/// c'est ici que les tests existent, donc c'est ici qu'ils se nomment. Une liste recopiée
/// dans la vue finirait par décrire un test que la séquence ne lance plus.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestDescription {
    pub number: u8,
    /// Le libellé long — `the folder exists and is readable`.
    pub label: String,
    /// Le libellé court de la note de barème — `folder readable`.
    pub short_label: String,
    /// Son échec invalide-t-il l'entrée, ou la réserve-t-il seulement ?
    pub decisive: bool,
}

/// Tout ce que la fenêtre de réglages a besoin de savoir pour se dessiner.
///
/// Un seul aller-retour plutôt qu'un par question : la liste, les adaptateurs et les tests
/// changent ensemble — un ajout ne veut rien dire sans les adaptateurs qui le rendaient
/// possible — et deux commandes laisseraient la fenêtre afficher un instant l'une sans
/// l'autre.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub tools: Vec<ToolDeclaration>,
    /// Les adaptateurs que cette version d'Ash embarque, dans l'ordre où on les propose.
    pub adapters: Vec<String>,
    /// Les quatre tests de la spec §9.1, dans l'ordre où ils se lancent.
    pub tests: Vec<TestDescription>,
}

impl SettingsSnapshot {
    fn of(registry: &ToolRegistry) -> Result<Self, SettingsError> {
        Ok(Self::around(registry.tools()?, registry))
    }

    /// L'instantané d'une liste que le registre vient de rendre.
    ///
    /// Une modification rend déjà la liste entière : la relire prendrait le verrou une
    /// seconde fois, et rendrait un instantané qui n'est pas celui que la modification a
    /// produit — la réponse décrirait un registre d'après l'appel, pas son résultat.
    fn around(tools: Vec<ToolDeclaration>, registry: &ToolRegistry) -> Self {
        Self {
            tools,
            adapters: registry.adapters(),
            tests: ToolTest::ALL
                .iter()
                .map(|test| TestDescription {
                    number: test.number(),
                    label: test.label().to_owned(),
                    short_label: test.short_label().to_owned(),
                    decisive: test.decisive(),
                })
                .collect(),
        }
    }
}

/// Ce que le second temps rapporte, pour une entrée nommée.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verified {
    pub command: String,
    pub verification: Verification,
    /// Ce que [`ToolDeclaration::verified`] vaut désormais pour cette entrée.
    ///
    /// Il voyage **avec** le résultat plutôt que d'être redéduit à l'arrivée : c'est le
    /// oui/non qui décide d'écrire chez l'utilisateur, et le recalculer côté fenêtre en
    /// ferait un second propriétaire — celui qui divergerait sans bruit le jour où
    /// `verified` cesserait d'être exactement `allows_hooks`
    /// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    pub verified: bool,
}

impl Verified {
    /// Le seul endroit où l'event se construit, et il lit `allows_hooks` comme
    /// [`ToolDeclaration::verified_by`] : une seule règle, deux lecteurs.
    fn of(command: String, verification: Verification) -> Self {
        Self {
            verified: verification.allows_hooks,
            command,
            verification,
        }
    }
}

/// Les commandes déclarées, lues par la fenêtre en s'affichant.
#[tauri::command]
pub fn settings_tools(
    registry: tauri::State<'_, Arc<ToolRegistry>>,
) -> Result<SettingsSnapshot, SettingsError> {
    SettingsSnapshot::of(&registry)
}

/// Ajoute une entrée — le bouton `add` du formulaire.
#[tauri::command]
pub fn settings_declare_tool<R: Runtime>(
    app: AppHandle<R>,
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    tool: NewTool,
) -> Result<SettingsSnapshot, SettingsError> {
    answer(app, &registry, registry.declare(tool)?)
}

/// Retire une entrée — le `✕` de l'en-tête de carte.
#[tauri::command]
pub fn settings_forget_tool(
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    command: String,
) -> Result<SettingsSnapshot, SettingsError> {
    Ok(SettingsSnapshot::around(
        registry.forget(&command)?,
        &registry,
    ))
}

/// Change le dossier ou l'adaptateur d'une entrée — la frappe dans le champ de chemin, le
/// menu d'adaptateur, et le bouton `apply` d'une correction proposée.
#[tauri::command]
pub fn settings_retarget_tool<R: Runtime>(
    app: AppHandle<R>,
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    command: String,
    adapter: String,
    config: Option<String>,
) -> Result<SettingsSnapshot, SettingsError> {
    let changed = registry.retarget(&command, &adapter, config.as_deref())?;
    answer(app, &registry, changed)
}

/// Relance la séquence sur une entrée — le bouton `re-verify` d'une carte.
#[tauri::command]
pub fn settings_verify_tool<R: Runtime>(
    app: AppHandle<R>,
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    command: String,
) -> Result<SettingsSnapshot, SettingsError> {
    let changed = registry.verify(&command)?;
    answer(app, &registry, changed)
}

/// Relance la séquence sur toute la liste — le bouton `re-verify all`.
#[tauri::command]
pub fn settings_verify_all<R: Runtime>(
    app: AppHandle<R>,
    registry: tauri::State<'_, Arc<ToolRegistry>>,
) -> Result<SettingsSnapshot, SettingsError> {
    let changed = registry.verify_all()?;
    answer(app, &registry, changed)
}

/// Ramène une entrée à son dernier dossier valide — le `↺` de l'en-tête de carte.
#[tauri::command]
pub fn settings_reset_tool<R: Runtime>(
    app: AppHandle<R>,
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    command: String,
) -> Result<SettingsSnapshot, SettingsError> {
    let changed = registry.reset(&command)?;
    answer(app, &registry, changed)
}

/// Annule la réinitialisation — le `undo the reset` de la bannière de doublon.
#[tauri::command]
pub fn settings_undo_reset<R: Runtime>(
    app: AppHandle<R>,
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    command: String,
) -> Result<SettingsSnapshot, SettingsError> {
    let changed = registry.undo_reset(&command)?;
    answer(app, &registry, changed)
}

/// Pose ou met à jour le bloc de hooks — le bouton `install` / `update` de la ligne.
///
/// **C'est le seul geste du frontend qui écrive dans un fichier de l'utilisateur.** Il ne
/// porte aucune condition : celle qui décide est en Rust, et elle est la même qui a allumé
/// le bouton ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
#[tauri::command]
pub fn settings_install_hooks(
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    command: String,
) -> Result<SettingsSnapshot, SettingsError> {
    Ok(SettingsSnapshot::around(
        registry.install_hooks(&command)?,
        &registry,
    ))
}

/// Retire le bloc et ses marqueurs — le `remove` de l'état `installed`.
#[tauri::command]
pub fn settings_remove_hooks(
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    command: String,
) -> Result<SettingsSnapshot, SettingsError> {
    Ok(SettingsSnapshot::around(
        registry.remove_hooks(&command)?,
        &registry,
    ))
}

/// Vérifie une saisie du formulaire d'ajout, sans rien ajouter.
#[tauri::command]
pub fn settings_verify_draft<R: Runtime>(
    app: AppHandle<R>,
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    tool: NewTool,
) -> Result<Verification, SettingsError> {
    let (shown, pending) = registry.verify_draft(&tool);
    follow_up(app, Arc::clone(&registry), pending.into_iter().collect());
    Ok(shown)
}

/// Répond avec ce que le premier temps a produit, et met les seconds temps en route.
fn answer<R: Runtime>(
    app: AppHandle<R>,
    registry: &Arc<ToolRegistry>,
    changed: Changed,
) -> Result<SettingsSnapshot, SettingsError> {
    let snapshot = SettingsSnapshot::around(changed.tools, registry);
    follow_up(app, Arc::clone(registry), changed.pending);
    Ok(snapshot)
}

/// Lance les seconds temps, chacun sur son fil, et annonce chaque réponse.
///
/// Un fil par entrée et non un fil unique qui les enchaînerait : la maquette demande une
/// relance **en parallèle**, et une entrée dont la commande met cinq secondes à répondre
/// retarderait sinon toutes les suivantes. Ce sont les jetons de
/// [`super::permits`](super::permits) — et non le nombre de fils — qui bornent le nombre de
/// programmes réellement démarrés en même temps : un fil qui attend son tour ne coûte rien,
/// un programme qui démarre, si.
fn follow_up<R: Runtime>(app: AppHandle<R>, registry: Arc<ToolRegistry>, pending: Vec<SecondPass>) {
    for next in pending {
        let app = app.clone();
        let registry = Arc::clone(&registry);
        std::thread::spawn(move || {
            let verification = registry.second_pass(&next);
            // Un registre empoisonné n'a pas à faire paniquer un fil de fond : la fenêtre
            // garde ce que le premier temps lui a dit, ce qui reste vrai.
            let _ = registry.settle(&next, verification.clone());
            let _ = app.emit(SETTINGS_VERIFIED, Verified::of(next.command, verification));
        });
    }
}

/// Ouvre la fenêtre de réglages, ou la ramène devant si elle est déjà là.
///
/// Construite à l'exécution plutôt que déclarée dans `tauri.conf.json` : une fenêtre
/// déclarée y est créée au démarrage, et il faudrait alors la cacher puis la montrer —
/// donc porter un état « ouverte » que personne ne détient. Ici elle existe quand on la
/// demande, et disparaît quand on la ferme.
///
/// Les mesures viennent de la maquette : **800 × 600 est la taille minimale de
/// lisibilité**, pas une taille imposée. C'est pourquoi elle est à la fois la taille
/// d'ouverture et le minimum — rien ne doit pouvoir être coupé.
///
/// Un échec d'ouverture n'arrête rien : le reste d'Ash continue de tourner, et le message
/// sur la sortie d'erreur est ce qui rend la panne trouvable.
pub fn open<R: Runtime>(app: &AppHandle<R>) {
    if let Some(existing) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return;
    }

    let builder = tauri::WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW,
        tauri::WebviewUrl::App(SETTINGS_PAGE.into()),
    )
    .title("settings — ash")
    .inner_size(800.0, 600.0)
    .min_inner_size(800.0, 600.0)
    // Comme la fenêtre principale : les pastilles de macOS sont dans la webview, et c'est
    // la bande de titre d'`app/titlebar.ts` qui leur réserve leur place et rend la fenêtre
    // saisissable.
    .title_bar_style(tauri::TitleBarStyle::Overlay)
    .hidden_title(true);

    if let Err(why) = builder.build() {
        eprintln!("ash: la fenêtre de réglages ne s'est pas ouverte : {why}");
    }
}
