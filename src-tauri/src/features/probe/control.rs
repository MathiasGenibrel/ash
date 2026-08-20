//! Arrêter et reprendre un groupe de processus — `SIGSTOP`, `SIGCONT`, et rien d'autre.
//!
//! C'est la « pause » d'[ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md),
//! au sens exact que son corollaire lui donne : **aucune touche envoyée au PTY, aucune
//! interprétation de l'interface de l'outil**. Envoyer `Esc` parce qu'on suppose que ça
//! interrompt serait supposer ce que l'agent affiche, exactement ce qu'ADR-0010 interdit.
//! Un signal, lui, ne suppose rien : le noyau arrête le processus, et le processus ne
//! l'apprend même pas.
//!
//! Le trait vit dans `probe` et non dans `pty` pour la raison qui a fait exister la
//! feature : c'est ici que le crate connaît les processus au sens du système — le groupe
//! en avant-plan vient déjà de [`super::Probe::foreground_pgid`] —, et c'est ici que
//! l'`unsafe` est confiné ([`super::macos`]). `pty` tient des onglets ; il n'a pas à
//! connaître `libc`.

use super::error::ProbeError;
use super::port::Pid;

/// Ce qu'on peut faire à un groupe de processus en avant-plan.
///
/// Deux verbes, et **exactement** deux : il n'y a pas de `kill` ici, et il n'y en aura pas.
/// Ash ne tue pas l'agent d'un utilisateur pour lui laisser faire un checkout — il l'arrête,
/// le temps du geste, et le rend ensuite. Un troisième verbe destructeur dans ce trait
/// ferait de la pause une porte vers autre chose.
///
/// Un trait, et pas deux fonctions libres : sans lui, la règle qui compte — « on ne signale
/// jamais son propre groupe » — ne se vérifierait qu'en arrêtant Ash pendant `cargo test`.
pub trait ProcessControl: Send + Sync {
    /// `SIGSTOP` sur le groupe. Le processus s'arrête sans pouvoir l'intercepter.
    fn pause(&self, pgid: Pid) -> Result<(), ProbeError>;

    /// `SIGCONT` sur le groupe. Il reprend là où il en était.
    fn resume(&self, pgid: Pid) -> Result<(), ProbeError>;
}

/// Le garde qui précède tout signal, isolé de `libc` pour être vérifiable.
///
/// `killpg(0, …)` ne veut pas dire « personne » : ça veut dire **le groupe de celui qui
/// appelle**, donc Ash lui-même. Un `SIGSTOP` posté là gèlerait la fenêtre, le lecteur de
/// chaque PTY et la boucle de sonde, et il n'existerait plus personne pour envoyer le
/// `SIGCONT` qui la réveille : l'application serait à reprendre à la main depuis un autre
/// terminal. Les valeurs négatives sont refusées pour la même raison — `killpg` les traite
/// comme des pgid, et `kill(-1, …)` vise tous les processus de l'utilisateur.
///
/// Le seul chemin de production qui alimente ce garde est [`super::Probe::foreground_pgid`],
/// qui rend déjà un pgid strictement positif. Il est là malgré tout : le jour où un second
/// appelant apparaît, c'est cette fonction — et son test — qui l'attend, pas une revue de code.
pub(super) fn signalable(pgid: Pid) -> Result<Pid, ProbeError> {
    if pgid <= 1 {
        return Err(ProbeError::NotSignalable(pgid));
    }
    Ok(pgid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_the_process_group_of_ash_itself_when_asking_to_signal_it_then_it_is_refused() {
        // Given — `killpg(0, …)` vise le groupe de l'appelant, c'est-à-dire Ash
        let own_group: Pid = 0;

        // When
        let allowed = signalable(own_group);

        // Then — un `SIGSTOP` là gèle la fenêtre, et plus personne ne peut la réveiller
        assert_eq!(allowed, Err(ProbeError::NotSignalable(0)));
    }

    #[test]
    fn given_a_negative_group_when_asking_to_signal_it_then_it_is_refused() {
        // Given — `kill(-1, …)` vise tous les processus de l'utilisateur
        let everything: Pid = -1;

        // When
        let allowed = signalable(everything);

        // Then
        assert_eq!(allowed, Err(ProbeError::NotSignalable(-1)));
    }

    #[test]
    fn given_the_init_process_when_asking_to_signal_it_then_it_is_refused() {
        // Given
        let init: Pid = 1;

        // When
        let allowed = signalable(init);

        // Then — aucun agent ne tourne dans le groupe 1, donc le viser est toujours une erreur
        assert_eq!(allowed, Err(ProbeError::NotSignalable(1)));
    }

    #[test]
    fn given_a_real_foreground_group_when_asking_to_signal_it_then_it_is_allowed() {
        // Given — ce que `tcgetpgrp` rend pour un onglet où tourne un agent
        let foreground: Pid = 4213;

        // When
        let allowed = signalable(foreground);

        // Then
        assert_eq!(allowed, Ok(4213));
    }
}
