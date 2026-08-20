//! La sonde réelle : `tcgetpgrp` puis `libproc`, comme le décrit ADR-0005.
//!
//! **C'est le seul module `unsafe` du crate**, et c'est la raison d'être de la feature :
//! quatre appels système bruts, chacun enveloppé dans une fonction sûre qui traduit les
//! conventions C (`-1`, tampon partiellement rempli, chaîne terminée par un nul) en
//! `Result`. Personne d'autre dans Ash n'a à connaître ces conventions.
#![allow(unsafe_code)]

use std::os::fd::RawFd;
use std::path::PathBuf;

use super::control::{signalable, ProcessControl};
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
            executable,
            cwd,
        })
    }

    fn argv0(&self, pid: Pid) -> Option<String> {
        first_argument(pid)
    }
}

impl ProcessControl for SystemProbe {
    fn pause(&self, pgid: Pid) -> Result<(), ProbeError> {
        signal_group(pgid, libc::SIGSTOP)
    }

    fn resume(&self, pgid: Pid) -> Result<(), ProbeError> {
        signal_group(pgid, libc::SIGCONT)
    }
}

/// Poste un signal au groupe de processus entier — `killpg`.
///
/// **Le groupe, et pas le pid**, parce que c'est le groupe qui tient l'avant-plan du
/// terminal : un agent lance des fils d'exécution et des sous-processus, et arrêter le seul
/// chef de groupe laisserait ses enfants écrire dans le worktree pendant le checkout — donc
/// laisserait la pause mentir. C'est aussi ce que `tcgetpgrp` désigne, et ce que le noyau
/// signale déjà lui-même pour `SIGWINCH` à chaque redimensionnement.
///
/// Le garde de [`signalable`] passe **avant** l'appel, jamais après : c'est lui qui
/// distingue « un pgid » de « mon propre groupe ».
fn signal_group(pgid: Pid, signal: libc::c_int) -> Result<(), ProbeError> {
    let target = signalable(pgid)?;

    // SAFETY: `killpg` ne fait que poster un signal ; il n'écrit dans aucune mémoire que
    // nous détenons et ne prend aucun pointeur. Un groupe disparu ou refusé n'est pas un
    // comportement indéfini : l'appel rend -1 et pose `errno`, cas traité juste après. Le
    // seul argument dangereux — un pgid qui désignerait notre propre groupe — a été écarté
    // au-dessus.
    let answered = unsafe { libc::killpg(target, signal) };

    if answered != 0 {
        return Err(ProbeError::SignalRefused {
            pgid: target,
            errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
        });
    }
    Ok(())
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

/// `argv[0]` d'un processus — `sysctl(KERN_PROCARGS2)`.
///
/// C'est le troisième signal d'ADR-0006 : un outil installé par npm tourne sous un
/// exécutable nommé `node`, et seul son premier argument dit `claude`. L'appel est le même
/// que celui de `ps`, **sans autorisation supplémentaire** : ni `task_for_pid`, ni
/// entitlement, ni consentement à demander à l'utilisateur.
///
/// Le format que le noyau rend est fixé et sans en-tête décrivant les longueurs :
///
/// ```text
/// [argc: i32][chemin de l'exécutable\0][\0 de bourrage]…[argv[0]\0][argv[1]\0]…
/// ```
///
/// On saute donc le compteur, puis le chemin, puis les nuls de bourrage, et ce qui vient
/// ensuite est `argv[0]`. Toute anomalie rend `None` : ce signal est un **bonus**, et son
/// silence laisse les deux premiers décider.
fn first_argument(pid: Pid) -> Option<String> {
    let raw = process_arguments(pid)?;

    // Le compteur d'arguments, dont on ne se sert pas : ce qui suit est le chemin.
    let after_argc = raw.get(std::mem::size_of::<libc::c_int>()..)?;

    let path_end = after_argc.iter().position(|&byte| byte == 0)?;
    let after_path = after_argc.get(path_end..)?;
    let start = after_path.iter().position(|&byte| byte != 0)?;
    let rest = after_path.get(start..)?;
    let end = rest
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(rest.len());

    let argument = String::from_utf8_lossy(rest.get(..end)?).into_owned();
    (!argument.is_empty()).then_some(argument)
}

/// L'espace d'arguments d'un processus, tel quel.
///
/// La taille du tampon vient de `KERN_ARGMAX` et non d'une constante choisie ici : le noyau
/// refuse (`ENOMEM`) un tampon plus petit que ce qu'il a à recopier, et un tampon deviné
/// trop court ferait taire le signal exactement pour les commandes aux longues lignes.
fn process_arguments(pid: Pid) -> Option<Vec<u8>> {
    let capacity = usize::try_from(argument_max()?).ok()?;
    let mut buffer = vec![0u8; capacity];
    let mut written = capacity;
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];

    // SAFETY: `mib` fait bien les trois entiers annoncés, le tampon appartient à cette pile
    // et vit jusqu'à la fin de l'appel, et `written` dit au noyau combien d'octets il a le
    // droit d'y écrire — il le remplace ensuite par ce qu'il a réellement écrit. Un pid mort
    // ou illisible rend -1 et pose `errno`, cas traité juste après.
    let answered = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buffer.as_mut_ptr().cast(),
            &mut written,
            std::ptr::null_mut(),
            0,
        )
    };

    if answered != 0 || written == 0 || written > buffer.len() {
        return None;
    }
    buffer.truncate(written);
    Some(buffer)
}

/// `KERN_ARGMAX` : la taille que le noyau peut avoir à recopier.
fn argument_max() -> Option<libc::c_int> {
    let mut max: libc::c_int = 0;
    let mut written = std::mem::size_of::<libc::c_int>();
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];

    // SAFETY: le noyau écrit au plus `written` octets à l'adresse d'un `c_int` de cette
    // pile, et `written` est exactement sa taille. L'appel est synchrone.
    let answered = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            std::ptr::addr_of_mut!(max).cast(),
            &mut written,
            std::ptr::null_mut(),
            0,
        )
    };

    (answered == 0 && max > 0).then_some(max)
}
