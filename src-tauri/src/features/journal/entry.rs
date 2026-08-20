//! Ce qu'une ligne du journal dit, et sous quel nom de fichier elle se range.
//!
//! Le format est **une ligne de JSON par commit observé**, dans un fichier par dépôt
//! ([ADR-0014](../../../../docs/adr/0014-attribution-locale-des-commits.md), spec §9.2).
//! Deux propriétés le gouvernent, et elles ne sont pas négociables au fil d'une tâche :
//!
//! - **append-only** : on n'édite jamais une ligne écrite. Une ligne est un fait observé à
//!   un instant, et un fait observé ne se corrige pas ;
//! - **inspectable et supprimable à la main** : c'est ce que l'ADR retient contre SQLite,
//!   et c'est ce qui rend crédible la promesse de la spec §10 pour un fichier qui contient
//!   des prompts.

use crate::shared::time::UnixMillis;

/// Une ligne du journal : un commit, et l'agent qu'Ash a vu l'écrire.
///
/// Les huit champs sont ceux d'ADR-0014, dans son ordre. **Les noms sont en `snake_case`**,
/// et c'est le seul endroit du crate où une forme sérialisée ne suit pas la convention de la
/// frontière Tauri : ce fichier n'est pas un contrat avec le frontend, c'en est un avec
/// l'utilisateur qui l'ouvrira dans un éditeur — et avec l'ADR, qui nomme les champs ainsi.
///
/// Deux champs sont facultatifs, et c'est **la même absence** : `session_started` et
/// `prompt` n'ont aujourd'hui aucune source. Voir la documentation du module `mod.rs` — ils
/// ne viendraient ni de la sonde ni du `cwd`, mais du flux de hooks, que l'ADR range
/// explicitement hors des dépendances de l'attribution.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// Le dépôt, désigné par son dossier git **commun** — la même clé que celle par
    /// laquelle la sidebar groupe les worktrees d'un même projet (`RepoRef.id`).
    pub repo: String,
    pub sha: String,
    /// La date d'auteur, **telle que git l'écrit** (ISO 8601 strict). Moitié de la clé de
    /// repli : git la préserve à travers un rebase, un amend et un cherry-pick.
    pub author_date: String,
    /// La première ligne du message. L'autre moitié de la clé de repli.
    pub subject: String,
    /// L'outil reconnu dans l'onglet au moment où le commit est né — `claude`, `codex`.
    pub agent: String,
    /// L'onglet où il tournait. Ce n'est pas décoratif : c'est ce qui permettra de
    /// retrouver la conversation, le jour où le prompt aura une source.
    pub tab_id: String,
    #[serde(default)]
    pub session_started: Option<UnixMillis>,
    #[serde(default)]
    pub prompt: Option<String>,
}

impl Entry {
    /// La ligne telle qu'elle part sur le disque, retour à la ligne compris.
    ///
    /// Un objet qui ne se sérialise pas rend une ligne vide plutôt que de paniquer : rien
    /// dans ce fichier ne vaut d'interrompre l'application qui l'écrit, et une ligne vide se
    /// relit comme une ligne qu'on jette.
    pub fn line(&self) -> String {
        match serde_json::to_string(self) {
            Ok(json) => format!("{json}\n"),
            Err(_) => String::new(),
        }
    }

    /// Les entrées que porte un fichier — les lignes illisibles sont **jetées**.
    ///
    /// Le fichier est fait pour être ouvert et modifié à la main (spec §10). Une ligne
    /// tronquée par un arrêt brutal, ou une ligne qu'un éditeur a coupée, ne doit pas
    /// emporter l'attribution de tout un dépôt.
    pub fn read_all(content: &str) -> Vec<Entry> {
        content
            .lines()
            .filter_map(|line| serde_json::from_str::<Entry>(line).ok())
            .collect()
    }
}

/// Le nom du fichier d'un dépôt, sous `~/.ash/journal/`.
///
/// Deux exigences qui se contredisent, et la façon dont elles se concilient : le nom doit
/// être **lisible** — la spec §10 promet un dossier qu'on inspecte à l'œil nu — et il doit
/// être **unique**, parce que deux dépôts homonymes existent sur une même machine (c'est
/// exactement pourquoi la sidebar groupe par dossier git commun et non par nom). D'où le
/// nom du projet, suivi d'une empreinte du chemin complet.
///
/// L'empreinte est FNV-1a 64 bits, écrite ici en quatre lignes : ce n'est pas une empreinte
/// cryptographique et elle n'a pas à l'être — elle sépare des chemins, elle ne protège rien.
pub fn file_name(repo: &str) -> String {
    format!("{}-{:016x}.jsonl", readable(repo), fingerprint(repo))
}

