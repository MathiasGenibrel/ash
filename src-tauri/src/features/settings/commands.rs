//! La surface de la feature vers le frontend : douze commandes, un event, et l'ouverture de
//! sa fenêtre.
//!
//! Le frontend ne connaît de `settings` que ces noms et la forme de [`SettingsSnapshot`].
//! Il **rend** la liste et ce qu'elle a prouvé ; il ne les détient pas
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
//!
//! **Les noms de commande sont jugés ici, et une seule fois.** Les paramètres restent des
//! chaînes — c'est ce que la webview envoie, et le contrat sur le fil ne bouge pas — mais
//! chacun devient un [`Command`] avant de toucher au registre. C'est ce qui fait qu'aucune
//! signature de `settings` ne porte plus un nom que rien n'a jugé, et qu'une commande de
//! plus ne peut pas passer à côté de la règle sans que le compilateur le dise.
//!
//! **Le résultat en deux temps traverse la frontière tel quel** : une commande de
//! vérification répond dès que les tests 1 à 3 ont parlé, et le test 4 arrive plus tard par
//! [`SETTINGS_VERIFIED`]. C'est ce qui permet au bouton d'installation de s'allumer sans
//! attendre le démarrage d'un programme, sans que la fenêtre ait à connaître la règle.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::error::SettingsError;
use super::hooks::HooksReport;
use super::notifications::{self, NotificationsReport};
use super::registry::{Changed, SecondPass, ToolRegistry};
use super::tool::{NewTool, ToolDeclaration};
use super::values::Command;
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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Verified {
    pub command: Command,
    pub verification: Verification,
    /// Ce que [`ToolDeclaration::verified`] vaut désormais pour cette entrée.
    ///
    /// Il voyage **avec** le résultat plutôt que d'être redéduit à l'arrivée : c'est le
    /// oui/non qui décide d'écrire chez l'utilisateur, et le recalculer côté fenêtre en
    /// ferait un second propriétaire — celui qui divergerait sans bruit le jour où
    /// `verified` cesserait d'être exactement `allows_hooks`
    /// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    pub verified: bool,
    /// Où en est la ligne `hooks` de cette entrée **après** ce résultat.
    ///
    /// Elle voyage avec lui parce que le test 4 peut la changer : une entrée qui attendait
    /// sa réponse laissait déjà écrire (`verifying` autorise les hooks), et un test 4 en
    /// échec la rend invalide. Sans cette ligne-là, la fenêtre garderait celle du premier
    /// temps — un bouton `install` allumé sur une entrée que le backend refuse désormais.
    ///
    /// `None` pour une saisie du formulaire d'ajout : elle n'est au registre, donc n'a pas
    /// de ligne `hooks` — le formulaire n'en montre aucune.
    pub hooks: Option<HooksReport>,
}

/// Ce que le second temps annonce à la fenêtre, ou rien du tout.
///
/// Deux règles, et elles sont ici — pures, donc éprouvées — plutôt que dans le fil de
/// [`follow_up`], qu'aucun test ne peut lancer sans une application Tauri :
///
/// 1. **un résultat que le registre a jeté ne s'annonce pas.** Il le jette quand l'entrée
///    ne décrit plus la même chose (spec §9.1, [`ToolRegistry::settle`]) ; l'émettre quand
///    même le ferait poser par la fenêtre sur l'entrée que le backend a refusé de toucher —
///    la règle de fraîcheur aurait un propriétaire, et une porte à côté.
/// 2. **une entrée du registre s'annonce entière** : sa vérification, son `verified` et sa
///    ligne `hooks`, tels que le registre vient de les reposer. Recomposer l'entrée champ
///    par champ côté fenêtre laisse vieillir ceux qu'on a oubliés.
fn announce(
    next: &SecondPass,
    verification: Verification,
    settled: Option<Vec<ToolDeclaration>>,
) -> Option<Verified> {
    if !next.stored {
        return Some(Verified {
            verified: verification.allows_hooks,
            command: next.command.clone(),
            verification,
            hooks: None,
        });
    }

    let tool = settled?
        .into_iter()
        .find(|tool| tool.command == next.command)?;
    Some(Verified {
        command: tool.command,
        verification,
        verified: tool.verified,
        hooks: Some(tool.hooks),
    })
}

