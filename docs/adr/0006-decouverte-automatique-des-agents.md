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

---

### Amendement du 2026-08-18 — le **chemin** identifie, le nom ne suffit pas

La rédaction initiale dit « dès que son **nom** figure dans les commandes reconnues ». La
mise en œuvre a montré que ce nom-là n'existe pas toujours.

**Ce que le code a montré.** La sonde lit `proc_pidpath` — le chemin de l'exécutable — et
l'installateur officiel de Claude Code pose un binaire dont le nom de fichier **est le
numéro de version** :

```
~/.local/bin/claude → ~/.local/share/claude/versions/2.1.234
```

Le processus en avant-plan ne s'appelle donc pas `claude` mais `2.1.234`, et il s'appellera
`2.1.235` après la prochaine mise à jour. Une reconnaissance par nom de commande aurait
échoué pour l'installation la plus courante — celle de la grande majorité des
utilisateurs — et elle aurait échoué **en silence** : un onglet qui ne devient jamais un
agent ne rend aucune erreur, il ne se passe simplement rien.

**Ce qui est décidé.** Un provider connu est reconnu par **trois signaux**, essayés dans
cet ordre :

| Signal | Reconnaît | Cas |
|---|---|---|
| Le **chemin** d'installation | `~/.local/share/claude/versions/*` → `claude-code` | l'installateur officiel |
| Le **nom** de l'exécutable | `~/.kimi-code/bin/kimi` → `kimi` | les outils qui gardent leur nom |
| **`argv[0]`** | `claude` alors que l'exécutable est `node` | l'installation npm |

Le chemin passe en premier parce qu'il est le seul stable à travers les mises à jour. Il
est comparé **par segments** et non par sous-chaîne : un dossier qui contiendrait ces mots
par accident ne reconnaît rien.

`argv[0]` se lit par `sysctl(KERN_PROCARGS2)`, sans permission supplémentaire — la
contrainte d'ADR-0005 tient. Il est lu **au plus une fois par pid**, et jamais pour le
shell : le noyau y recopie tout l'espace d'arguments, ce qui est trop cher pour la boucle
de 300 ms.

**Ce qui ne change pas.** La décision de fond est intacte : Ash observe, il ne demande
rien, et l'utilisateur tape sa commande comme avant. La table des providers connus est
**embarquée** — c'est ce qui permet à un utilisateur qui n'a rien configuré de voir son
agent dès la première seconde. Les entrées déclarées à la main l'emportent sur elle, pour
les cas que la table ne couvre pas : alias, script intermédiaire, second compte.

**Ce que ça ne fait pas.** Reconnaître est de la **lecture** : aucun fichier écrit, aucune
permission, donc rien à demander à l'utilisateur. Instrumenter reste de l'**écriture** dans
ses fichiers, donc un geste explicite ([ADR-0007](./0007-etats-par-hooks.md)). Un agent
reconnu mais non instrumenté a `idle` et `working`, qui viennent de la sonde, et n'aura
jamais `waiting`, qui n'a jamais d'autre source qu'un hook.

**Limite connue, et assumée.** Un lanceur qui réécrit son propre `argv[0]` — certains
wrappers `npx` — reste invisible aux trois signaux. C'est le cas déjà accepté plus haut
(« un outil lancé indirectement peut ne pas être reconnu »), et il se règle par une
déclaration à la main.
