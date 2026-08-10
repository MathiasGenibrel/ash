//! La sonde réelle : `tcgetpgrp` puis `libproc`, comme le décrit ADR-0005.
//!
//! **C'est le seul module `unsafe` du crate**, et c'est la raison d'être de la feature :
//! trois appels système bruts, chacun enveloppé dans une fonction sûre qui traduit les
//! conventions C (`-1`, tampon partiellement rempli, chaîne terminée par un nul) en
//! `Result`. Personne d'autre dans Ash n'a à connaître ces conventions.
#![allow(unsafe_code)]

use std::os::fd::RawFd;
use std::path::PathBuf;

use super::error::ProbeError;
use super::port::{process_name, Pid, Probe, ProcessInfo};

/// Implémentation macOS de [`Probe`]. Choisie au composition root, nulle part ailleurs.
#[derive(Default)]
pub struct SystemProbe;

impl Probe for SystemProbe {
    fn foreground_pgid(&self, terminal: RawFd) -> Result<Pid, ProbeError> {
        // SAFETY: `tcgetpgrp` ne fait que consulter la table des descripteurs du
        // processus appelant ; il n'écrit dans aucune mémoire que nous détenons. Un
        // descripteur invalide, fermé, ou qui ne désigne pas un terminal n'est pas un
        // comportement indéfini : l'appel rend -1 et pose `errno`, cas traité juste après.
        let pgid = unsafe { libc::tcgetpgrp(terminal) };

        if pgid <= 0 {
            return Err(ProbeError::NoForeground(terminal));
        }
        Ok(pgid)
    }

    fn inspect(&self, pid: Pid) -> Result<ProcessInfo, ProbeError> {
        // Le `cwd` d'abord : c'est lui qui décide de l'onglet, et son échec est le signal
        // que le processus a disparu entre-temps. Aller chercher un nom serait du travail
        // perdu.
        let cwd = current_directory(pid)?;
        let executable = executable_path(pid)?;

        Ok(ProcessInfo {
            pid,
            name: process_name(&executable),
            cwd,
        })
    }
}

/// Le répertoire courant d'un processus — `proc_pidinfo(PROC_PIDVNODEPATHINFO)`.
fn current_directory(pid: Pid) -> Result<PathBuf, ProbeError> {
    let mut info = std::mem::MaybeUninit::<libc::proc_vnodepathinfo>::uninit();
    let size = i32::try_from(std::mem::size_of::<libc::proc_vnodepathinfo>())
        .map_err(|_| ProbeError::Vanished(pid))?;

    // SAFETY: le noyau écrit au plus `size` octets, et `size` est exactement la taille de
    // la structure pointée. Le pointeur vient d'un `MaybeUninit` vivant pour toute la
    // durée de l'appel, qui est synchrone. Un pid mort ou illisible rend une valeur
    // négative ou courte, jamais un tampon à moitié valide qu'on lirait quand même.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };

    if written != size {
        return Err(ProbeError::Vanished(pid));
    }

    // SAFETY: `proc_pidinfo` a rendu la taille complète de la structure, donc le noyau
    // l'a entièrement écrite. C'est la seule branche où cette lecture est permise.
    let info = unsafe { info.assume_init() };

    // `vip_path` est déclaré `[[c_char; 32]; 32]` par `libc` — un `[c_char; 1024]` que la
    // bibliothèque n'a pas pu écrire d'un trait. On le remet à plat, et on s'arrête au nul.
    let raw: Vec<u8> = info
        .pvi_cdir
        .vip_path
        .iter()
        .flatten()
        .map(|&byte| byte as u8)
        .collect();
    let end = raw.iter().position(|&byte| byte == 0).unwrap_or(raw.len());
    let path = String::from_utf8_lossy(&raw[..end]).into_owned();

    if path.is_empty() {
        return Err(ProbeError::Vanished(pid));
    }
    Ok(PathBuf::from(path))
}

/// Le chemin de l'exécutable d'un processus — `proc_pidpath`.
fn executable_path(pid: Pid) -> Result<PathBuf, ProbeError> {
    let capacity =
        usize::try_from(libc::PROC_PIDPATHINFO_MAXSIZE).map_err(|_| ProbeError::Vanished(pid))?;
    let mut buffer = vec![0u8; capacity];
    let size = u32::try_from(capacity).map_err(|_| ProbeError::Vanished(pid))?;

    // SAFETY: le tampon appartient à cette pile, il fait exactement `size` octets, et il
    // vit jusqu'à la fin de l'appel — qui est synchrone. `proc_pidpath` écrit une chaîne
    // sans nul final et rend sa longueur, bornée par `size`.
    let written = unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), size) };

    let length = usize::try_from(written).map_err(|_| ProbeError::Vanished(pid))?;
    if length == 0 || length > buffer.len() {
        return Err(ProbeError::Vanished(pid));
    }

    buffer.truncate(length);
    Ok(PathBuf::from(String::from_utf8_lossy(&buffer).into_owned()))
}
