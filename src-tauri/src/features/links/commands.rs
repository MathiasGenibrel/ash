//! La surface de la feature vers le frontend : **une question, une ouverture**.
//!
//! ## Le frontend demande, il ne décide pas
//!
//! [`links_openable`] est une **question**, et [`links_open`] refait le travail depuis le
//! début. C'est délibéré, et c'est la propriété la plus importante de ce fichier : ce qui a
//! été souligné à l'écran n'autorise rien. Le frontend renvoie le mot, pas un jeton, pas un
//! indice, pas un chemin déjà résolu — donc il n'y a aucun état partagé entre l'instant du
//! survol et celui du clic, et rien de ce qu'un rendu aurait pu retenir n'atteint
//! `opener.rs`. Un fichier effacé entre les deux instants n'est plus ouvrable, et c'est la
//! bonne réponse.
//!
//! ## Pourquoi une question **par lot**
//!
//! Le survol interroge une ligne entière d'un coup. Un aller-retour par mot ferait une
//! dizaine d'appels pour un seul mouvement de souris, et le critère d'acceptation de
//! l'issue #126 est que la vérification **ne bloque jamais le rendu** : elle est
//! asynchrone côté écran, et un candidat pas encore vérifié y reste du texte.
//!
//! ## Le `cwd` vient du frontend, et pourquoi c'est acceptable
//!
//! Il vient de `TabInfo.cwd`, que la sonde d'
//! [ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md) tient à jour à travers les
//! `cd` — c'est la valeur propre de la fonctionnalité : Ash résout un chemin **relatif**
//! sans se tromper là où un terminal ordinaire ne le peut pas. Le faire passer par le
//! frontend plutôt que d'aller le rechercher dans `features/pty` évite de coupler deux
//! features pour une donnée qui traverse déjà la frontière à chaque `ash://tab-changed`.
//!
//! Ce que ça ne concède pas : un `cwd` inventé ne fait qu'**une** chose, désigner un autre
//! dossier de départ — il ne peut ni changer la liste blanche des schémas, ni faire exister
//! un fichier, ni faire exécuter quoi que ce soit. La décision reste entière dans
//! `target.rs`, qui ne prend le `cwd` que comme point de départ et refuse même de s'en
//! servir s'il n'est pas absolu.

use std::path::Path;
use std::sync::Arc;

use super::error::LinkError;
use super::files::Files;
use super::opener::Opener;
use super::target::resolve;

/// Combien de candidats une question peut porter.
///
/// Une ligne de terminal très large produit au plus quelques dizaines de mots ; au-delà,
/// c'est une sortie qui essaie de faire travailler le système de fichiers. Le frontend
/// borne déjà de son côté — cette borne-ci est celle qui compte, parce qu'elle ne dépend
/// pas de lui.
const MOST_CANDIDATES: usize = 128;

/// Le dossier personnel, lu à chaque question plutôt que retenu.
///
/// Une lecture d'environnement coûte moins qu'un `stat`, et il y en a un par candidat juste
/// après. La retenir demanderait un état de plus au composition root pour rien.
fn home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

/// Ceux des candidats qu'Ash accepterait d'ouvrir — l'ordre d'entrée n'est pas conservé,
/// et le frontend n'en a pas besoin : il en fait un ensemble.
fn openable(
    candidates: &[String],
    cwd: &Path,
    home: Option<&Path>,
    files: &dyn Files,
) -> Vec<String> {
    candidates
        .iter()
        .take(MOST_CANDIDATES)
        .filter(|candidate| resolve(candidate, cwd, home, files).is_some())
        .cloned()
        .collect()
}

/// Lesquels de ces mots sont des liens — la question que pose le survol sous `Cmd`.
///
/// Ne rend **rien d'autre** que les mots eux-mêmes : ni le chemin résolu, ni le genre de
/// lien. Le frontend n'a besoin que de savoir lesquels souligner, et lui rendre un chemin
/// absolu lui donnerait une valeur qu'il serait tenté de renvoyer plus tard.
#[tauri::command]
pub fn links_openable(
    files: tauri::State<'_, Arc<dyn Files>>,
    cwd: String,
    candidates: Vec<String>,
) -> Vec<String> {
    openable(
        &candidates,
        Path::new(&cwd),
        home().as_deref(),
        files.inner().as_ref(),
    )
}

