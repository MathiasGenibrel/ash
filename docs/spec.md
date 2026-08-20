# Ash — Spécification

> Statut : brouillon issu de la session de cadrage du 2026-08-07,
> **révisé le 2026-08-10 après la direction visuelle**.
> Les décisions structurantes sont tracées séparément dans [`docs/adr/`](./adr/).

---

## 1. Objet

Ash est une application macOS qui **entoure** un shell plutôt que de le remplacer.
L'utilisateur lance ses agents de code (`claude`, `claude-perso`, `codex`, `kimi`,
`opencode`, …) exactement comme il le fait aujourd'hui, dans un vrai bash. Ash
apporte trois choses par-dessus :

1. une **navigation** : dépôts, worktrees et onglets, pilotables au clavier comme à
   la souris ;
2. une **supervision** : savoir en permanence, pour chaque agent en cours, s'il
   travaille, s'il attend une réponse, ou s'il a terminé ;
3. un **git conscient des agents** : les opérations git dont la présence d'un agent
   change ce qu'il faut en dire ou en faire
   ([ADR-0011](./adr/0011-git-domaine-de-premier-plan.md)).

### Non-objectifs

- Ash n'est **pas** un client d'API ni une UI de chat. Il n'envoie pas de prompts,
  ne gère pas de conversation, ne connaît pas les modèles.
- Ash n'est **pas** un multiplexeur de sessions distantes. Pas de detach, pas de SSH,
  pas de partage de session (voir [ADR-0009](./adr/0009-cycle-de-vie-des-agents.md)).
- Ash n'est **pas** un gestionnaire de configuration des outils. Il lit leurs
  configs pour y poser ses hooks, il ne les administre pas.
- Ash n'est **pas** un client git complet. Zone de préparation, écriture d'un commit,
  remotes, tags, `stash` et configuration restent dans le terminal : un agent qui
  tourne n'y change rien ([ADR-0011](./adr/0011-git-domaine-de-premier-plan.md)).
- Ash ne **valide rien** à la place de l'utilisateur. Il peut rédiger un texte dans un
  terminal ; il ne presse jamais `⏎`
  ([ADR-0010](./adr/0010-sidebar-informe-terminal-agit.md),
  [ADR-0015](./adr/0015-ash-compose-l-utilisateur-envoie.md)).

---

## 2. Vocabulaire

| Terme | Définition |
|---|---|
| **Onglet** | Une surface sélectionnable depuis la sidebar (§4.2 — il n'y a pas de barre d'onglets). Un onglet **shell** porte un PTY ; un onglet **outil** (merge) n'en a pas. |
| **Worktree** | Un arbre de travail git — le dossier dans lequel on est. Unité de rattachement des onglets. Émergent, pas déclaré. |
| **Dépôt** | Le groupe qui réunit les worktrees d'un même projet. Nœud d'affichage, sans onglets en propre. |
| **Agent** | Un onglet shell dont le processus en avant-plan est un outil reconnu. Un onglet devient un agent, puis redevient un shell. |
| **Subagent** | Une tâche déléguée *à l'intérieur* d'un agent. N'a pas de PTY propre. |
| **Adaptateur** | Le composant qui sait traduire l'activité d'un outil donné en états Ash. Un adaptateur par outil. |
| **Commande reconnue** | Un nom d'exécutable déclaré dans la configuration, associé à un adaptateur et à un dossier de configuration. |
| **Fiche de branche** | `.ash/worktree.md` — l'intention d'un worktree, versionnée avec sa branche. |

Note : les deux abonnements Claude de l'utilisateur sont déjà séparés en amont par
deux commandes distinctes dans le `PATH` (`claude` et `claude-perso`). Ash n'a donc
**pas** de notion de profil : il a une liste de commandes reconnues, chacune avec
son propre dossier de configuration.

Le mot « workspace » de la version précédente est abandonné : il désigne désormais un
worktree ([ADR-0012](./adr/0012-worktree-unite-de-travail.md)).

---

## 3. Modèle de données

```
Session (runtime)
├── Repo*                 clé = chemin du répertoire git commun
│   ├── name              "omelette-web"
│   ├── path              /Users/mathias/dev/omelette-web/.git
│   └── worktrees → Worktree*
│
├── Worktree*             clé = chemin absolu de la racine de l'arbre de travail
│   ├── path              /Users/mathias/dev/wt/omelette-web-sidebar
│   ├── suffix            "sidebar"       (affiché ·sidebar)
│   ├── is_main           bool
│   ├── vcs               Vcs | null
│   ├── pinned            bool            (persisté)
│   ├── collapsed         bool            (persisté)
│   └── tabs → Tab*
│
└── Tab*                  clé = ASH_TAB_ID (ulid)
    ├── kind              Shell { … } | Merge { … }
    ├── title             nom affiché
    └── scrollback        buffer xterm.js       (Shell uniquement)

Tab::Shell
├── pty                   handle portable-pty
├── shell_pid             pid du bash
├── fg_pid                pid du process en avant-plan (sondé)
├── cwd                   sondé (libproc)
└── agent                 Agent | null

Tab::Merge                (pas de PTY — ADR-0003)
├── operation             rebase | merge | cherry-pick
├── files → ConflictFile* { path, hunks, resolved }
└── orig_head             sha de secours

Vcs
├── branch                "feat/agent-sidebar" | détaché
├── upstream              { ahead, behind } | null
├── tree                  { added, modified, untracked }
└── operation             Rebase { onto, step, total } | Merge { … } | null

Agent
├── command               "claude" | "claude-perso" | "codex" | …
├── adapter               id de l'adaptateur résolu
├── state                 idle | working | waiting | done | error
├── since                 instant du dernier changement d'état
├── detail                texte court optionnel ("2 options", "build failed")
└── subagents → Subagent* { label, state, since }
```

