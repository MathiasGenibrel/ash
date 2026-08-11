//! L'invocation de `git`, derrière un trait que la feature possède.
//!
//! C'est le seul endroit du dépôt où le binaire `git` est lancé en production, et il n'y
//! en aura pas d'autre. La règle qui l'encadre est celle du critère d'acceptation de
//! l'issue #8 : **jamais dans la boucle de sonde**. L'appel part des trois mêmes moments
//! que le reste des métadonnées — rattachement, focus, écriture surveillée — et passe par
//! la même limitation à un rafraîchissement par worktree et par tranche de 5 s.
//!
//! Pourquoi un appel plutôt qu'une lecture de fichiers : l'état de l'arbre (`+3 ~1`) est
//! la comparaison de l'index avec l'arbre de travail, et l'avance sur l'amont (`↑2 ↓1`)
//! est un parcours du graphe de commits. Ni l'un ni l'autre ne se lit dans `.git` sans
//! réimplémenter une bibliothèque git complète.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Délai au-delà duquel on renonce à l'état de l'arbre.
///
/// Cinq secondes, comme la fenêtre de limitation : au-delà, un nouveau rafraîchissement
/// peut de toute façon être demandé, et un `git` par worktree suffit largement à
/// encombrer une machine. Un dépôt trop gros pour répondre dans ce délai rend une ligne
/// de statut **sans** état d'arbre — mais avec sa branche, qui vient des fichiers.
pub const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// L'état d'un arbre de travail, tel que `git` sait seul le dire.
///
/// Rend la sortie **brute** : l'interprétation est une règle pure, et elle vit dans
/// [`super::porcelain`]. `None` couvre tout ce qui peut mal se passer — `git` absent du
/// `PATH`, dépôt trop gros pour le délai, sortie en erreur — parce que l'appelant en fait
/// la même chose : il affiche la branche sans l'état de l'arbre. Ce n'est pas une panne,
/// c'est un cas nominal.
pub trait StatusReader: Send + Sync {
    fn read(&self, worktree_root: &Path) -> Option<String>;
}

/// L'appel réel : un processus `git`, dans le worktree, sans shell.
#[derive(Debug, Clone, Copy)]
pub struct SystemGit {
    timeout: Duration,
}

impl Default for SystemGit {
    fn default() -> Self {
        Self {
            timeout: STATUS_TIMEOUT,
        }
    }
}

impl StatusReader for SystemGit {
    fn read(&self, worktree_root: &Path) -> Option<String> {
        // `Command` prend le programme et ses arguments séparément : aucun shell n'est
        // lancé, donc aucun chemin de worktree ne peut être interprété comme du code.
        // Le répertoire de travail est **explicite** — un `git` lancé depuis le
        // répertoire courant du processus décrirait un autre dépôt que celui demandé.
        let mut child = Command::new("git")
            .current_dir(worktree_root)
            .args([
                // Un lecteur de fond n'a pas à réécrire l'index de l'utilisateur pour
                // rafraîchir des dates : sans ça, chaque appel écrirait dans `.git`.
                "--no-optional-locks",
                // Les chemins de contrôle sont échappés, jamais rendus tels quels : une
                // ligne du résultat reste une ligne, même pour un fichier au nom exotique.
                "-c",
                "core.quotePath=true",
                "status",
                // Format documenté et stable, contrairement à `--short`.
                "--porcelain=v2",
                // L'en-tête `# branch.ab +2 -1` : l'avance et le retard, dans le même appel.
                "--branch",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let mut output = child.stdout.take()?;
        // La lecture se fait dans un fil à part pour que le délai soit tenu même si `git`
        // ne rend jamais la main : `wait_timeout` n'existe pas dans la bibliothèque
        // standard, et un `read` bloquant ne s'interrompt pas.
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut text = String::new();
            let read = output.read_to_string(&mut text);
            let _ = sender.send(read.ok().map(|_| text));
        });

        match receiver.recv_timeout(self.timeout) {
            Ok(text) => {
                // Le code de sortie compte : `git status` hors d'un dépôt sort en 128 et
                // n'écrit rien sur la sortie standard.
                let succeeded = child.wait().map(|status| status.success()).unwrap_or(false);
                succeeded.then_some(text).flatten()
            }
            Err(_) => {
                // Le dépôt est trop gros, ou `git` est bloqué sur un verrou. On le tue :
                // un processus abandonné par worktree, toutes les cinq secondes, finirait
                // par se voir.
                let _ = child.kill();
                let _ = child.wait();
                None
            }
        }
    }
}