/// Ce que la section `notifications` affiche (spec §8).
///
/// Sans registre : l'autorisation ne dépend d'aucune entrée déclarée, et les deux états qui
/// interrompent viennent d'`agents`. Elle est **relue à chaque appel**, et la fenêtre appelle
/// à chaque ouverture de la section : l'autorisation peut être changée dans les Réglages
/// Système pendant qu'Ash est ouvert, et une valeur retenue au démarrage vieillirait mal
/// dans le seul panneau où l'on vient justement la vérifier.
///
/// Le centre est **injecté**, et c'est celui qui pose les bannières : la fenêtre décrit donc
/// l'autorisation du mécanisme qui interrompt vraiment, et non celle d'un autre.
///
/// **`async` volontairement**, pour la raison exacte de [`super::super::git::commands`] :
/// Tauri exécute une commande synchrone sur le fil de l'interface, et celle-ci **attend** —
/// macOS répond à `getNotificationSettings` par un bloc, sur une file à lui, et le port
/// borne cette attente à deux secondes. Deux secondes mesurées à moins de dix millisecondes,
/// mais deux secondes de fenêtre figée le jour où elles arrivent. Le handle plutôt que
/// `tauri::State` : une commande `async` qui emprunte l'état est obligée de rendre un
/// `Result`, et une erreur qui ne peut pas se produire n'a pas sa place dans le contrat.
#[tauri::command]
pub async fn settings_notifications<R: Runtime>(app: AppHandle<R>) -> NotificationsReport {
    let banners = app.state::<Arc<dyn crate::features::notifications::Banners>>();
    notifications::report(notifications::observed(banners.authorization()))
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
        registry.forget(&Command::parse(&command)?)?,
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
    let changed = registry.retarget(&Command::parse(&command)?, &adapter, config.as_deref())?;
    answer(app, &registry, changed)
}

/// Relance la séquence sur une entrée — le bouton `re-verify` d'une carte.
#[tauri::command]
pub fn settings_verify_tool<R: Runtime>(
    app: AppHandle<R>,
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    command: String,
) -> Result<SettingsSnapshot, SettingsError> {
    let changed = registry.verify(&Command::parse(&command)?)?;
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
    let changed = registry.reset(&Command::parse(&command)?)?;
    answer(app, &registry, changed)
}