/// Le nom du projet tel qu'un humain le reconnaîtra, réduit à ce qu'un nom de fichier
/// accepte.
fn readable(repo: &str) -> String {
    let trimmed = repo.trim_end_matches('/');
    // `/dev/ash/.git` est un dépôt classique ; `/dev/ash/.git/worktrees/x` n'arrive jamais
    // ici — le dossier **commun** est ce qui identifie le dépôt, et il s'arrête à `.git`.
    let without_git = trimmed.strip_suffix("/.git").unwrap_or(trimmed);
    let name: String = without_git
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("repo")
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() || letter == '-' || letter == '_' || letter == '.' {
                letter.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    if name.is_empty() || name.starts_with('.') {
        format!("repo{name}")
    } else {
        name
    }
}

fn fingerprint(repo: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in repo.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::journal::fakes::EntryBuilder;

    #[test]
    fn given_an_observed_commit_when_it_is_written_then_the_line_carries_the_eight_fields_of_the_adr(
    ) {
        // Given — la forme de la ligne est un contrat avec la version d'Ash de demain, et
        // avec l'utilisateur qui ouvrira le fichier. Ce test tombe le jour où un champ
        // change de nom, disparaît, ou en amène un neuvième sans qu'on l'ait décidé.
        let entry = EntryBuilder::new().build();

        // When
        let line = entry.line();

        // Then
        assert!(
            line.ends_with('\n'),
            "une ligne par commit, et rien d'autre"
        );
        let parsed: serde_json::Value =
            serde_json::from_str(line.trim_end()).expect("la ligne écrite est du JSON");
        let object = parsed.as_object().expect("la ligne écrite est un objet");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "agent",
                "author_date",
                "prompt",
                "repo",
                "session_started",
                "sha",
                "subject",
                "tab_id",
            ]
        );
    }

    #[test]
    fn given_a_journal_file_with_a_broken_line_when_it_is_read_back_then_the_other_entries_survive()
    {
        // Given — le fichier est fait pour être ouvert à la main (spec §10), et un arrêt
        // brutal peut le tronquer. Perdre l'attribution de tout un dépôt pour une ligne
        // abîmée serait la pire réponse possible dans un fichier append-only.
        let written = format!(
            "{}{{ tronqué\n{}",
            EntryBuilder::new().sha("aaa").build().line(),
            EntryBuilder::new().sha("bbb").build().line()
        );

        // When
        let entries = Entry::read_all(&written);

        // Then
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].sha, "aaa");
        assert_eq!(entries[1].sha, "bbb");
    }

    #[test]
    fn given_two_repositories_with_the_same_name_when_their_files_are_named_then_they_do_not_collide(
    ) {
        // Given — deux clones du même projet, à deux endroits du disque. C'est le cas que la
        // sidebar traite déjà en groupant par dossier git commun plutôt que par nom ; le
        // journal ne peut pas se permettre de le traiter autrement, sous peine de mêler
        // l'historique de deux dépôts.
        let here = "/dev/ash/.git";
        let there = "/Users/mathias/archive/ash/.git";

        // When
        let (one, other) = (file_name(here), file_name(there));

        // Then — le nom reste lisible, et il reste unique
        assert!(one.starts_with("ash-"), "{one}");
        assert!(other.starts_with("ash-"), "{other}");
        assert_ne!(one, other);
        assert_eq!(one, file_name(here), "le même dépôt garde le même fichier");
    }

    #[test]
    fn given_a_repository_whose_folder_name_is_exotic_when_its_file_is_named_then_the_name_stays_a_file_name(
    ) {
        // Given — un nom de dossier peut porter un `/` échappé, un espace, un `..`. Le
        // fichier du journal se pose dans `~/.ash/journal/` et nulle part ailleurs.
        let repo = "/dev/../mon projet (2)/.git";

        // When
        let name = file_name(repo);

        // Then
        assert!(!name.contains('/'), "{name}");
        assert!(!name.contains(' '), "{name}");
        assert!(name.ends_with(".jsonl"), "{name}");
    }
}