/// Décide, puis ouvre — jamais l'inverse, et jamais sur la foi d'une décision passée.
fn open_candidate(
    candidate: &str,
    cwd: &Path,
    home: Option<&Path>,
    files: &dyn Files,
    opener: &dyn Opener,
) -> Result<(), LinkError> {
    let target = resolve(candidate, cwd, home, files).ok_or(LinkError::Unopenable)?;
    opener.open(&target)
}

/// Ouvre un lien — après l'avoir décidé de nouveau. Voir l'en-tête.
#[tauri::command]
pub fn links_open(
    files: tauri::State<'_, Arc<dyn Files>>,
    opener: tauri::State<'_, Arc<dyn Opener>>,
    cwd: String,
    candidate: String,
) -> Result<(), LinkError> {
    open_candidate(
        &candidate,
        Path::new(&cwd),
        home().as_deref(),
        files.inner().as_ref(),
        opener.inner().as_ref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::links::files::FakeFiles;
    use crate::features::links::opener::FakeOpener;

    const CWD: &str = "/dev/ash";
    const HOME: &str = "/Users/moi";

    fn words(candidates: &[&str]) -> Vec<String> {
        candidates.iter().map(|it| (*it).to_owned()).collect()
    }

    #[test]
    fn given_a_line_of_words_when_asking_which_are_links_then_only_the_url_and_the_existing_path_come_back(
    ) {
        // Given
        let files = FakeFiles::with(["/dev/ash/src/main.rs"]);
        let candidates = words(&[
            "https://example.com",
            "src/main.rs",
            "src/gone.rs",
            "javascript:alert(1)",
        ]);
        // When
        let openable = openable(&candidates, Path::new(CWD), Some(Path::new(HOME)), &files);
        // Then
        assert_eq!(openable, vec!["https://example.com", "src/main.rs"]);
    }

    #[test]
    fn given_a_hostile_line_of_thousands_of_words_when_asking_then_the_disk_is_asked_a_bounded_number_of_times(
    ) {
        // Given
        let files = FakeFiles::with([]);
        let painted: Vec<String> = (0..5_000).map(|index| format!("f{index}/x")).collect();
        // When
        let openable = openable(&painted, Path::new(CWD), Some(Path::new(HOME)), &files);
        // Then
        assert!(openable.is_empty());
        assert_eq!(files.asked(), MOST_CANDIDATES);
    }

    fn clicking(candidate: &str, files: &FakeFiles, opener: &FakeOpener) -> Result<(), LinkError> {
        open_candidate(
            candidate,
            Path::new(CWD),
            Some(Path::new(HOME)),
            files,
            opener,
        )
    }

    #[test]
    fn given_a_path_that_disappeared_between_the_hover_and_the_click_when_clicking_then_nothing_is_opened(
    ) {
        // Given — la vérification du survol avait dit oui, le disque a changé depuis
        let gone = FakeFiles::with([]);
        let opener = FakeOpener::new();
        // When
        let clicked = clicking("src/main.rs", &gone, &opener);
        // Then
        assert_eq!(clicked, Err(LinkError::Unopenable));
        assert!(opener.opened().is_empty());
    }

    /// Le critère d'acceptation de l'issue #126 : « un `javascript:` … n'est **jamais**
    /// ouvert, même écrit tel quel dans la sortie ; un test le prouve ». Celui-ci le prouve
    /// sur le chemin complet du clic, et pas seulement sur la décision.
    #[test]
    fn given_a_scheme_outside_the_whitelist_when_clicking_it_then_launch_services_is_never_reached()
    {
        // Given — un fichier de ce nom existe même, pour fermer le repli par le chemin
        let files = FakeFiles::with(["/dev/ash/javascript:alert(1)", "/etc/passwd"]);
        let opener = FakeOpener::new();
        for hostile in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>",
            "vbscript:msgbox(1)",
        ] {
            // When
            let clicked = clicking(hostile, &files, &opener);
            // Then
            assert_eq!(clicked, Err(LinkError::Unopenable), "{hostile}");
        }
        assert!(opener.opened().is_empty());
    }

    #[test]
    fn given_an_existing_relative_path_when_clicking_it_then_it_is_revealed_from_the_tab_cwd() {
        // Given
        let files = FakeFiles::with(["/dev/ash/src/features/terminal/index.ts"]);
        let opener = FakeOpener::new();
        // When
        let clicked = clicking("src/features/terminal/index.ts", &files, &opener);
        // Then
        assert_eq!(clicked, Ok(()));
        assert_eq!(opener.opened().len(), 1);
    }
}
