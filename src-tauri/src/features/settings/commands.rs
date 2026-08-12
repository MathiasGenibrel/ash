//! La surface de la feature vers le frontend : trois commandes, et l'ouverture de sa
//! fenêtre.
//!
//! Le frontend ne connaît de `settings` que ces noms et la forme de [`SettingsSnapshot`].
//! Il **rend** la liste ; il ne la détient pas
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).

use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};

use super::error::SettingsError;
use super::registry::ToolRegistry;
use super::tool::{NewTool, ToolDeclaration};

/// Le label de la seconde fenêtre. Contrat avec `src-tauri/capabilities/settings.json`,
/// qui lui accorde ses permissions par ce nom.
pub const SETTINGS_WINDOW: &str = "settings";

/// La page de la webview. Contrat avec `settings.html` et l'entrée du même nom dans
/// `vite.config.ts` : une seconde fenêtre est une seconde page, pas un second état de la
/// première.
const SETTINGS_PAGE: &str = "settings.html";

/// Tout ce que la fenêtre de réglages a besoin de savoir pour se dessiner.
///
/// Un seul aller-retour plutôt qu'un par question : la liste et les adaptateurs changent
/// ensemble — un ajout ne veut rien dire sans les adaptateurs qui le rendaient possible —
/// et deux commandes laisseraient la fenêtre afficher un instant l'une sans l'autre.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub tools: Vec<ToolDeclaration>,
    /// Les adaptateurs que cette version d'Ash embarque, dans l'ordre où on les propose.
    pub adapters: Vec<String>,
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
            adapters: registry.adapters().to_vec(),
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
///
/// L'entrée arrive **non vérifiée**, donc rien n'est écrit dans `~/.ash/config.toml` : la
/// vérification des quatre tests de la spec §9.1 est l'issue #15, et c'est elle qui ouvrira
/// l'écriture.
#[tauri::command]
pub fn settings_declare_tool(
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    tool: NewTool,
) -> Result<SettingsSnapshot, SettingsError> {
    Ok(SettingsSnapshot::around(registry.declare(tool)?, &registry))
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
