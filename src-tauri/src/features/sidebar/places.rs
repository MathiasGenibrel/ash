use std::path::Path;

/// Le dépôt commun sous lequel une ligne épinglée se range.
///
/// Même forme que le `RepoRef` de `features::pty`, et **pas** le même type : une feature
/// n'importe pas l'intérieur d'une autre, et un onglet et une épingle n'ont aucune raison de
/// partager une structure parce qu'elles se ressemblent aujourd'hui. Le côté TypeScript, lui,
/// n'écrit qu'une interface et vérifie qu'elle reflète les deux (`shared/ipc/mirror.ts`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PinnedRepo {
    /// Le dossier git commun : c'est par lui que la sidebar groupe, jamais par le nom.
    pub id: String,
    pub name: String,
}

/// Un worktree épinglé, tel qu'il traverse la frontière — donc **tel qu'il est aujourd'hui**,
/// et non tel que le fichier l'a gardé.
///
/// Le disque ne garde qu'une racine (`Persisted`) ; le nom et le dépôt sont relus à
/// chaque lecture par [`WorktreePlaces`]. Une ligne épinglée dit donc la vérité après un
/// `git worktree add` dans le même dépôt, après un renommage de dossier, et après un
/// déplacement — ce qu'une fiche recopiée dans `state.json` ne saurait pas faire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PinnedWorktree {
    /// La racine du worktree — **la** clé, la même que celle des onglets.
    pub worktree_root: String,
    /// Le nom brut du dossier, comme pour un onglet : la matière du suffixe `·sidebar`.
    pub worktree_name: String,
    /// `None` **est** la forme à plat d'ADR-0012, exactement comme pour un onglet.
    pub repo: Option<PinnedRepo>,
}

/// Où se trouve un worktree épinglé, aujourd'hui.
///
/// Le port que la feature possède : `sidebar` ne connaît ni git, ni le système de
/// fichiers, et `git` ne sait rien des épingles. C'est le composition root qui les relie,
/// exactement comme il relie la résolution des onglets.
///
/// `None` veut dire « ce chemin ne désigne plus un worktree qu'on sache situer » — le dossier
/// a été supprimé, le disque externe est débranché, le dépôt a disparu. Ce que la feature en
/// fait est décidé dans [`super::state`], et ce n'est pas d'oublier l'épingle.
pub trait WorktreePlaces: Send + Sync {
    fn place(&self, root: &Path) -> Option<PinnedWorktree>;
}