### 3.1 Ce qui est persisté

| Où | Quoi | Décision |
|---|---|---|
| `~/.ash/state.json` | worktrees épinglés, état replié | — |
| `~/.ash/journal/<repo>.jsonl` | attribution commit → agent → prompt | [ADR-0014](./adr/0014-attribution-locale-des-commits.md) |
| `<worktree>/.ash/worktree.md` | la fiche de branche, dans le dépôt | [ADR-0013](./adr/0013-fiche-de-branche-dans-le-depot.md) |

La règle : **Ash persiste ce que les agents ont fait, jamais ce qu'ils étaient en train
de faire.** Aucune session, aucun scrollback, aucun état d'agent en cours ne survit à
la fermeture ([ADR-0009](./adr/0009-cycle-de-vie-des-agents.md)).

---

## 4. Interface

Fenêtre unique, deux colonnes, **pas de splits de terminaux**
([ADR-0003](./adr/0003-zone-terminal-unique.md)).

```
┌─ bande de titre ─────────────────────────────────────────────┐
│       ash — omelette-web / feat/agent-sidebar                │
├─ sidebar (≈240px) ──┬─ terminal ─────────────────────────────┤
│ workspaces          │ $ claude                               │
│ 1 waiting / 7 agents│ > implémente la sonde cwd              │
│                     │ ⁘ Baking… (15m22s · esc to interrupt)  │
│ ▾ omelette-web      │                                        │
│   3 worktrees       │                                        │
│   ▾ feat/agent-side…│                                        │
│      ·sidebar       │                                        │
│     ● claude 15m22s │                                        │
│     ○ bun dev  idle │                                        │
│   ▸ main      ·web  │                                        │
│   ▸ fix/toc   ·toc  │                                        │
│                     │                                        │
│ ▾ ash-core     main ├─ panneau bas (repliable) ──────────────┤
│   ◉ codex  waiting  │ graph │ worktrees · 4 │ conflicts      │
│ + tab            ⌘T │ …                                      │
├─────────────────────┴────────────────────────────────────────┤
│ ~/dev/wt/…-sidebar │ feat/agent-sidebar +3 ~1 ⌘⌃B │ claude · │
│                                              working 15m22s  │
└──────────────────────────────────────────────────────────────┘
```

### 4.1 Sidebar

- En-tête : un **compteur agrégé** — `1 waiting / 7 agents` — qui reste visible quand
  la sidebar est repliée.
- Liste de dépôts, chacun repliable. Un dépôt sans worktree lié s'affiche **à plat**,
  sans niveau intermédiaire.
- Sous un dépôt : ses worktrees, chacun portant sa branche et le suffixe de son
  dossier (`·sidebar`, `·toc`), plus l'état de l'arbre (`+3 ~1`, `↑2 ↓1`) ou
  l'opération en cours (`rebasing onto main · 2/5`).
- Dépliés : les onglets du worktree, avec pastille d'état, libellé court, durée.
- Sous un agent : ses subagents, en retrait, plus discrets, **non cliquables**
  ([ADR-0008](./adr/0008-abstraction-adapter.md) pour leur provenance).
- **Remontée d'état** : une ligne repliée porte l'état le plus urgent de ses enfants.
  `waiting` l'emporte sur tout le reste, puis `error`, puis `working`. Une ligne
  repliée ne doit jamais cacher un agent qui attend.
- Un worktree épinglé reste affiché même sans onglet ; le clic y ouvre un onglet.
- Repliable entièrement (la colonne disparaît, le terminal prend toute la largeur).

### 4.2 Zone terminal

- Affiche l'onglet sélectionné, et lui seul.
- **Pas de barre d'onglets** — amendé le 2026-08-17. La barre dessinée par la
  maquette a été construite au J1, puis retirée : à l'usage, elle ne dit rien que
  la sidebar ne dise mieux. La sidebar range les onglets sous leur worktree et
  leur dépôt (§4.1), marque l'actif, porte leur état et leur durée ; la barre les
  remettait à plat, sans contexte, et son libellé — le nom du processus en
  avant-plan — était le moins lisible des deux.
  - Ce que la barre portait, et où c'est passé : sélectionner un onglet → la
    sidebar ; en ouvrir un → le `+` de son pied et `⌘T` ; fermer, ouvrir à `~`,
    effacer le scrollback → le menu natif, qui les portait déjà. La règle de
    §4.4 est donc tenue sans elle : ces actions restent atteignables à la souris.
  - **L'onglet outil du merge** (§7.4) n'a plus de barre où figurer. Il devient
    une ligne de la sidebar, sous le worktree dont le rebase s'est arrêté, avec
    son compte restant (`merge · 2`). C'est sa place naturelle : l'opération
    appartient à un worktree, pas à la fenêtre. À réaliser avec #30, pas avant.
