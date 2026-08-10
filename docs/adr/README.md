# Décisions d'architecture (ADR)

Chaque fichier trace une décision structurante : son contexte, ce qui a été décidé,
ce que ça coûte, et ce qui a été écarté et pourquoi.

Statut au 2026-08-10 : 0001 à 0010 sont issues de la session de cadrage du 2026-08-07,
0011 à 0015 de la revue de la direction visuelle. Les ADR qui portaient la mention
« à réviser avec le design » ont été reprises : 0001 et 0002 confirmées, 0003
reformulée.

| # | Décision | Statut |
|---|---|---|
| [0001](./0001-application-graphique-avec-pty-embarques.md) | Application graphique embarquant des PTY | confirmée par le design |
| [0002](./0002-tauri-rust-portable-pty.md) | Tauri + Rust (`portable-pty`) comme coquille | confirmée par le design |
| [0003](./0003-zone-terminal-unique.md) | Un seul terminal à la fois, pas de splits de terminaux | **reformulée** |
| [0004](./0004-workspace-racine-git.md) | Le workspace est la racine git du `cwd`, suivie en direct | amendée par 0012 |
| [0005](./0005-sonde-cwd-libproc.md) | Suivi du `cwd` par sonde système, sans toucher au shell | — |
| [0006](./0006-decouverte-automatique-des-agents.md) | Les agents sont découverts, pas déclarés | — |
| [0007](./0007-etats-par-hooks.md) | Les états viennent des hooks de l'outil, pas de la sortie | — |
| [0008](./0008-abstraction-adapter.md) | Un trait `Adapter` dès le premier jalon | — |
| [0009](./0009-cycle-de-vie-des-agents.md) | Les agents meurent avec l'application (v1) | amendée par 0014 · à revoir à l'usage |
| [0010](./0010-sidebar-informe-terminal-agit.md) | La sidebar informe, le terminal agit | amendée par 0015 |
| [0011](./0011-git-domaine-de-premier-plan.md) | Git est un domaine de premier plan, intégré à Ash | — |
| [0012](./0012-worktree-unite-de-travail.md) | Le worktree est l'unité de travail, le dépôt est le groupe | — |
| [0013](./0013-fiche-de-branche-dans-le-depot.md) | La fiche de branche vit dans le dépôt, en markdown | — |
| [0014](./0014-attribution-locale-des-commits.md) | L'attribution d'un commit à un agent est un journal local | — |
| [0015](./0015-ash-compose-l-utilisateur-envoie.md) | Ash compose, l'utilisateur envoie | — |

## Dépendances

```
0001 application graphique
 └─ 0002 Tauri + Rust
     └─ 0003 un seul terminal à la fois

0004 workspace = racine git
 ├─ 0005 sonde cwd (libproc)
 │   └─ 0006 découverte automatique des agents
 │       └─ 0007 états par hooks
 │           └─ 0008 trait Adapter
 └─ 0012 worktree = unité de travail        (amende 0004)

0011 git domaine de premier plan
 ├─ 0012 worktree = unité de travail
 │   └─ 0013 fiche de branche dans le dépôt
 ├─ 0014 attribution locale des commits     (amende 0009)
 └─ 0015 ash compose, l'utilisateur envoie  (amende 0010)

0009 cycle de vie          (la plus révisable)
0010 sidebar informe
```

## Les deux règles transverses

Elles traversent plusieurs ADR et se citent plus souvent qu'elles ne se décident :

- **Bloc délimité, sauvegarde, jamais silencieux.** Partout où Ash écrit dans un
  fichier de l'utilisateur — `settings.json` (0007), `<!-- ash:log -->` (0013) — il
  écrit entre marqueurs, sauvegarde avant, et refuse d'écraser une édition manuelle.
- **On voit ce qui va partir avant que ça parte.** Vaut pour les hooks (0007) comme
  pour les prompts composés (0015).
