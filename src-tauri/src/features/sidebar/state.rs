use std::path::Path;
use std::sync::{Arc, Mutex};

use super::persisted::Persisted;
use super::places::{PinnedWorktree, WorktreePlaces};
use super::store::SidebarStore;

/// Les lignes que la colonne garde d'une session à l'autre : celles qu'une épingle fait
/// exister — **situées**, donc relues —, et celles qui sont repliées.
///
/// Ce n'est pas ce que le disque garde ([`Persisted`]) : le disque garde des chemins, ceci
/// porte des worktrees relus. Deux formes plutôt qu'une, parce que ce sont deux choses — un
/// fichier qu'on veut minimal et durable, une fiche qu'on veut fraîche.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SidebarRows {
    /// Les épingles **encore trouvables**, dans l'ordre où elles ont été posées.
    pub pinned: Vec<PinnedWorktree>,
    pub collapsed: Vec<String>,
}

/// L'état de la colonne qui survit à la fermeture — **la** source de vérité.
///
/// Il vit ici, en Rust, et pas dans un `useState` de la webview : le frontend rend un état,
/// il ne le détient pas
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Pour une épingle, la
/// question ne se pose même pas — elle doit survivre à la fenêtre qui l'affiche.
pub struct SidebarState {
    kept: Mutex<Persisted>,
    store: Arc<dyn SidebarStore>,
    places: Arc<dyn WorktreePlaces>,
}

impl SidebarState {
    /// Repart de l'état de la session précédente, ou d'une colonne sans épingle.
    pub fn restore(store: Arc<dyn SidebarStore>, places: Arc<dyn WorktreePlaces>) -> Self {
        let kept = store.load().unwrap_or_default();
        Self {
            kept: Mutex::new(kept),
            store,
            places,
        }
    }

    /// Ce que la sidebar affiche : chaque épingle relue, et les lignes repliées.
    ///
    /// **Une épingle dont le dossier a disparu ne rend rien, et reste dans le fichier.** Un
    /// disque externe débranché, un worktree supprimé le temps d'un `git worktree prune`, un
    /// dépôt pas encore cloné sur cette machine : afficher une ligne qu'un clic ne saurait pas
    /// ouvrir tromperait, et retirer l'épingle serait effacer un geste de l'utilisateur pour
    /// une absence peut-être temporaire — or Ash **signale**, il ne supprime jamais (spec
    /// §5.4). Rebrancher le disque suffit à faire revenir la ligne.
    pub fn snapshot(&self) -> SidebarRows {
        let kept = self.locked().clone();
        SidebarRows {
            pinned: kept
                .pinned
                .iter()
                .filter_map(|root| self.places.place(Path::new(root)))
                .collect(),
            collapsed: kept.collapsed,
        }
    }

    /// Épingle ou désépingle un worktree. Rend `true` si quelque chose a changé.
    pub fn pin(&self, worktree_root: String, pinned: bool) -> bool {
        self.change(|kept| Persisted::toggle(&mut kept.pinned, worktree_root, pinned))
    }

    /// Replie ou déplie une ligne, par sa clé. Rend `true` si quelque chose a changé.
    pub fn collapse(&self, key: String, collapsed: bool) -> bool {
        self.change(|kept| Persisted::toggle(&mut kept.collapsed, key, collapsed))
    }

    /// Applique un changement et le garde sur le disque.
    ///
    /// L'écriture peut échouer — disque plein, `~/.ash` non inscriptible — et ça ne remet pas
    /// le changement en cause : la ligne s'épingle tout de suite, elle ne survivra simplement
    /// pas au redémarrage. C'est la conduite de `features::theme`, pour la même raison.
    fn change(&self, apply: impl FnOnce(&mut Persisted) -> bool) -> bool {
        let mut kept = self.locked();
        if !apply(&mut kept) {
            return false;
        }
        let _ = self.store.save(&kept);
        true
    }

    /// Un verrou empoisonné veut dire qu'un fil a paniqué **ailleurs** en le tenant. Ce qu'il
    /// protège est une liste de chemins : elle est intacte, et propager la panique éteindrait
    /// la fenêtre pour une épingle.
    fn locked(&self) -> std::sync::MutexGuard<'_, Persisted> {
        self.kept
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::error::SidebarError;
    use super::super::places::PinnedRepo;

    /// `~/.ash/state.json`, en mémoire.
    #[derive(Default)]
    struct FakeStore {
        content: Mutex<Option<Persisted>>,
        /// Un disque qui refuse d'écrire — plein, ou en lecture seule.
        read_only: bool,
    }

    impl SidebarStore for FakeStore {
        fn load(&self) -> Option<Persisted> {
            self.content.lock().unwrap().clone()
        }

        fn save(&self, state: &Persisted) -> Result<(), SidebarError> {
            if self.read_only {
                return Err(SidebarError::Io {
                    path: std::path::PathBuf::from("/dev/null/state.json"),
                    why: "lecture seule".to_owned(),
                });
            }
            *self.content.lock().unwrap() = Some(state.clone());
            Ok(())
        }
    }

    /// Le disque tel que la résolution le voit : les worktrees qui existent, et rien d'autre.
    #[derive(Default)]
    struct FakePlaces {
        known: Mutex<Vec<PinnedWorktree>>,
    }

    impl FakePlaces {
        /// Un worktree lié d'un dépôt — la forme à trois niveaux d'ADR-0012.
        fn holding(self, root: &str, name: &str, repo: &str) -> Self {
            self.known.lock().unwrap().push(PinnedWorktree {
                worktree_root: root.to_owned(),
                worktree_name: name.to_owned(),
                repo: Some(PinnedRepo {
                    id: format!("{repo}/.git"),
                    name: repo.to_owned(),
                }),
            });
            self
        }