- **Bande de titre** : la seule prise de la fenêtre porte, centré,
  `<application> — <dépôt> / <branche>` de l'onglet actif. C'est le contexte que
  la barre d'onglets ne portait qu'à demi, et il reste visible sidebar repliée
  (`⌘B`) — ce qui remplace le repli qui faisait grossir le libellé d'un onglet.
  - **Le premier mot est le nom de l'application, pas un mot de la maquette** —
    amendé le 2026-08-17. La maquette écrivait `ash` en minuscules ; ce mot est
    abandonné, parce qu'il aurait fait deux noms pour une même application. Ce
    qui s'écrit est `APP_NAME`, seule source du nom affiché : donc `Ash` dans
    l'application installée, et `Ash-dev` dans une compilation de développement.
    Ce n'est pas un effet de bord mais le but — Ash est le terminal quotidien de
    son auteur, une instance installée tourne pendant qu'on en développe une
    autre, et la bande de titre est l'endroit où l'œil les sépare. La fenêtre de
    réglages suit la même règle : `settings — <application>`.
- Ligne de statut en bas : `cwd` · branche et état de l'arbre · état de l'agent.
  La branche y est cliquable et ancre le popup de branches (`⌘⌃B`).
  - **L'usage est à sa droite** — amendé le 2026-08-20. Quatre morceaux, dans cet
    ordre, chacun affiché **seulement si sa donnée existe** : le quota de session
    (`s 63% · 2h14`), le quota hebdomadaire (`w 28% · 3d 09h`), la jauge de
    contexte de la conversation, et son libellé (`ctx 41%`). La jauge et son
    libellé passent en `--ash-warning` à 70 % puis en `--ash-accent` à 90 % —
    et **rien d'autre ne se produit** à ces seuils : ni alerte, ni modale, ni
    bannière. Un contexte plein annonce un compactage, pas une panne.
  - **Deux rythmes, et une seule ligne.** La jauge suit l'**onglet** : elle
    arrive avec sa fiche, et change quand on en change. Les deux quotas sont
    ceux du **compte** : ils ne dépendent d'aucune sélection, arrivent par
    `ash://account-usage`, et changer d'onglet ne les touche pas.
  - **Le weekly est masqué par défaut dans la barre**, et le clic gauche sur une
    pastille ouvre un popover de 248 px qui montre les **deux** quotas — c'est
    ce qu'il sert à révéler. Le menu contextuel « show in the status bar » de la
    maquette (vue 5c), qui rendrait ces défauts réglables, n'est **pas** livré :
    la ligne porte les défauts sans le moyen de les changer. Le `⌘⌥U` écrit au
    pied du popover est un **indice** : la vue d'usage complète n'existe pas, et
    aucune liaison n'est réclamée pour cette combinaison (§4.4).
  - Une valeur qu'Ash n'a pas **disparaît** — ni zéro, ni tiret, ni dernière
    valeur connue, et aucune erreur signalée : l'écran ne sait pas laquelle des
    raisons s'applique ([ADR-0016](./adr/0016-ash-sort-sur-le-reseau.md),
    condition 3). Un onglet dont l'outil est muet rend donc exactement la ligne
    d'avant.
  - **Ce qui traverse est une date, jamais un décompte** : les `2h14` et
    `3d 09h` sont dérivés à l'affichage de `resetsAt`, comme la durée d'état
    l'est de `stateSince`. La durée de la fenêtre — les cinq heures écrites dans
    le popover — n'est une donnée nulle part : c'est un libellé.
  - Quand la ligne se resserre, les segments se retirent dans un ordre défini —
    les quotas d'abord, la jauge et son libellé ensuite. Le `cwd`, la branche et
    l'état de l'agent ne se retirent jamais.
