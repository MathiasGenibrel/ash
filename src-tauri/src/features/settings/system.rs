//! Les deux ports, branchés sur la vraie machine.
//!
//! Le seul fichier de la feature qui touche le disque et le seul qui crée un processus.
//! Il ne porte **aucune** règle : ce qu'on lit, ce qu'on lance et ce qu'on en conclut est
//! décidé par [`super::verification`], qui ne connaît que les traits.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::mpsc;

use super::ports::{Answer, CommandRunner, ConfigFiles, Folder, Launch};
use super::values::Command;

/// Au-delà, on cesse de lire la sortie de la commande.
///
/// Personne n'analyse ce texte : il sert à répéter à l'utilisateur ce que la commande a
/// répondu quand elle a mal répondu. Une commande qui déverse un mégaoctet n'a rien de plus
/// à dire qu'une qui en écrit deux mille — et ce mégaoctet finirait dans un `String`
/// traversant l'IPC.
const MAX_OUTPUT: usize = 2048;

/// Le système de fichiers réel.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemConfigFiles;

impl ConfigFiles for SystemConfigFiles {
    fn read_folder(&self, path: &Path) -> Folder {
        match std::fs::read_dir(path) {
            Ok(entries) => Folder::Readable(
                entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect(),
            ),
            Err(why) => match why.kind() {
                std::io::ErrorKind::NotFound => Folder::Missing,
                std::io::ErrorKind::PermissionDenied => Folder::Unreadable,
                // `NotADirectory` n'est pas stable dans cette version de Rust : la question
                // se repose au système, qui sait déjà répondre.
                _ if path.is_file() => Folder::NotADirectory,
                _ => Folder::Unreadable,
            },
        }
    }

    fn home(&self) -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Le lancement réel — **la seule création de processus de la feature**.
///
/// Six contraintes tiennent la frontière décrite par [`Launch`], et elles sont ici parce
/// que c'est ici qu'elles s'exercent :
///
/// 1. **Aucun shell.** [`Process`](std::process::Command) prend le programme et ses
///    arguments séparément : ni le chemin de configuration ni le nom de commande ne peuvent
///    être relus comme du code.
/// 2. **Le programme est celui que le `PATH` a résolu** ([`Self::locate`]), jamais un
///    chemin recomposé à partir d'une saisie. Ash ne lance donc que ce que taper le nom
///    dans un shell aurait lancé — et [`Command`] garantit dès la signature que ce nom n'est
///    ni un chemin, ni une ligne de commande, ni vide.
/// 3. **L'environnement est remplacé, pas complété** (`env_clear`). Ce qui traîne dans
///    celui d'Ash — jusqu'à `ASH_SOCK`, qu'un hook interpréterait — ne fuit pas dans un
///    programme qu'on lance pour lui poser une question.
/// 4. **Le répertoire courant est neutre.** Lancer depuis le dossier vérifié laisserait un
///    outil lire ce qu'il y trouve ; lancer depuis celui d'Ash n'aurait aucun sens.
/// 5. **L'entrée standard est fermée** : rien ne peut attendre une réponse de personne.
/// 6. **La sortie est bornée et le délai tenu**, sans quoi une commande bavarde ou muette
///    tiendrait un fil pour toujours.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommands;

impl CommandRunner for SystemCommands {
    fn locate(&self, command: &Command) -> Option<PathBuf> {
        // Aucune revérification ici, et ce n'est pas un oubli : ce fichier est le dernier
        // avant le processus, et un chemin absolu déguisé en nom de commande ferait sortir
        // la résolution du `PATH`. La garde est passée du corps à la signature — un
        // [`Command`] ne se fabrique qu'en validant, donc rien ne peut atteindre cette ligne
        // sans l'avoir passée.
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join(command.as_str()))
            .find(|candidate| is_executable_file(candidate))
    }

    fn run(&self, launch: &Launch) -> Result<Answer, String> {
        let mut child = Process::new(&launch.program)
            .args(&launch.args)
            .env_clear()
            .envs(launch.env.iter().map(|(n, v)| (n.as_str(), v.as_str())))
            .current_dir(Path::new("/"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|why| why.to_string())?;

        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            return Err("no output to read".to_owned());
        };

        // La lecture part sur un fil pour que le délai soit tenu même si la commande ne
        // rend jamais la main — même raison que le `git status` de `features/git`.
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stdout
                .by_ref()
                .take(MAX_OUTPUT as u64)
                .read_to_string(&mut text);
            let _ = sender.send(text);
        });

        match receiver.recv_timeout(launch.timeout) {
            Ok(output) => {
                let succeeded = child.wait().map(|status| status.success()).unwrap_or(false);
                Ok(Answer { succeeded, output })
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(format!("no answer in {}s", launch.timeout.as_secs()))
            }
        }
    }
}

/// Un fichier que le système accepterait d'exécuter.
///
/// Le bit d'exécution est vérifié parce que sans lui, un `PATH` qui contient un dossier de
/// données ferait « trouver » un fichier homonyme que rien ne pourrait lancer — et le test
/// 3 dirait alors « la commande existe » pour un fichier texte.
fn is_executable_file(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(found) => found.is_file() && found.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_folder_that_does_not_exist_when_it_is_read_then_it_is_missing_and_not_unreadable() {
        // Given — les deux mènent à des corrections différentes : un dossier absent se
        // remplace, un dossier verrouillé se déverrouille
        let files = SystemConfigFiles;

        // When
        let found = files.read_folder(Path::new("/ash-does-not-exist-here/config"));

        // Then
        assert_eq!(found, Folder::Missing);
    }
}