        /// Le dossier disparaît du disque, sans que l'épingle bouge.
        fn losing(&self, root: &str) {
            self.known
                .lock()
                .unwrap()
                .retain(|known| known.worktree_root != root);
        }
    }

    impl WorktreePlaces for FakePlaces {
        fn place(&self, root: &Path) -> Option<PinnedWorktree> {
            let root = root.display().to_string();
            self.known
                .lock()
                .unwrap()
                .iter()
                .find(|known| known.worktree_root == root)
                .cloned()
        }
    }

    fn restored(store: Arc<FakeStore>, places: Arc<FakePlaces>) -> SidebarState {
        SidebarState::restore(store, places)
    }

    #[test]
    fn given_a_worktree_pinned_in_a_previous_session_when_ash_starts_again_then_the_row_is_there_without_a_tab(
    ) {
        // Given — le geste de la session d'hier, et un disque où le worktree existe toujours
        let store = Arc::new(FakeStore::default());
        let places =
            Arc::new(FakePlaces::default().holding("/wt/ash-sidebar", "ash-sidebar", "/dev/ash"));
        restored(Arc::clone(&store), Arc::clone(&places)).pin("/wt/ash-sidebar".to_owned(), true);

        // When — la session suivante, avant qu'aucun onglet ne soit ouvert
        let snapshot = restored(store, places).snapshot();

        // Then — la ligne existe sans onglet, et elle est située : la sidebar peut la ranger
        // sous son dépôt
        assert_eq!(
            snapshot.pinned,
            vec![PinnedWorktree {
                worktree_root: "/wt/ash-sidebar".to_owned(),
                worktree_name: "ash-sidebar".to_owned(),
                repo: Some(PinnedRepo {
                    id: "/dev/ash/.git".to_owned(),
                    name: "/dev/ash".to_owned(),
                }),
            }]
        );
    }

    #[test]
    fn given_a_pinned_worktree_when_it_is_unpinned_then_nothing_of_it_survives_the_next_start() {
        // Given
        let store = Arc::new(FakeStore::default());
        let places = Arc::new(FakePlaces::default().holding("/wt/ash-toc", "ash-toc", "/dev/ash"));
        let state = restored(Arc::clone(&store), Arc::clone(&places));
        state.pin("/wt/ash-toc".to_owned(), true);

        // When
        let changed = state.pin("/wt/ash-toc".to_owned(), false);

        // Then — le geste a changé quelque chose, et la session suivante n'en garde rien
        assert!(changed);
        assert!(restored(store, places).snapshot().pinned.is_empty());
    }

    #[test]
    fn given_a_pinned_worktree_whose_folder_disappeared_when_the_column_is_drawn_then_the_row_is_gone_but_the_pin_is_not(
    ) {
        // Given — un disque externe débranché, ou un `git worktree remove` fait dans un
        // terminal. Le geste de l'utilisateur, lui, n'a pas été défait.
        let store = Arc::new(FakeStore::default());
        let places = Arc::new(FakePlaces::default().holding("/wt/ash-toc", "ash-toc", "/dev/ash"));
        let state = restored(Arc::clone(&store), Arc::clone(&places));
        state.pin("/wt/ash-toc".to_owned(), true);
        places.losing("/wt/ash-toc");

        // When
        let while_gone = state.snapshot();

        // Then — rien à afficher : un clic sur une ligne fantôme lancerait un shell dans un
        // dossier qui n'existe pas
        assert!(while_gone.pinned.is_empty());

        // Then — l'épingle est intacte, et rebrancher le disque suffit à la faire revenir
        let places = Arc::new(FakePlaces::default().holding("/wt/ash-toc", "ash-toc", "/dev/ash"));
        assert_eq!(
            restored(store, places)
                .snapshot()
                .pinned
                .iter()
                .map(|pinned| pinned.worktree_root.clone())
                .collect::<Vec<_>>(),
            vec!["/wt/ash-toc".to_owned()]
        );
    }

    #[test]
    fn given_a_collapsed_repository_row_when_ash_starts_again_then_it_is_still_collapsed() {
        // Given — spec §5.2 : les épingles **et leur état replié** survivent au redémarrage
        let store = Arc::new(FakeStore::default());
        let places = Arc::new(FakePlaces::default());
        let state = restored(Arc::clone(&store), Arc::clone(&places));
        state.collapse("repo:/dev/ash/.git".to_owned(), true);

        // When
        let next = restored(store, places).snapshot();

        // Then
        assert_eq!(next.collapsed, vec!["repo:/dev/ash/.git".to_owned()]);
    }

    #[test]
    fn given_a_read_only_disk_when_a_worktree_is_pinned_then_the_row_still_appears_in_this_session()
    {
        // Given — `~/.ash` non inscriptible : l'utilisateur ne doit pas s'en apercevoir en
        // voyant son geste refusé
        let store = Arc::new(FakeStore {
            content: Mutex::new(None),
            read_only: true,
        });
        let places = Arc::new(FakePlaces::default().holding("/dev/ash", "ash", "/dev/ash"));
        let state = restored(store, Arc::clone(&places));

        // When
        let changed = state.pin("/dev/ash".to_owned(), true);

        // Then — la ligne est là ; elle ne survivra simplement pas au redémarrage
        assert!(changed);
        assert_eq!(state.snapshot().pinned.len(), 1);
    }
}
