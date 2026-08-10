# ADR-0006 — Les agents sont découverts, pas déclarés

- **Statut** : accepté (2026-08-07)
- **S'appuie sur** : [ADR-0005](./0005-sonde-cwd-libproc.md)

## Contexte

L'exigence de départ est claire : garder l'accès à bash et lancer `claude`, `codex`,
`kimi`, `opencode` « peu importe » — c'est-à-dire en les tapant soi-même. Mais la
sidebar doit malgré tout savoir qu'un agent existe, dans quel dossier, depuis quand.

Quatre façons de faire naître une entrée dans la sidebar : la découvrir, la faire
créer par l'application, la déclarer via un wrapper explicite, ou combiner.

## Décision

**Découverte automatique.** Chaque onglet est un `bash` ordinaire. Ash observe le
processus en avant-plan du PTY (`tcgetpgrp`) ; dès que son nom figure dans les
commandes reconnues de `~/.ash/config.toml`, une entrée apparaît dans la sidebar,
avec le `cwd` et la branche git de l'onglet. Quand le processus disparaît, l'agent
se termine.

Un onglet n'est donc pas « un agent » ou « un shell » : il **devient** un agent, puis
redevient un shell.

## Conséquences

- Aucun changement d'habitude : on tape `claude` comme avant, et ça apparaît.
- Les deux comptes Claude de l'utilisateur sont distingués gratuitement, puisqu'ils
  sont déjà deux commandes distinctes dans le `PATH` (`claude` et `claude-perso`).
  Ash n'a donc pas besoin d'une notion de profil.
- Un outil lancé indirectement (alias, `npx`, `bunx`, script intermédiaire) peut ne
  pas être reconnu par son nom de processus. Cas accepté ; la configuration permet
  d'ajouter des `match` supplémentaires.
- Ash n'ayant pas lancé l'outil, il ne peut pas lui passer de drapeaux au démarrage.
  L'instrumentation doit donc passer par la configuration de l'outil
  ([ADR-0007](./0007-etats-par-hooks.md)) et par l'environnement du shell, préparé
  au moment où Ash crée le PTY.
- La même sonde sert au `cwd` et à la détection : un seul mécanisme à maintenir.

## Alternatives écartées

- **L'application spawn, la sidebar fait autorité** (bouton `+`, choix du projet et
  de l'outil) : information parfaite et instrumentation triviale, mais renverse le
  rapport — c'est l'application qui commande, plus le shell.
- **Wrapper explicite** (`ash claude`, ou un shim nommé `claude` en tête de `PATH`) :
  fiable à 100 % sans polling, et reste du bash. Écarté parce qu'un oubli rend
  l'agent invisible, et parce qu'un shim qui fuit hors d'Ash est une mauvaise
  surprise. Reste l'option de repli si la découverte par nom de processus s'avère
  trop poreuse.
- **Découverte + spawn** : couvre tous les usages mais impose de réconcilier les
  doublons (un agent lancé par l'application puis redécouvert). Repoussé.
