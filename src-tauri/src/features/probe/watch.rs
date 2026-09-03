use std::os::fd::RawFd;
use std::path::{Path, PathBuf};

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
    /// Le chemin entier de l'exécutable — le signal le plus stable d'ADR-0006.
    ///
    /// Un binaire posé par l'installateur officiel de Claude Code s'appelle `2.1.234` : son
    /// nom change à chaque mise à jour, son chemin non.
    pub executable: PathBuf,
    /// Le premier mot de sa ligne de commande, quand le système a bien voulu le dire.
    ///
    /// C'est ce qui reconnaît un outil lancé par npm — l'exécutable est alors `node`. Il est
    /// lu **une fois par programme en avant-plan** et non à chaque passe : le relire trois
    /// fois par seconde ferait recopier l'espace d'arguments du processus pour rien. Ce qui
    /// décide qu'un programme est nouveau tient sur `TabWatch::known_argv0`, et là seulement.
    ///
    /// Il peut donc être périmé dans un cas, nommé au même endroit : un `exec` vers le
    /// *même* chemin d'exécutable garde l'`argv[0]` d'avant. Ce que ça coûte se lit dans
    /// `agents::providers`, qui compare ce mot en dernier — la reconnaissance retombe alors
    /// sur les deux premiers signaux d'ADR-0006, ou se trompe d'outil si le même binaire a
    /// été relancé sous un autre nom.
    pub argv0: Option<String>,
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
    /// Le `argv[0]` du dernier avant-plan observé, avec **le pid et l'exécutable** auxquels
    /// il appartient.
    ///
    /// La mémoire est ce qui rend le troisième signal gratuit, et c'est la propriété
    /// d'ADR-0006 à tenir : **une lecture de `sysctl` par programme lancé**, pas trois par
    /// seconde. `KERN_PROCARGS2` fait recopier l'espace d'arguments entier du processus, et
    /// la boucle d'ADR-0005 repasse toutes les 300 ms.
    ///
    /// Ce qui ne change pas de ligne de commande, c'est le **couple pid + exécutable**, et
    /// non le pid seul : `execve` remplace la ligne de commande en **gardant** le pid. Bash
    /// lance une commande par `fork` puis `exec`, et l'enfant porte le pgid de l'avant-plan
    /// dès le `fork` — une passe de sonde peut donc tomber entre les deux et voir un
    /// processus qui est encore bash. Mémoriser contre le seul pid figeait alors `bash`, ou
    /// le `None` d'un `sysctl` refusé pendant la transition, pour toute la vie de l'onglet :
    /// un agent installé par npm — exécutable `node`, `argv[0]` `claude` — n'aurait plus
    /// jamais été reconnu. Le chemin de l'exécutable, lui, est relu à chaque passe : le
    /// prendre pour clé ne coûte aucun appel système de plus.
    ///
    /// **Angle mort assumé** : un `exec` vers le *même* chemin — `exec bash`, un agent qui
    /// se relance par-dessus lui-même — garde un `argv[0]` périmé. La clé ne bouge pas,
    /// donc la mémoire non plus. Rien ne le rattrape avant que l'avant-plan change.
    known_argv0: Option<(Pid, PathBuf, Option<String>)>,
}