- Le rendu est délégué à xterm.js. Le terminal doit rester pleinement fonctionnel
  pour les TUI plein écran (c'est le cas de tous les outils visés).
- **Police par défaut : JetBrains Mono**, embarquée avec l'application — pas chargée
  depuis un CDN, et pas supposée présente sur la machine. L'utilisateur peut en choisir
  une autre parmi les monospace installées (§9). Le design system livré avec la
  direction visuelle embarque Geist Mono ; c'est JetBrains Mono qui est retenue.

### 4.3 Panneau bas

Un panneau repliable sous la zone terminal, à hauteur réglable, qui **ne contient
jamais de terminal** ([ADR-0003](./adr/0003-zone-terminal-unique.md)). Trois vues :
`graph`, `worktrees`, `conflicts` — plus la fiche de branche.

Il rend sa hauteur au terminal en se repliant. Le redimensionnement à chaud d'un PTY
sous une TUI plein écran est un point à vérifier au jalon J5.

### 4.4 Raccourcis

| Raccourci | Effet |
|---|---|
| `Cmd+1` … `Cmd+9` | Sélectionne le n-ième onglet (ordre d'affichage de la sidebar) |
| `Ctrl+Tab` | Onglet suivant, en bouclant après le dernier |
| `Ctrl+Shift+Tab` | Onglet précédent, en bouclant avant le premier |
| `Cmd+T` | Nouvel onglet dans le worktree courant |
| `Cmd+Shift+T` | Nouvel onglet à `~` (donc, jusqu'au premier `cd`, un worktree `~`) |
| `Cmd+W` | Ferme l'onglet (confirmation si un agent y tourne) |
| `Cmd+B` | Replie / déplie la sidebar |
| `Cmd+K` | Efface le scrollback de l'onglet courant |
| `Cmd+Ctrl+B` | Popup de branches |
| `Cmd+Ctrl+G` | Affiche / masque le graphe |
| `Cmd+Ctrl+W` | Worktrees |
| `Cmd+Ctrl+M` | Onglet de merge — seulement pendant un rebase ou un merge arrêté |
| `Cmd+Ctrl+I` | Fiche de branche |

`Cmd+T` ouvre un onglet, et non `Cmd+N` — amendé le 2026-08-12. Sur macOS, `Cmd+N`
ouvre une **fenêtre** partout ailleurs, et `Cmd+T` un onglet : Safari, Terminal.app,
iTerm et Chrome sont d'accord entre eux. Ash n'a pas de seconde fenêtre de terminal à
offrir, donc `Cmd+N` et `Cmd+Shift+N` ne font plus rien du tout, plutôt que de rester
des doublons — deux gestes pour une action, c'est celui qu'on ne connaît pas qui gagne.

`Cmd+1`…`Cmd+9` s'arrête toujours à neuf, et c'est justement pourquoi `Ctrl+Tab`
existe : au-delà du neuvième onglet, ou simplement pour aller voir à côté, il n'y avait
rien. Les deux sens bouclent, sans quoi il faudrait regarder où l'on est avant de savoir
si le raccourci va faire quelque chose. C'est la convention des navigateurs et d'iTerm2.

`Ctrl+Tab` est la seule de ces touches dont le terminal a un usage propre — `Tab` seul
complète dans `zsh`. Elle n'est retenue que si `Control` est enfoncé, et sans `Cmd` ni
`Option` : `Tab` nu, `Cmd+Tab` (le commutateur d'applications de macOS) et `Option+Tab`
partent au shell inchangés.

Le groupe git utilise `Cmd+Ctrl` parce que ces cinq lettres sont libres sur macOS et
ne sont pas interceptées par le terminal, contrairement à `Ctrl+B` seul que tmux
réclame. Mnémonique : **B**ranches, **G**raph, **W**orktrees, **M**erge, **I**nfo.
Attention en cas de rebinding : `Cmd+Ctrl+F`, `Cmd+Ctrl+D` et `Cmd+Ctrl+Space` sont,
eux, pris par le système.

**« Seulement pendant un rebase ou un merge arrêté » veut dire : l'entrée de menu est
éteinte** — écrit le 2026-08-20, avec l'issue #32. C'est la forme de macOS, celle que
`validateMenuItem:` produit partout ailleurs sur le système : « Resolve Conflicts » reste
à sa place dans le menu Git, grisée, et son équivalent clavier ne s'allume pas. Les deux
autres formes ont été écartées — laisser l'accélérateur posé et refuser au moment du geste
aurait annoncé un raccourci qui ne fait rien, et retirer l'entrée du menu l'aurait fait
scintiller au rythme des rebases, en escamotant une ligne que l'utilisateur a peut-être
rebindée. Le prix est que le menu apprend deux choses qu'il ne savait pas : quel worktree
est sous les yeux — la fenêtre le rapporte, elle seule le sait — et si quelque chose y est
arrêté, qu'il **demande** à `features::merge`, par la lecture même que l'ouverture consulte.
La surveillance de `.git` rouvre la question quand un rebase commence ou se termine, sans
quoi le raccourci ne s'allumerait qu'au prochain changement d'onglet.

**Un raccourci est un caractère, pas une position de touche** — écrit le 2026-08-19, après
l'issue #133. Ces combinaisons se lisent comme macOS les apparie : `Cmd+W` est la touche qui
produit `w`, où qu'elle se trouve sur le clavier. Changer de disposition peut donc déplacer un
raccourci d'une touche à l'autre — sur un AZERTY, `Cmd+W` se frappe à la première position de
la rangée du haut, et `Cmd+Q` à celle où un QWERTY a le `A`. C'est la convention du système, et
toutes ses applications s'y tiennent ; retenir la position aurait donné un raccourci que la
touche pressée ne joue pas.

Toutes ces actions doivent être également atteignables à la souris.

---

## 5. Dépôts et worktrees

### 5.1 Résolution

Pour chaque onglet shell, à chaque cycle de sonde :

1. lire le `cwd` du processus en avant-plan du PTY ;
2. remonter jusqu'à trouver un `.git` → c'est la racine du **worktree** ;
3. si ce `.git` est un **fichier**, lire son `gitdir:` puis le `commondir` associé →
   c'est le **dépôt** ; si c'est un dossier, le dépôt est le worktree lui-même ;
4. à défaut de `.git`, le worktree est le `cwd` lui-même, sans dépôt.

L'onglet est rattaché au worktree ainsi résolu, et **migre** si le `cwd` en change
([ADR-0004](./adr/0004-workspace-racine-git.md),
[ADR-0012](./adr/0012-worktree-unite-de-travail.md)).

### 5.2 Cycle de vie

- Un worktree existe dans la sidebar tant qu'il a au moins un onglet, **ou** qu'il est
  épinglé, **ou** que `git worktree list` le déclare pour un dépôt déjà affiché.
- Un dépôt existe tant qu'il a au moins un worktree affiché.
- Épingler / désépingler est une action manuelle, au niveau du worktree.
- Les worktrees épinglés (et leur état replié) survivent au redémarrage.
- Aucun historique automatique des dossiers visités.

### 5.3 Métadonnées git

`branch`, `tree`, `upstream` et `operation` sont rafraîchies :

- au rattachement d'un onglet à un worktree,
- sur `focus` de la fenêtre,
- sur modification de `.git/HEAD`, `.git/refs`, `.git/rebase-merge` ou
  `.git/MERGE_HEAD` (surveillance de fichiers, pas de sondage),
- au plus une fois toutes les 5 s par worktree.

Un `git status` par cycle de sonde est exclu : le coût est trop élevé sur `n` dépôts.

### 5.4 Worktree obsolète

Un worktree sans agent depuis plus de 3 jours **et** portant des fichiers modifiés est
signalé `stale` dans le tableau. Ash le **signale**, ne le supprime jamais.

La suppression d'un worktree est une action explicite, qui doit énoncer ce qu'elle
emporte (fichiers modifiés, agent en cours) avant de le faire.

---

## 6. Détection des agents

### 6.1 Découverte

Ash ne demande pas à l'utilisateur de déclarer ce qu'il lance
([ADR-0006](./adr/0006-decouverte-automatique-des-agents.md)).

Boucle de sonde, ~300 ms par onglet shell :

```
fg_pgid   = tcgetpgrp(pty_master)
fg_proc   = nom de l'exécutable de fg_pgid
cwd       = proc_pidinfo(fg_pgid, PROC_PIDVNODEPATHINFO)   # macOS libproc
```

- `fg_proc` figure dans les commandes reconnues → l'onglet **devient** un agent, et
  la sonde suffit à le montrer `working`.
- `fg_proc` redevient le shell → l'agent passe en `done` ou en `error` selon son code
  de sortie, puis la ligne redevient un simple onglet shell après un délai d'affichage
  (voir §6.4).

**La découverte reste bornée, et Ash ne demandera pas Full Disk Access pour l'élargir.**
Le [spike #62](./spike-bounded-discovery.md) a mesuré les quatre outils visés : un seul
range sa configuration au-delà du premier niveau de `$HOME` — `~/.config/opencode` —, et
c'est une constante documentée, donc une ligne dans la table des adaptateurs et non une
permission. Aucun des quatre n'écrit dans un emplacement protégé par TCC : les 156 refus
relevés en parcourant `$HOME` en entier sont tous des données personnelles Apple (Mail,
Messages, Safari, Contacts), c'est-à-dire exactement ce que Full Disk Access débloquerait
et rien de ce qu'on cherche. Le parcours coûte 74 s pour 614 917 dossiers contre 30 ms
pour seize emplacements connus, et rend une réponse plus mauvaise. Le seul cas qui
échappe à la table — un dossier déplacé par `CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
`KIMI_CODE_HOME` ou `OPENCODE_CONFIG_DIR` — se lit dans l'environnement du processus,
sans aucune permission ; il ne se cherche pas sur le disque.

### 6.2 États

```
                    ┌──────────────────────────────┐
                    │                              │
   idle ──lancement──▶ working ──question──▶ waiting
                       │    ▲                  │
                       │    └──réponse─────────┘
                       │
             ┌─fin─────┴─────échec─┐
             ▼                     ▼
           done                  error
             │                     │
             └──▶ (retour shell) ◀─┘
```

`done` et `error` sont les deux issues **exclusives** d'une même terminaison : on ne
passe jamais de l'un à l'autre. Les deux mènent au retour à `idle` (§6.4).

| État | Sens | Source |
|---|---|---|
| `idle` | shell sans agent | sonde |
| `working` | un agent tient l'avant-plan | sonde, ou hook |
| `waiting` | l'agent attend une réponse de l'utilisateur | **hook, et rien d'autre** |
| `done` | l'agent a rendu la main | hook, ou disparition avec un code 0 |
| `error` | l'agent s'est terminé anormalement | hook, ou code de sortie non nul |

`working` a **deux** producteurs, et c'est voulu : la sonde répond à une question de
présence — quelque chose d'autre que le shell tient l'avant-plan — tandis que le hook
répond de ce que l'agent fait. Un outil sans instrumentation n'a que le premier, et
reste utilisable. Voir la précision du 2026-08-11 dans
[ADR-0007](./adr/0007-etats-par-hooks.md) et l'amendement d'
[ADR-0008](./adr/0008-abstraction-adapter.md).

`waiting` est l'état qui compte : c'est le seul qui justifie d'interrompre
l'utilisateur.

### 6.3 Provenance des états

Les états viennent des **hooks de l'outil**, pas d'une analyse de la sortie
([ADR-0007](./adr/0007-etats-par-hooks.md)).

- Ash écrit ses propres entrées, chacune marquée, dans le `settings.json` de chaque
  commande reconnue — à côté de celles que l'utilisateur y a déjà mises
  ([ADR-0007](./adr/0007-etats-par-hooks.md), amendement du 2026-08-12).
- Les hooks appellent un petit binaire `ash-event` qui poste sur `$ASH_SOCK`.
- La corrélation hook → onglet se fait par `ASH_TAB_ID`, variable d'environnement
  posée par Ash à la création du bash et héritée par toute la descendance.

```
Ash ──spawn──▶ bash(ASH_TAB_ID=01J..., ASH_SOCK=~/.ash/ash.sock)
                 └─▶ claude
                       └─▶ hook: ash-event working --tab $ASH_TAB_ID
                                    │
Ash ◀──unix socket──────────────────┘
```

`ash-event <state> --tab <id>` est la **forme canonique**, celle qu'Ash écrit dans le
bloc de hooks. C'est elle que le `settings.json` de l'utilisateur portera.

Le socket vit dans `~/.ash/`, avec `config.toml` et `theme.json`, et non dans `/tmp`.
Le suffixe `<uid>` que dessinait la première rédaction n'existait que pour contourner le
fait que `/tmp` est partagé — un problème qu'on peut ne pas avoir. Un dossier personnel
en `0700` ferme en outre la fenêtre entre le `bind` et la pose du `0600` sur le socket ;
sur `/tmp`, cette fenêtre reste ouverte. Il survit enfin au nettoyage de `/tmp`.

### 6.4 Règles de transition

- Un événement de hook fait autorité sur la sonde.
- Un agent en `working` y reste tant qu'un hook ou la disparition du processus n'en
  décide autrement. **Aucun silence, si long soit-il, ne change son état** — un agent
  met couramment bien plus d'une minute à faire une tâche, et Ash ne devine pas. La
  sonde ne dit que la **présence** et la **disparition** du processus, jamais ce que
  l'agent fait.

  La rédaction initiale chiffrait ce silence à 60 s. Le seuil ne déclenchait rien —
  la règle est une **interdiction**, pas un minuteur — et un nombre dans une spec
  finit par se lire comme un déclencheur qu'il resterait à implémenter.
- Quand le processus disparaît sans événement `done` : `done` si code 0, `error` sinon.
- Une ligne `done`/`error` reste visible 30 s dans la sidebar, puis l'onglet
  redevient une ligne shell `idle`. Elle reste visible indéfiniment si la fenêtre
  Ash n'a pas eu le focus depuis le passage en `done`.

### 6.5 Subagents

Un subagent est une tâche déléguée dans le processus de l'agent parent. Il n'a ni
PTY ni sortie séparable. Ash l'affiche comme une ligne fille informative — libellé,
état, durée — et **rien de plus** : la ligne n'est pas cliquable, le clic sélectionne
le parent. Pour lire ce qu'a fait un subagent, on scrolle le terminal du parent.

---

## 7. Git

Périmètre et justification : [ADR-0011](./adr/0011-git-domaine-de-premier-plan.md).
Ash n'intègre une opération que si la présence d'agents change ce qu'il faut en dire
ou en faire.

### 7.1 Popup de branches — `⌘⌃B`

Ancré sur la branche du pied de fenêtre. Une seule liste, filtrée en tapant, groupée
`current` / `recent` / `local` / `remote` — la branche courante en tête, pas rangée
dans l'ordre alphabétique.

Deux ajouts par rapport à un popup de branches classique :

- une colonne de droite qui nomme le **worktree** quand la branche vit ailleurs ;
- un avertissement nommant **l'agent qui travaille** dans ce worktree, parce qu'un
  checkout déplacerait des fichiers sous ses pieds.

`⌘⏎` ouvre le sous-menu d'actions sans quitter le clavier. Les actions y nomment leurs
deux côtés — « Rebase feat/agent-sidebar onto main », jamais « Rebase » — y compris
dans les messages d'erreur. Les actions qui touchent l'arbre de travail pendant qu'un
agent écrit sont marquées, et déclenchent une confirmation qui propose de mettre
l'agent en pause ([ADR-0015](./adr/0015-ash-compose-l-utilisateur-envoie.md) pour ce
que « pause » veut dire exactement).

### 7.2 Graphe — `⌘⌃G`

Vue du panneau bas. Couloirs calculés côté Rust ; quatre suffisent à la plupart des
dépôts, au-delà Ash replie les branches inactives depuis plus de 30 jours.

La colonne `by` est la raison d'être de l'écran : elle nomme **l'agent** qui a écrit le
commit, et le panneau de détail garde le **prompt** qui l'a produit
([ADR-0014](./adr/0014-attribution-locale-des-commits.md)). Un commit sans attribution
connue affiche simplement son auteur git.

### 7.3 Worktrees — `⌘⌃W`

Tableau : worktree, branche, `agents now`, `last worked by`, état de l'arbre, fiche.
Les deux colonnes du milieu sont celles que `git worktree list` ne donne pas ; Ash les
connaît parce qu'il connaît le `cwd` de chaque onglet.

L'état le plus utile du tableau est `done · waiting for your review` : un agent a fini,
personne n'a regardé.

### 7.4 Conflits — `⌘⌃M`

Quand un rebase ou un merge s'arrête, Ash affiche l'opération, les fichiers en conflit,
le `ORIG_HEAD` de secours, et **ne touche à rien de lui-même**. Deux routes, non
exclusives :

- **Passer à l'agent** — Ash rédige dans l'onglet de l'agent un prompt portant les
  chemins, le commit d'arrêt et la commande de test. Il ne l'envoie pas
  ([ADR-0015](./adr/0015-ash-compose-l-utilisateur-envoie.md)).
- **Résoudre dans Ash** — un onglet de merge à trois panneaux, hunk par hunk. Les côtés
  portent le **nom de leur branche**, pas le jargon `ours`/`theirs` de git, qui
  s'inverse en rebase. Le panneau central reste éditable. `continue` reste visible mais
  éteint tant qu'il reste des conflits, avec le compte à sa droite.

L'onglet de merge se ferme sans rien perdre : l'état vit dans l'index git, pas dans
Ash. `abort` et `skip` restent visibles avant d'entrer.

### 7.5 Fiche de branche — `⌘⌃I`

`.ash/worktree.md`, rendu à gauche et source à droite, markdown standard
([ADR-0013](./adr/0013-fiche-de-branche-dans-le-depot.md)). Front matter pour les
métadonnées, `- [ ]` pour la progression, tableaux, clôtures `mermaid`.

Ash n'écrit que dans le bloc `<!-- ash:log -->` — même régime que les hooks : bloc
délimité, sauvegarde avant écriture, refus d'écrire si le bloc a été modifié à la main.

---

## 8. Notifications

- **Toujours** : la ligne change d'état dans la sidebar, et le compteur agrégé de
  l'en-tête est visible même sidebar repliée.
- **Si Ash n'est pas au premier plan** : notification macOS pour `waiting` et
  `error`. Le clic sélectionne l'agent concerné.
- **Jamais** : sélection automatique d'un onglet, ni vol de focus clavier
  ([ADR-0010](./adr/0010-sidebar-informe-terminal-agit.md)).
- `done` ne notifie pas en v1 (à rediscuter à l'usage).
- L'écran de réglages doit exposer l'état « permission macOS non accordée », avec le
  chemin pour l'accorder.

---

## 9. Configuration

`~/.ash/config.toml`, éditable à la main **et** par l'écran de réglages.

```toml
[ui]
sidebar_width   = 240
sidebar_density = "comfortable"   # comfortable | compact
poll_interval_ms = 300

[appearance]
theme     = "system"              # system | light | dark
font      = "JetBrains Mono"      # défaut ; liste des monospace installées
font_size = 13

[[command]]
match   = "claude"
label   = "Pro"                   # libellé d'affichage, optionnel
adapter = "claude-code"
config  = "~/.claude"

[[command]]
match   = "claude-perso"
label   = "Perso"
adapter = "claude-code"
config  = "~/.claude-perso"

[[command]]
match   = "codex"
adapter = "codex"

[[command]]
match   = "kimi"
adapter = "generic"        # aucun état fin, seulement idle/done/error

[notifications]
waiting = true
error   = true
done    = false
```

### 9.1 Vérification d'une entrée

Toute entrée est vérifiée avant qu'Ash n'écrive quoi que ce soit. Quatre tests, dans
cet ordre :

| # | Test | Coût |
|---|---|---|
| 1 | le dossier existe et est lisible | instantané |
| 2 | il porte la signature de l'adaptateur (`settings.json`, `projects/`, …) | instantané |
| 3 | la commande existe dans le `PATH` et répond | instantané |
| 4 | la commande, lancée avec ce dossier, l'utilise réellement | lance la commande |

Le résultat arrive donc **en deux temps** : les tests 1 à 3 immédiatement, le test 4
ensuite. Cinq états possibles : *non vérifié*, *vérification en cours*, *valide*,
*valide avec réserve*, *invalide*. Un état invalide dit **quel** test a échoué, ce qui
était attendu, ce qui a été trouvé, et propose la correction qui a une chance.

La vérification se relance automatiquement 400 ms après la dernière frappe, ou
immédiatement sur `⏎`, et à chaque changement de chemin ou d'adaptateur.

Le test 4 mérite une précision : c'est **à l'utilisateur** d'avoir une commande liée à
son dossier de configuration (`claude-perso` pointant sur `~/.claude-perso`). Ash ne
lance pas l'outil en usage normal et ne peut pas lui passer de variable
([ADR-0006](./adr/0006-decouverte-automatique-des-agents.md)) : le test ne fait que
vérifier que le couple tient.

Seuls *valide* et *valide avec réserve* autorisent l'écriture des hooks. Le bouton
d'installation reste alors **visible et éteint, avec sa raison** — jamais masqué.

Deux entrées pointant sur le même dossier ne sont pas une erreur système, mais l'une
des deux ne servira à rien. Le doublon est signalé **sur les deux lignes**.

**Réinitialiser une entrée la ramène à sa dernière valeur valide**, pas au défaut de son
adaptateur. Chaque entrée retient donc le dernier dossier qui a passé la vérification, et
c'est cette mémoire-là que le geste restaure.

La nuance décide du sens de tout l'écran. Deux entrées partagent souvent un adaptateur —
`claude` et `claude-perso` sont toutes deux en `claude-code` — donc un retour au défaut de
l'adaptateur les rendrait **identiques**, et le doublon signalé ci-dessus passerait
d'accident rare à conséquence mécanique du geste. Ramener chaque entrée là où *elle*
fonctionnait laisse le doublon exceptionnel, ce qu'il doit être.

Tant qu'une entrée n'a jamais été valide, elle n'a rien à restaurer : le bouton reste
**visible et éteint, avec sa raison**, comme celui de l'installation des hooks.

### 9.2 Fichiers écrits par Ash

`~/.ash/state.json` (worktrees épinglés, état replié) et
`~/.ash/journal/<repo>.jsonl` (attribution, cf. §3.1).

---

## 10. Empreinte sur le système

Ash est retirable **de la machine** sans laisser de traces. Il laisse en revanche des
fiches de branche dans les dépôts où il a servi — c'est assumé, et c'est le seul cas.

| Ce qu'Ash touche | Pourquoi | Réversible |
|---|---|---|
| `settings.json` de chaque commande reconnue | poser les hooks | Oui — une entrée par événement, chacune marquée et versionnée (`# ash:hook v1`), fusionnée avec les hooks déjà là, sauvegarde `.bak` avant écriture, désinstallation en un geste qui rend le fichier à l'octet près |
| `<worktree>/.ash/worktree.md` | la fiche de branche, committée avec la branche | Oui — suppression du fichier, mais **elle est passée dans l'historique git** ([ADR-0013](./adr/0013-fiche-de-branche-dans-le-depot.md)) |
| Environnement des bash qu'il crée | `ASH_TAB_ID`, `ASH_SOCK` | Oui — n'existe que dans les process enfants d'Ash |
| `~/.ash/` | config, état, journal d'attribution | Oui — suppression du dossier |
| **Rien d'autre** | pas de `.zshrc`, pas de `PATH`, pas de shim, pas de hook git dans le dépôt | — |

Le suivi du `cwd` se fait par sonde système précisément pour éviter de toucher à la
configuration du shell ([ADR-0005](./adr/0005-sonde-cwd-libproc.md)).

Le journal d'attribution contient des **prompts**. Il n'est ni synchronisé ni envoyé
nulle part, et doit être purgeable explicitement.

Conflit d'édition : si un bloc géré a été modifié à la main entre deux lancements,
Ash ne réécrit pas silencieusement — il signale, propose le diff, et demande. Cette
règle vaut pour les `settings.json` comme pour `<!-- ash:log -->`.

---

## 11. Jalons

| Jalon | Contenu | Critère de sortie |
|---|---|---|
| **J1 — Terminal** | Tauri + portable-pty + xterm.js, onglets, `Cmd+T`/`Cmd+Shift+T`/`Cmd+1..9`/`Ctrl+Tab`, sidebar groupée par dépôt et worktree, sonde cwd | Ash remplace le terminal quotidien de l'utilisateur. Aucun état d'agent. |
| **J2 — États** | Socket + `ash-event` + trait `Adapter` + adaptateur `claude-code` + installation des hooks + écran de réglages « Outils » | `working` / `waiting` / `done` fiables sur `claude` et `claude-perso` |
| **J3 — Attention** | Notifications macOS, subagents, compteur agrégé, remontée d'état | Un agent en `waiting` est vu en < 10 s même hors d'Ash |
| **J4 — Ouverture** | Épinglage, désinstallation propre, reste des réglages, colonne redimensionnable | **Atteint le 2026-08-20**, amendé : voir ci-dessous |
| **J5 — Git** | Panneau bas, popup de branches, graphe + journal d'attribution, tableau des worktrees, onglet de merge, fiche de branche | Un rebase en conflit se traite sans quitter Ash, et l'historique dit quel agent a écrit quoi |

**Amendement du 2026-08-20 — le critère de sortie de J4 a changé en cours de route.**
Il disait « un deuxième outil supporté sans toucher au cœur », et c'est l'adaptateur `codex`
(#21) qui le portait. L'enquête a été menée et son verdict est écrit
([`spike codex`](./spikes/codex-adapter.md)) : **le trait `Adapter` suffit tel quel**, un
adaptateur ne toucherait aucune ligne du cœur, et ADR-0008 tient. Mais l'implémentation a été
**sortie du jalon** — c'est un *must have* dont le moment n'est pas venu. J4 ferme donc sur ce
qu'il a livré, et la démonstration du critère reste à faire le jour où #21 sera repris.

Ce que J4 a livré, et qui n'était pas au programme d'origine : la **colonne redimensionnable**
par son bord (#129), et les **raccourcis réglables** — la source de vérité des liaisons est
passée du menu natif à un magasin persistant, dont le menu **dérive** (#22).

J5 pèse à lui seul autant que J1 à J4 réunis. Il vient **après** que la supervision
soit fiable : c'est elle qui donne sa valeur à la colonne `by` et à l'avertissement de
checkout, pas l'inverse.

**Risque à lever dès J1** : la performance de xterm.js sous WKWebView sur une sortie
verbeuse. À mesurer avant que le reste soit construit dessus.

---

## 12. Questions ouvertes

1. **Ce que codex, kimi et opencode exposent réellement.** Toute la conception des
   états repose sur des hooks, qui existent pour Claude Code. Pour les autres, on ne
   sait pas encore s'il y a un hook, un fichier de session exploitable, ou seulement
   de la sortie à parser. Si aucun n'existe, il faudra un adaptateur heuristique —
   explicitement écarté pour l'instant — et donc un second moteur d'état.
   *Nuance apportée par le design* : l'attribution des commits (§7.2) ne dépend que de
   la sonde, donc elle fonctionne pour tous les outils, y compris en `generic`.
2. **Performance du rendu** sous WKWebView (addon WebGL pas garanti), et
   redimensionnement à chaud d'un PTY quand le panneau bas s'ouvre sous une TUI.
3. **Durée d'affichage de `done`** : les 30 s de §6.4 sont un choix arbitraire.
4. **Onglets ouverts à `~`** : ils créent un worktree `~` jusqu'au premier `cd`.
   À valider à l'usage que ça ne pollue pas la sidebar.
5. **Ordre des onglets pour `Cmd+1..9`** : ordre d'affichage sidebar (qui bouge quand
   un onglet migre, et désormais sur trois niveaux) ou ordre de création (stable mais
   désaligné du visuel). Le niveau dépôt rend la question plus épineuse qu'avant.
6. **Le suffixe du worktree principal.** Deux worktrees ne peuvent pas être sur la même
   branche, donc la branche suffit à les distinguer — sauf pour le principal, à qui le
   design donne un suffixe tiré de son dossier (`·web`). Convention à valider.
7. **Le mode local de la fiche de branche.** Que fait Ash quand l'équipe ne veut pas de
   `.ash/` dans le dépôt ? Repli dans `~/.ash/worktrees/`, mais la fiche perd alors ce
   qui la justifie.
8. **Conflit sur `<!-- ash:log -->`.** La règle est qu'Ash ne le résout jamais seul,
   mais il faut vérifier à l'usage que ça n'arrive pas assez souvent pour être
   pénible.
