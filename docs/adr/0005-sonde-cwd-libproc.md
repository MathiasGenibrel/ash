# ADR-0005 — Suivi du `cwd` par sonde système, sans toucher au shell

- **Statut** : accepté (2026-08-07)
- **Découle de** : [ADR-0004](./0004-workspace-racine-git.md)

## Contexte

Rattacher un onglet au bon workspace après un `cd` suppose de connaître le `cwd` du
shell en temps réel. Deux écoles existent :

- le **shell le dit** : la convention OSC 7, émise à chaque prompt, utilisée par
  iTerm, WezTerm et Ghostty. Instantané et exact, mais suppose d'ajouter une ligne
  au `PROMPT_COMMAND` / `precmd` de l'utilisateur ;
- le **système le dit** : macOS expose le `cwd` d'un processus via `libproc`
  (`proc_pidinfo` / `PROC_PIDVNODEPATHINFO`).

## Décision

Ash **sonde le système**. Pour chaque onglet, toutes les ~300 ms :

```
fg_pgid = tcgetpgrp(pty_master)
cwd     = proc_pidinfo(fg_pgid, PROC_PIDVNODEPATHINFO)
```

Aucun fichier de configuration shell n'est modifié.

## Conséquences

- Ash fonctionne avec n'importe quel shell, sans installation préalable, et continue
  de fonctionner si l'utilisateur change de shell.
- Le `cwd` est correct même quand le `cd` est fait à l'intérieur d'un script, ou
  pendant qu'un programme tourne — cas où OSC 7 ne dit rien, puisqu'il n'émet qu'au
  retour du prompt. C'est précisément le cas d'usage d'Ash : un agent qui tourne
  longtemps.
- Latence d'au plus ~300 ms sur le changement de workspace. Imperceptible ici.
- Une sonde par onglet : coût négligeable pour une dizaine d'onglets, à surveiller
  au-delà.
- Spécifique à macOS. Un portage Linux devrait lire `/proc/<pid>/cwd`. La sonde est
  donc isolée derrière une abstraction (`probe.rs`), même si Linux n'est pas visé.
- La même sonde sert à détecter le processus en avant-plan, donc la naissance et la
  mort des agents ([ADR-0006](./0006-decouverte-automatique-des-agents.md)) : un seul
  mécanisme, deux usages.

## Alternatives écartées

- **OSC 7** : instantané et sans polling, mais modifie la configuration shell de
  l'utilisateur — refusé — et reste muet pendant qu'un programme tourne.
- **Les deux, OSC 7 avec repli sur la sonde** : le plus robuste en théorie, mais
  deux sources de vérité à arbitrer pour un gain de latence sans valeur ici.
- **Pas de suivi** (workspace figé au démarrage de l'onglet) : contredirait
  [ADR-0004](./0004-workspace-racine-git.md).