impl TabWatch {
    pub fn new(terminal: RawFd, shell: Pid) -> Self {
        Self {
            terminal,
            shell,
            last: None,
            known_argv0: None,
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
                let argv0 = self.argv0_of(probe, info.pid, &info.executable);
                let observation = TabObservation {
                    cwd: info.cwd,
                    foreground: Foreground {
                        pid: info.pid,
                        is_shell: info.pid == self.shell,
                        name: info.name,
                        executable: info.executable,
                        argv0,
                    },
                };
                self.last = Some(observation.clone());
                Ok(observation)
            }
            Err(silence) => self.last.clone().ok_or(silence),
        }
    }

    /// Le `argv[0]` d'un avant-plan, demandé au système **au plus une fois par programme**.
    ///
    /// La clé est le couple pid + exécutable, pour la raison écrite sur [`Self::known_argv0`] :
    /// un pid traverse `execve`, et une passe de sonde peut tomber entre le `fork` et l'`exec`
    /// de bash. Un exécutable qui change sous un pid inchangé est exactement cet instant-là :
    /// on redemande, une fois.
    ///
    /// Le shell n'est jamais interrogé : il n'est reconnu comme aucun outil, et l'onglet à
    /// son invite est le cas le plus fréquent de tous. C'est ce qui garde la passe de sonde
    /// à ses deux appels système d'ADR-0005 tant que rien de neuf ne tient l'avant-plan.
    fn argv0_of(&mut self, probe: &dyn Probe, pid: Pid, executable: &Path) -> Option<String> {
        if pid == self.shell {
            return None;
        }
        if let Some((known, ran, argv0)) = &self.known_argv0 {
            if *known == pid && ran == executable {
                return argv0.clone();
            }
        }
        let argv0 = probe.argv0(pid);
        self.known_argv0 = Some((pid, executable.to_path_buf(), argv0.clone()));
        argv0
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
        /// Ce que `sysctl` répondrait, et **combien de fois** on le lui a demandé.
        argv0: HashMap<Pid, String>,
        asked: std::sync::Mutex<Vec<Pid>>,
    }

    /// Test Data Builder : un système cohérent par défaut — un shell à son invite.
    struct SystemBuilder {
        foreground: Option<Pid>,
        processes: HashMap<Pid, ProcessInfo>,
        argv0: HashMap<Pid, String>,
    }

    impl SystemBuilder {
        fn new() -> Self {
            let mut builder = Self {
                foreground: Some(SHELL),
                processes: HashMap::new(),
                argv0: HashMap::new(),
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
                    executable: PathBuf::from(format!("/usr/local/bin/{name}")),
                    cwd: PathBuf::from(cwd),
                },
            );
            self
        }

        /// Le processus présente ce premier mot de ligne de commande.
        fn announcing(mut self, pid: Pid, argv0: &str) -> Self {
            self.argv0.insert(pid, argv0.to_owned());
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
                argv0: self.argv0,
                asked: std::sync::Mutex::new(Vec::new()),
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

        fn argv0(&self, pid: Pid) -> Option<String> {
            self.asked.lock().unwrap().push(pid);
            self.argv0.get(&pid).cloned()
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
    fn given_a_program_that_keeps_the_foreground_when_probing_twice_then_its_command_line_is_read_only_once(
    ) {
        // Given — `sysctl(KERN_PROCARGS2)` fait recopier l'espace d'arguments du processus,
        // et la boucle d'ADR-0005 passe trois fois par seconde. Un `argv[0]` ne change pas
        // tant que le même exécutable tourne sous le même pid : le redemander serait un
        // coût pur.
        let system = SystemBuilder::new()
            .with_process(AGENT, "node", "/dev/ash")
            .announcing(AGENT, "claude")
            .handed_over_to(AGENT)
            .build();
        let mut watch = watch();

        // When
        let first = watch.observe(&system).expect("le système doit répondre");
        let second = watch.observe(&system).expect("le système doit répondre");

        // Then — le troisième signal d'ADR-0006 est là, et il n'a coûté qu'un appel
        assert_eq!(first.foreground.argv0.as_deref(), Some("claude"));
        assert_eq!(second.foreground.argv0, first.foreground.argv0);
        assert_eq!(system.asked.lock().unwrap().as_slice(), [AGENT]);
    }

    #[test]
    fn given_a_foreground_that_execs_into_another_program_under_the_same_pid_when_probing_then_its_command_line_is_read_again(
    ) {
        // Given — bash lance une commande par `fork` puis `exec`, et l'enfant porte le pgid
        // de l'avant-plan dès le `fork` : une passe de sonde peut le voir encore bash. Le
        // pid, lui, ne change pas — c'est le même processus qui devient l'agent.
        let forked = SystemBuilder::new()
            .with_process(AGENT, "bash", "/dev/ash")
            .announcing(AGENT, "bash")
            .handed_over_to(AGENT)
            .build();
        let executed = SystemBuilder::new()
            .with_process(AGENT, "node", "/dev/ash")
            .announcing(AGENT, "claude")
            .handed_over_to(AGENT)
            .build();
        let mut watch = watch();

        // When — la première passe tombe dans la fenêtre fork/exec, les deux suivantes
        // arrivent après
        let before = watch.observe(&forked).expect("le système doit répondre");
        let after = watch.observe(&executed).expect("le système doit répondre");
        let still = watch.observe(&executed).expect("le système doit répondre");

        // Then — mémorisé contre le seul pid, `claude` resterait `bash` pour toute la vie de
        // l'onglet, et le troisième signal d'ADR-0006 serait perdu sans aucun symptôme
        assert_eq!(before.foreground.argv0.as_deref(), Some("bash"));
        assert_eq!(after.foreground.argv0.as_deref(), Some("claude"));
        // …et on redemande **une fois**, pas à chaque passe : l'`exec` rouvre la mémoire, il
        // ne la supprime pas
        assert_eq!(still.foreground.argv0.as_deref(), Some("claude"));
        assert_eq!(executed.asked.lock().unwrap().as_slice(), [AGENT]);
    }

    #[test]
    fn given_a_tab_at_its_prompt_when_probing_then_the_shell_command_line_is_never_read() {
        // Given — un onglet posé à son invite est le cas le plus fréquent de tous, et aucun
        // shell n'est un agent (ADR-0006) : l'interroger serait un appel système par passe
        // et par onglet, pour une réponse dont personne ne ferait rien
        let system = SystemBuilder::new().build();

        // When
        let seen = watch().observe(&system).expect("le shell reste observable");

        // Then
        assert!(seen.foreground.is_shell);
        assert!(system.asked.lock().unwrap().is_empty());
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
