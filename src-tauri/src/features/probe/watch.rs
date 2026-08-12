use std::os::fd::RawFd;
use std::path::PathBuf;

use super::error::ProbeError;
use super::port::{Pid, Probe};

/// Ce qu'une passe de sonde apprend d'un onglet.
///
/// Le `cwd` et l'avant-plan voyagent ensemble : c'est la même passe qui les produit, et
/// [ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md) tient à ce qu'il n'y ait
/// qu'un mécanisme pour les deux usages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabObservation {
    /// Le répertoire du processus en avant-plan — pas celui du shell.
    ///
    /// C'est toute la différence avec OSC 7 : pendant qu'un programme tourne, c'est *son*
    /// répertoire qui décrit ce que l'onglet est en train de faire.
    pub cwd: PathBuf,
    pub foreground: Foreground,
}

/// Qui tient l'avant-plan du terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foreground {
    pub pid: Pid,
    pub name: String,
    /// Vrai quand c'est le shell lui-même — l'onglet est à son invite.
    ///
    /// C'est la frontière que la découverte d'agents (ADR-0006) regardera : un onglet
    /// n'est pas « un agent » ou « un shell », il le **devient**.
    pub is_shell: bool,
}

/// La sonde d'un onglet : ce qu'il faut interroger, et ce qu'on a vu la dernière fois.
///
/// Le descripteur est celui du master du PTY. Sa validité est celle de l'onglet : la
/// sonde vit dans le même objet que la session qui détient le master, et meurt avec elle.
/// Aucune sonde ne survit donc au descripteur qu'elle interroge.
pub struct TabWatch {
    terminal: RawFd,
    /// Le pid du shell, retenu à l'ouverture — le repli quand l'avant-plan se dérobe.
    shell: Pid,
    last: Option<TabObservation>,
}

impl TabWatch {
    pub fn new(terminal: RawFd, shell: Pid) -> Self {
        Self {
            terminal,
            shell,
            last: None,
        }
    }

