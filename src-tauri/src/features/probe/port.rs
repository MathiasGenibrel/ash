use std::os::fd::RawFd;
use std::path::{Path, PathBuf};

use super::error::ProbeError;

/// Un identifiant de processus, tel que le système le donne.
pub type Pid = i32;

/// Ce que le système sait dire d'un processus.
///
/// Le `cwd` **et** l'identité sont rendus ensemble parce qu'ils viennent de la même
/// passe de sonde : ADR-0005 et [ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)
/// se partagent un seul mécanisme, l'un pour rattacher l'onglet à un worktree, l'autre
/// pour reconnaître un agent à son nom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: Pid,
    /// Dernier segment du chemin de l'exécutable — `claude`, `bash`, `less`.
    pub name: String,
    pub cwd: PathBuf,
}

/// L'effet système, derrière un trait que la feature possède.
///
/// Deux raisons, et pas une seule : les règles de la sonde — repli, dédoublonnage,
/// courses — se vérifient sans lancer le moindre processus ; et un portage Linux, qui
/// lirait `/proc/<pid>/cwd` au lieu de `proc_pidinfo`, n'aurait que ce trait à
/// réimplémenter ([ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md)).
pub trait Probe: Send + Sync {
    /// Le groupe de processus qui tient l'avant-plan du terminal — `tcgetpgrp`.
    ///
    /// Le pgid rendu est aussi le pid de son chef de groupe : c'est lui qu'on inspecte
    /// ensuite.
    fn foreground_pgid(&self, terminal: RawFd) -> Result<Pid, ProbeError>;

    /// Le `cwd` et l'identité d'un processus — `proc_pidinfo`.
    fn inspect(&self, pid: Pid) -> Result<ProcessInfo, ProbeError>;
}

/// Le nom d'un processus : le dernier segment du chemin de son exécutable.
///
/// C'est cette chaîne que la découverte d'agents comparera aux commandes reconnues
/// (ADR-0006). Un chemin sans dernier segment — le noyau peut rendre une chaîne vide
/// pour un processus qu'on n'a pas le droit de lire — ne doit pas produire un nom vide
/// qui vaudrait pour n'importe quoi : on rend le chemin tel quel, quitte à ne
/// correspondre à rien.
pub(super) fn process_name(executable: &Path) -> String {
    executable
        .file_name()
        .map_or_else(
            || executable.as_os_str().to_string_lossy(),
            |name| name.to_string_lossy(),
        )
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_an_executable_path_when_naming_the_process_then_only_its_last_segment_is_kept() {
        // Given
        let executable = Path::new("/opt/homebrew/bin/claude");

        // When
        let name = process_name(executable);

        // Then — c'est « claude » que la configuration d'ADR-0006 déclarera, pas un chemin
        assert_eq!(name, "claude");
    }

    #[test]
    fn given_a_path_without_a_last_segment_when_naming_the_process_then_it_does_not_become_empty() {
        // Given — un nom vide correspondrait à toutes les commandes reconnues
        let executable = Path::new("/");

        // When
        let name = process_name(executable);

        // Then
        assert_eq!(name, "/");
    }
}