/// Annule la réinitialisation — le `undo the reset` de la bannière de doublon.
#[tauri::command]
pub fn settings_undo_reset<R: Runtime>(
    app: AppHandle<R>,
    registry: tauri::State<'_, Arc<ToolRegistry>>,
    command: String,
) -> Result<SettingsSnapshot, SettingsError> {
    let changed = registry.undo_reset(&Command::parse(&command)?)?;
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
        registry.install_hooks(&Command::parse(&command)?)?,
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
        registry.remove_hooks(&Command::parse(&command)?)?,
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
    let (shown, pending) = registry.verify_draft(&tool)?;
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
            // Un registre empoisonné n'a pas à faire paniquer un fil de fond, et il se lit
            // ici comme un résultat qui n'a pas été posé : rien n'est annoncé, et la fenêtre
            // garde ce que le premier temps lui a dit. C'est [`announce`] qui en décide.
            let settled = registry.settle(&next, verification.clone()).ok().flatten();
            if let Some(announcement) = announce(&next, verification, settled) {
                let _ = app.emit(SETTINGS_VERIFIED, announcement);
            }
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
    // Le même mot que la bande d'`app/settings.ts`, qui recouvre ce titre dès que la page
    // est peinte : c'est celui-là qu'on voit, mais celui-ci est ce que macOS met dans
    // Fenêtre → et dans Mission Control, et deux noms pour une fenêtre seraient un bug.
    //
    // `crate::APP_NAME` est lu directement plutôt que passé en paramètre parce que le nom
    // n'a **qu'une** source (voir sa documentation) et que la faire descendre jusqu'ici
    // demanderait de la porter dans le routage du menu, qui est le seul appelant. Une
    // constante de build lue depuis la racine n'est pas une feature qui en connaît une
    // autre. Si les réglages devaient un jour connaître autre chose de l'identité de
    // l'application, c'est par injection que ça passerait, pas par un second `crate::`.
    .title(format!("settings — {}", crate::APP_NAME))
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::features::settings::hooks::{HookAction, HookState};
    use crate::features::settings::ports::Launch;

    /// Test Data Builder : le second temps d'une entrée du registre.
    fn second_pass(command: &str, stored: bool) -> SecondPass {
        SecondPass {
            command: Command::parse(command).expect("un nom valide"),
            adapter: "claude-code".to_owned(),
            config: Some("/home/.claude".to_owned()),
            launch: Launch {
                program: PathBuf::from("/bin/claude"),
                args: vec!["--version".to_owned()],
                env: Vec::new(),
                timeout: Duration::from_secs(5),
            },
            stored,
        }
    }

    /// Une entrée telle que le registre vient de la reposer, avec sa ligne `hooks` éteinte.
    fn refused(command: &str) -> ToolDeclaration {
        let mut tool = NewTool {
            command: command.to_owned(),
            label: None,
            adapter: "claude-code".to_owned(),
            config: Some("/home/.claude".to_owned()),
        }
        .declare(&["claude-code".to_owned()], &[])
        .unwrap_or_else(|why| panic!("la saisie est valide : {why}"));
        tool.hooks = HooksReport {
            state: HookState::Blocked,
            summary: "unavailable until the path is verified".to_owned(),
            note: "the button stays where it is, dimmed.".to_owned(),
            file: None,
            action: HookAction::Install,
            enabled: false,
            choices: Vec::new(),
            diff: None,
            backup: None,
        };
        tool
    }

    #[test]
    fn given_a_second_pass_the_registry_dropped_as_stale_when_it_is_announced_then_nothing_is_emitted(
    ) {
        // Given — l'entrée ne décrit plus la même chose : le registre a jeté le résultat
        // sans bruit. L'annoncer quand même le ferait poser par la fenêtre sur l'entrée
        // que le backend a justement refusé de toucher
        let next = second_pass("claude", true);

        // When
        let announcement = announce(&next, Verification::unverified(), None);

        // Then
        assert!(announcement.is_none());
    }

    #[test]
    fn given_a_fourth_test_that_invalidated_an_entry_when_it_is_announced_then_the_hooks_line_travels_with_it(
    ) {
        // Given — pendant qu'elle attendait sa réponse, l'entrée laissait écrire
        // (`verifying` autorise les hooks). Annoncer la seule vérification laisserait à
        // l'écran le bouton `install` allumé du premier temps
        let next = second_pass("claude", true);

        // When
        let announcement = announce(
            &next,
            Verification::unverified(),
            Some(vec![refused("claude")]),
        );

        // Then
        let announced = announcement.expect("l'entrée est au registre");
        assert!(!announced.verified);
        let hooks = announced
            .hooks
            .expect("une entrée déclarée a une ligne hooks");
        assert_eq!(hooks.state, HookState::Blocked);
        assert!(!hooks.enabled);
    }

    #[test]
    fn given_a_draft_the_form_is_still_waiting_on_when_it_is_announced_then_it_is_emitted_without_a_hooks_line(
    ) {
        // Given — une saisie n'est pas au registre : elle n'a pas de ligne `hooks`, et le
        // formulaire n'en montre aucune. Se taire ferait attendre le test 4 pour toujours
        let next = second_pass("claude", false);

        // When
        let announcement = announce(&next, Verification::unverified(), None);

        // Then
        let announced = announcement.expect("le formulaire attend sa réponse");
        assert_eq!(announced.command.as_str(), "claude");
        assert!(announced.hooks.is_none());
    }
}