    /// Une passe de sonde : `tcgetpgrp`, puis `proc_pidinfo`.
    ///
    /// Trois replis, dans cet ordre, parce que sonder un système vivant est une course
    /// perdue d'avance si on exige que les deux appels parlent du même instant :
    ///
    /// 1. le terminal ne désigne personne — le shell vient de reprendre la main, ou de
    ///    mourir : on interroge le shell ;
    /// 2. le processus en avant-plan a disparu entre les deux appels : on interroge le
    ///    shell, dont le `cwd` reste la meilleure réponse ;
    /// 3. plus rien n'est lisible : on rend la dernière observation réussie plutôt que
    ///    de faire clignoter l'onglet vers un état vide.
    pub fn observe(&mut self, probe: &dyn Probe) -> Result<TabObservation, ProbeError> {
        let leader = probe.foreground_pgid(self.terminal).unwrap_or(self.shell);

        let seen = probe.inspect(leader).or_else(|vanished| {
            if leader == self.shell {
                Err(vanished)
            } else {
                probe.inspect(self.shell)
            }
        });

        match seen {
            Ok(info) => {
                let observation = TabObservation {
                    cwd: info.cwd,
                    foreground: Foreground {
                        pid: info.pid,
                        is_shell: info.pid == self.shell,
                        name: info.name,
                    },
                };
                self.last = Some(observation.clone());
                Ok(observation)
            }
            Err(silence) => self.last.clone().ok_or(silence),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::port::ProcessInfo;
    use super::*;
    use std::collections::HashMap;

    const TERMINAL: RawFd = 7;
    const SHELL: Pid = 100;
    const AGENT: Pid = 200;

    /// Un système observable, entièrement décrit par le test.
    ///
    /// Aucun processus n'est lancé : ce qui est vérifié ici, ce sont les règles de repli
    /// de la sonde, pas `libproc`.
    #[derive(Default)]
    struct FakeProbe {
        foreground: Option<Pid>,
        processes: HashMap<Pid, ProcessInfo>,
    }

    /// Test Data Builder : un système cohérent par défaut — un shell à son invite.
    struct SystemBuilder {
        foreground: Option<Pid>,
        processes: HashMap<Pid, ProcessInfo>,
    }

    impl SystemBuilder {
        fn new() -> Self {
            let mut builder = Self {
                foreground: Some(SHELL),
                processes: HashMap::new(),
            };
            builder = builder.with_process(SHELL, "bash", "/dev/ash");
            builder
        }

        fn with_process(mut self, pid: Pid, name: &str, cwd: &str) -> Self {
            self.processes.insert(
                pid,
                ProcessInfo {
                    pid,
                    name: name.to_owned(),
                    cwd: PathBuf::from(cwd),
                },
            );
            self
        }

        /// Le terminal a été donné à ce groupe de processus.
        fn handed_over_to(mut self, pid: Pid) -> Self {
            self.foreground = Some(pid);
            self
        }

        /// Le processus existe encore pour `tcgetpgrp`, mais plus pour `proc_pidinfo`.
        fn without_process(mut self, pid: Pid) -> Self {
            self.processes.remove(&pid);
            self
        }

        fn without_foreground(mut self) -> Self {
            self.foreground = None;
            self
        }

        fn build(self) -> FakeProbe {
            FakeProbe {
                foreground: self.foreground,
                processes: self.processes,
            }
        }
    }

    impl Probe for FakeProbe {
        fn foreground_pgid(&self, terminal: RawFd) -> Result<Pid, ProbeError> {
            self.foreground.ok_or(ProbeError::NoForeground(terminal))
        }

        fn inspect(&self, pid: Pid) -> Result<ProcessInfo, ProbeError> {
            self.processes
                .get(&pid)
                .cloned()
                .ok_or(ProbeError::Vanished(pid))
        }
    }

    fn watch() -> TabWatch {
        TabWatch::new(TERMINAL, SHELL)
    }

    #[test]
    fn given_a_program_running_in_a_subdirectory_when_probing_then_it_reports_that_directory() {
        // Given — le shell est resté dans /dev/ash, le programme travaille ailleurs
        let system = SystemBuilder::new()
            .with_process(AGENT, "claude", "/dev/ash/worktrees/probe")
            .handed_over_to(AGENT)
            .build();

        // When
        let seen = watch().observe(&system).expect("le système doit répondre");

        // Then — c'est là qu'OSC 7 se tairait : le shell n'est pas revenu à son invite
        assert_eq!(seen.cwd, PathBuf::from("/dev/ash/worktrees/probe"));
        assert_eq!(seen.foreground.name, "claude");
        assert!(!seen.foreground.is_shell);
    }

    #[test]
    fn given_a_shell_at_its_prompt_when_probing_then_the_foreground_is_the_shell_itself() {
        // Given
        let system = SystemBuilder::new().build();

        // When
        let seen = watch().observe(&system).expect("le système doit répondre");

        // Then — aucun agent ne naît d'un onglet posé à son invite (ADR-0006)
        assert!(seen.foreground.is_shell);
        assert_eq!(seen.cwd, PathBuf::from("/dev/ash"));
    }

    #[test]
    fn given_a_foreground_process_that_died_between_the_two_calls_when_probing_then_it_falls_back_to_the_shell(
    ) {
        // Given — `tcgetpgrp` le désigne encore, `proc_pidinfo` ne le connaît déjà plus.
        // C'est la course nominale d'une boucle qui sonde toutes les 300 ms.
        let system = SystemBuilder::new()
            .handed_over_to(AGENT)
            .without_process(AGENT)
            .build();

        // When
        let seen = watch().observe(&system).expect("le shell reste observable");

        // Then
        assert_eq!(seen.cwd, PathBuf::from("/dev/ash"));
        assert!(seen.foreground.is_shell);
    }

    #[test]
    fn given_a_terminal_that_designates_nobody_when_probing_then_the_shell_answers_for_the_tab() {
        // Given — entre deux commandes, le terminal peut n'avoir aucun groupe en avant-plan
        let system = SystemBuilder::new().without_foreground().build();

        // When
        let seen = watch().observe(&system).expect("le shell reste observable");

        // Then
        assert_eq!(seen.cwd, PathBuf::from("/dev/ash"));
    }

    #[test]
    fn given_a_tab_already_seen_when_nothing_is_observable_anymore_then_it_keeps_its_last_position()
    {
        // Given — le shell est mort ; la sonde n'a plus rien à interroger
        let mut watch = watch();
        watch
            .observe(&SystemBuilder::new().build())
            .expect("la première passe doit réussir");
        let dead = SystemBuilder::new().without_process(SHELL).build();

        // When
        let seen = watch
            .observe(&dead)
            .expect("la dernière position doit rester");

        // Then — l'onglet ne doit pas clignoter vers un répertoire vide avant sa fermeture
        assert_eq!(seen.cwd, PathBuf::from("/dev/ash"));
    }

    #[test]
    fn given_a_tab_never_observed_when_nothing_is_observable_then_probing_fails_instead_of_inventing(
    ) {
        // Given
        let system = SystemBuilder::new().without_process(SHELL).build();

        // When
        let seen = watch().observe(&system);

        // Then
        assert_eq!(seen.unwrap_err(), ProbeError::Vanished(SHELL));
    }
}
