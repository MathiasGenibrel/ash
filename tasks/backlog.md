# Backlog — Ash

> Dérivé de `docs/spec.md`, des 15 ADR et de la direction visuelle.
>
> **Les 32 issues ont été créées le 2026-08-10** sur `MathiasGenibrel/ash`, réparties dans
> les six jalons GitHub `J0` → `J5`. **La numérotation est alignée : le ticket _n_ de ce
> fichier est l'issue `#n`.** À partir d'ici, la source de vérité est GitHub — ce fichier
> reste comme vue d'ensemble ordonnée, il n'est pas maintenu ticket par ticket.

Ordre = ordre de traitement. Les jalons de la spec §11 sont respectés : la supervision
doit être fiable avant que git ne soit construit dessus.

Légende de la colonne **Source** : `spec` = déjà spécifié avant le design ·
`design` = vient de la direction visuelle · `risque` = lève un risque identifié.

---

## J0 — Socle et levée du risque

### 1. Squelette Tauri et outillage

`chore` · spec §11 · [ADR-0002](../docs/adr/0002-tauri-rust-portable-pty.md) · source : spec

Créer le projet Tauri v2 vide qui compile et démarre, avec les feature folders des deux
côtés et les six commandes de vérification opérationnelles.

- [ ] `bun run tauri dev` ouvre une fenêtre vide
- [ ] `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `bun run lint`,
      `bun run typecheck`, `bun test` passent tous sur un projet vide
- [ ] l'arborescence est celle de `.claude/docs/architecture.md`, sans dossier vide créé
      « pour la forme »
- [ ] JetBrains Mono est **embarquée** dans le bundle, pas chargée depuis un CDN

Hors périmètre : tout PTY, toute UI.

---

### 2. Spike — performance de xterm.js sous WKWebView

`spike` · spec §11 et §12.2 · [ADR-0002](../docs/adr/0002-tauri-rust-portable-pty.md) · source : **risque**

**Ce ticket conditionne tous les autres.** La spec le désigne deux fois comme le risque à
lever en premier. Un `cat` d'un fichier de 100 000 lignes, un `bun test` verbeux, une TUI
qui se redessine : mesurer, et décider.

- [ ] mesure chiffrée du débit soutenu (lignes/s) et de la latence de frappe sous charge,
      avec et sans l'addon WebGL
- [ ] verdict écrit : xterm.js tient / tient avec réglages / ne tient pas
- [ ] si « ne tient pas », le ticket produit les options et leur coût — il ne les
      implémente pas

Hors périmètre : optimiser. Ce ticket mesure et rend un verdict.

**Ce spike peut invalider [ADR-0002](../docs/adr/0002-tauri-rust-portable-pty.md).** C'est
son intérêt. Le résultat s'écrit en amendement d'ADR, quel qu'il soit.

---

## J1 — Terminal

Critère de sortie du jalon : **Ash remplace le terminal quotidien.** Aucun état d'agent.

### 3. Un PTY dans un onglet

`feat` · spec §3, §4.2 · [ADR-0001](../docs/adr/0001-application-graphique-avec-pty-embarques.md) · source : spec

Un vrai `bash` dans un PTY, rendu par xterm.js, avec `ASH_TAB_ID` et `ASH_SOCK` posés dans
son environnement dès maintenant — même si rien ne les lit encore.

- [ ] un `bash` démarre, répond, et `htop` puis `vim` s'affichent sans casse
- [ ] redimensionner la fenêtre propage un `SIGWINCH` correct
- [ ] fermer l'onglet termine le processus et libère le PTY
- [ ] `echo $ASH_TAB_ID` rend un ulid

### 4. Onglets multiples et raccourcis

`feat` · spec §4.2, §4.4 · source : spec

- [ ] `Cmd+T`, `Cmd+Shift+T`, `Cmd+W`, `Cmd+1..9`, `Ctrl+Tab`, `Cmd+K` font ce que dit la
      spec §4.4
- [ ] `Cmd+W` demande confirmation si un processus tourne dans l'onglet
- [ ] chaque action est aussi atteignable à la souris
- [ ] un seul terminal visible à la fois ([ADR-0003](../docs/adr/0003-zone-terminal-unique.md))

### 5. Sonde `cwd` et processus en avant-plan

`feat` · spec §5.1, §6.1 · [ADR-0005](../docs/adr/0005-sonde-cwd-libproc.md) · source : spec

`tcgetpgrp` + `proc_pidinfo`, derrière un trait `Probe`, avec `unsafe` confiné à cette
feature.

- [ ] le `cwd` suit un `cd`, y compris pendant qu'un programme tourne, en moins de 400 ms
- [ ] le nom du processus en avant-plan est correct quand une TUI tourne
- [ ] tout `unsafe` du crate est dans `features/probe/`, derrière une fonction sûre
- [ ] les règles se testent avec un `FakeProbe`, sans lancer de processus
- [ ] aucun fichier de configuration shell n'est touché

### 6. Résolution worktree → dépôt

`feat` · spec §5.1 · [ADR-0012](../docs/adr/0012-worktree-unite-de-travail.md) · source : design

Le cas qui casse les implémentations naïves : dans un worktree lié, `.git` est un
**fichier**.

- [ ] un `cwd` dans un worktree lié résout le bon worktree **et** le bon dépôt commun
- [ ] un dépôt classique (`.git` dossier) résout à plat, sans niveau intermédiaire
- [ ] un `cwd` hors dépôt donne un worktree sans dépôt
- [ ] testé sur de vrais dépôts créés en dossier temporaire (`src-tauri/tests/`)

### 7. Sidebar — dépôts, worktrees, onglets

`feat` · spec §4.1 · [ADR-0012](../docs/adr/0012-worktree-unite-de-travail.md) · source : design

- [ ] les onglets sont groupés par worktree, les worktrees par dépôt
- [ ] un dépôt sans worktree lié s'affiche **à plat**
- [ ] deux worktrees du même dépôt se distinguent par leur suffixe (`·sidebar`, `·toc`)
- [ ] un `cd` vers un autre dépôt fait migrer l'onglet en moins d'une seconde
- [ ] `Cmd+B` replie la colonne, le terminal prend toute la largeur
- [ ] 15 onglets restent lisibles à 240 px

⚠️ Les écrans de sidebar de la direction visuelle (référencés `1x` / `2b`) n'ont pas été
retrouvés dans le projet de design. À fournir avant de commencer, ou ce ticket se fera
depuis la maquette ASCII de la spec §4.

### 8. Métadonnées git du worktree

`feat` · spec §5.3 · source : spec + design

Branche, `+3 ~1`, `↑2 ↓1`, et l'opération en cours (`rebasing onto main · 2/5`).

- [ ] rafraîchi au rattachement, sur focus de fenêtre, et sur modification de
      `.git/HEAD`, `.git/refs`, `.git/rebase-merge`, `.git/MERGE_HEAD`
- [ ] **aucun `git status` dans la boucle de sonde** — surveillance de fichiers
- [ ] au plus un rafraîchissement toutes les 5 s par worktree
- [ ] avec 5 dépôts ouverts, la consommation CPU au repos reste négligeable

### 9. Ligne de statut et thème

`feat` · spec §4.2 · source : design

- [ ] `cwd` · branche et état de l'arbre · état de l'agent (vide à ce stade)
- [ ] thèmes clair, sombre et système, avec les cinq états d'agent lisibles dans les deux
- [ ] JetBrains Mono partout, y compris dans les formulaires

---

## J2 — États

Critère de sortie : **`working` / `waiting` / `done` fiables sur `claude` et
`claude-perso`.**

### 10. Socket d'events et binaire `ash-event`

`feat` · spec §6.3 · [ADR-0007](../docs/adr/0007-etats-par-hooks.md) · source : spec

- [ ] `ash-event working --tab <id>` posté sur `$ASH_SOCK` arrive dans Ash
- [ ] la corrélation se fait par `ASH_TAB_ID`, jamais par `cwd` ni horodatage
- [ ] un event pour un `tab_id` inconnu est ignoré sans planter
- [ ] le socket est nettoyé à la fermeture

### 11. Trait `Adapter` et adaptateur `generic`

`feat` · spec §6 · [ADR-0008](../docs/adr/0008-abstraction-adapter.md) · source : spec

- [ ] le trait a les quatre méthodes de l'ADR-0008
- [ ] `generic` fonctionne : `idle` / `done` / `error` depuis la seule sonde
- [ ] le cœur ne connaît que `idle · working · waiting · done · error`
- [ ] une suite de **tests contractuels** existe, que toute implémentation doit passer

### 12. Machine à états et règles de transition

`feat` · spec §6.2, §6.4 · source : spec

Le cœur du produit. Entièrement en Rust, entièrement testable avec une horloge injectée.

- [ ] chaque transition du diagramme §6.2 a son test
- [ ] un hook fait autorité sur la sonde
- [ ] > 60 s sans event en `working` reste `working` — Ash ne devine pas
- [ ] disparition sans `done` : `done` si code 0, `error` sinon
- [ ] `done` reste visible 30 s, **indéfiniment** si la fenêtre n'a pas eu le focus depuis
- [ ] aucun `sleep` dans les tests

### 13. Adaptateur `claude-code` et pose des hooks

`feat` · spec §6.3, §10 · [ADR-0007](../docs/adr/0007-etats-par-hooks.md) · source : spec

Le premier endroit où Ash écrit dans un fichier de l'utilisateur.

- [ ] bloc délimité `ash:begin` / `ash:end`, versionné (`ash block v3`)
- [ ] `settings.json.bak` créé **avant** toute écriture
- [ ] rien n'est modifié hors marqueurs
- [ ] si le bloc a été édité à la main : Ash **refuse** d'écrire, signale, propose le diff
- [ ] `claude` et `claude-perso` fonctionnent en parallèle, deux dossiers, deux blocs
- [ ] désinstallation en un geste, qui ne laisse rien

### 14. Réglages — fenêtre, navigation, liste des outils

`feat` · design 3a / 3b / 3h · spec §9 · source : **design**

- [ ] fenêtre séparée, lisible à 800 × 600 exactement
- [ ] quatre sections, navigation au clavier (`tab`, `⌥↑↓`)
- [ ] ajout d'une entrée, état vide de la liste
- [ ] libellé d'affichage optionnel (`Pro`, `Perso`)
- [ ] clair et sombre

### 15. Réglages — vérification d'un chemin en 4 tests

`feat` · design 3c / 3d / 3e · spec §9.1 · source : **design**

L'écran que le brief demandait « à concevoir en priorité », et il l'a été.

- [ ] les 4 tests, dans l'ordre, avec le résultat en **deux temps** (1–3 puis 4)
- [ ] les 5 états sont distincts : non vérifié, en cours, valide, valide avec réserve,
      invalide
- [ ] un état invalide nomme le test échoué, l'attendu, le trouvé, et propose la
      correction qui a une chance
- [ ] relance automatique 400 ms après la dernière frappe, ou sur `⏎`
- [ ] `re-verify all` relance la liste en parallèle
- [ ] **une entrée non vérifiée ou invalide ne peut pas recevoir les hooks**, et le bouton
      reste visible, éteint, avec sa raison

### 16. Réglages — état des hooks et doublons

`feat` · design 3f / 3g / 3i · spec §9.1 · source : **design**

- [ ] les 5 états de la ligne hooks : installés, absents, version antérieure, conflit,
      bloqué
- [ ] le conflit affiche le **diff** et Ash refuse d'écrire
- [ ] deux entrées sur le même dossier sont signalées **sur les deux lignes**
- [ ] la réinitialisation d'une entrée `claude-code` qui produit un doublon le dit, avec
      un « annuler la réinitialisation »
- [ ] `generic` est présenté comme **dégradé** avant qu'on l'applique

⚠️ Les écrans `3g` et `3i` n'ont pas pu être lus (limite de taille de l'API). À relire
avant de commencer.

---

## J3 — Attention

Critère de sortie : **un agent en `waiting` est vu en moins de 10 s, même hors d'Ash.**

### 17. Remontée d'état et compteur agrégé

`feat` · spec §4.1 · source : **design**

- [ ] une ligne repliée porte l'état le plus urgent de ses enfants
- [ ] `waiting` l'emporte sur tout, puis `error`, puis `working`
- [ ] l'en-tête affiche `1 waiting / 7 agents`, visible sidebar repliée
- [ ] **une ligne repliée ne cache jamais un agent qui attend** — c'est le test qui compte

### 18. Notifications macOS

`feat` · spec §8 · [ADR-0010](../docs/adr/0010-sidebar-informe-terminal-agit.md) · source : spec

- [ ] notification pour `waiting` et `error` quand Ash n'est pas au premier plan
- [ ] le clic sélectionne l'agent concerné
- [ ] **jamais** de sélection automatique ni de vol de focus
- [ ] `done` ne notifie pas
- [ ] l'état « permission macOS non accordée » est visible dans les réglages, avec le
      chemin pour l'accorder

### 19. Subagents

`feat` · spec §6.5 · source : spec

- [ ] lignes filles avec libellé, état, durée
- [ ] **non cliquables** — le clic sélectionne le parent

---

## J4 — Ouverture

Critère de sortie : **un deuxième outil supporté sans toucher au cœur.**

### 20. Épinglage et persistance

`feat` · spec §3.1, §5.2 · source : spec

- [ ] un worktree épinglé reste affiché sans onglet ; le clic y en ouvre un
- [ ] `~/.ash/state.json` ne contient que les épinglés et l'état replié
- [ ] rien d'autre ne survit à la fermeture

### 21. Adaptateur `codex`

`feat` · spec §12.1 · [ADR-0008](../docs/adr/0008-abstraction-adapter.md) · source : spec

**Ce ticket commence par une enquête**, pas par du code : que `codex` expose-t-il
réellement ? C'est la question ouverte n°1 de la spec, et le principal risque du projet.

- [ ] réponse écrite : hook, fichier de session, ou rien
- [ ] si un mécanisme existe, l'adaptateur est écrit **sans modifier le cœur** — c'est le
      critère de sortie du jalon
- [ ] si aucun n'existe, `codex` tombe sur `generic` et le ticket produit l'amendement
      d'ADR correspondant

### 22. Réglages — raccourcis, apparence, notifications

`feat` · design 3j / 3k / 3l / 3m / 3n · source : **design**

- [ ] liste éditable groupée, capture d'une combinaison, échappement
- [ ] conflit entre deux actions, avec résolution proposée
- [ ] avertissement quand la combinaison est prise par macOS ou interceptée par le
      terminal
- [ ] retour au défaut, par ligne et global
- [ ] aperçu réel du thème montrant la sidebar et ses cinq états — pas trois boutons radio
- [ ] police, taille, largeur et densité de la sidebar

⚠️ Écrans non lus (limite de taille). À relire avant de commencer.

### 23. Désinstallation propre

`feat` · spec §10 · source : spec

- [ ] « retirer Ash de tous les fichiers » enlève chaque bloc de hooks
- [ ] les `.bak` sont conservés
- [ ] supprimer `~/.ash/` suffit à effacer le reste
- [ ] rien n'a été écrit dans `.zshrc`, le `PATH`, ou un hook git

---

## J5 — Git

Critère de sortie : **un rebase en conflit se traite sans quitter Ash, et l'historique dit
quel agent a écrit quoi.**

### 24. Panneau bas

`feat` · spec §4.3 · [ADR-0003](../docs/adr/0003-zone-terminal-unique.md) · source : **design**

L'infrastructure des trois vues git. Le ticket qui porte le risque technique du jalon.

- [ ] hauteur réglable, repliable, rend sa hauteur au terminal
- [ ] **ne contient jamais de terminal**, ne prend jamais le focus clavier tout seul
- [ ] le redimensionnement à chaud du PTY sous une TUI plein écran (`vim`, `htop`) ne
      casse pas l'affichage — c'est le vrai critère

### 25. Popup de branches et actions

`feat` · design 4a / 4b · spec §7.1 · source : **design**

- [ ] `⌘⌃B`, ancré sur la branche du pied de fenêtre, filtrable en tapant
- [ ] groupes `current` / `recent` / `local` / `remote`, courante en tête
- [ ] la colonne de droite nomme le worktree quand la branche vit ailleurs
- [ ] **l'avertissement nomme l'agent qui travaille** avant un checkout
- [ ] `⌘⏎` ouvre les actions ; chaque action nomme ses deux côtés (« Rebase X onto Y »)
- [ ] les actions qui touchent l'arbre pendant qu'un agent écrit déclenchent une
      confirmation, qui propose la **pause** — `SIGSTOP`, pas une touche envoyée au PTY
      ([ADR-0015](../docs/adr/0015-ash-compose-l-utilisateur-envoie.md))

### 26. Journal d'attribution des commits

`feat` · spec §3.1 · [ADR-0014](../docs/adr/0014-attribution-locale-des-commits.md) · source : **design**

À faire **avant** le graphe : c'est lui qui remplit la colonne `by`.

- [ ] les commits sont détectés par surveillance de `.git/logs/HEAD`, pas par sondage
- [ ] enregistre `(repo, sha, author_date, subject, agent, tab_id, prompt)`
- [ ] la correspondance de repli `(author_date, subject)` **survit à un rebase** — testé
      sur un vrai rebase
- [ ] rien n'est écrit dans le dépôt de l'utilisateur
- [ ] le journal est purgeable explicitement

### 27. Graphe

`feat` · design 4c · spec §7.2 · source : **design**

- [ ] `⌘⌃G`, couloirs calculés **en Rust**, pas en TypeScript
- [ ] la colonne `by` nomme l'agent ; un commit sans attribution affiche son auteur git
- [ ] le panneau de détail garde le prompt qui a produit le commit
- [ ] au-delà de 4 couloirs, les branches inactives depuis 30 jours sont repliées
- [ ] reste fluide sur un dépôt de plusieurs milliers de commits

### 28. Tableau des worktrees

`feat` · design 4f · spec §7.3 · source : **design**

- [ ] `⌘⌃W` — worktree, branche, `agents now`, `last worked by`, arbre, fiche
- [ ] `done · waiting for your review` est visible : un agent a fini, personne n'a regardé
- [ ] `stale` = sans agent depuis 3 jours **et** fichiers modifiés — signalé, jamais
      supprimé
- [ ] la suppression d'un worktree énonce ce qu'elle emporte avant de le faire

### 29. Détection d'un rebase arrêté et composition du prompt

`feat` · design 4d · spec §7.4 · [ADR-0015](../docs/adr/0015-ash-compose-l-utilisateur-envoie.md) · source : **design**

- [ ] l'opération, les fichiers en conflit, le pas (`2/5`) et `ORIG_HEAD` sont lus depuis
      les fichiers de contrôle et affichés justes
- [ ] Ash **ne touche à rien** de lui-même
- [ ] le prompt composé porte les chemins, le commit d'arrêt et la commande de test
- [ ] **Ash ne presse jamais `⏎`** — le texte est visible, éditable, effaçable (`⌥⌫`)
- [ ] composer sélectionne l'onglet de destination
- [ ] Ash **refuse de composer dans un prompt non vide**
- [ ] `abort` et `skip` restent visibles avant d'entrer

### 30. Onglet de merge

`feat` · design 4e · spec §7.4 · source : **design**

Le premier onglet sans PTY — il valide le typage des onglets du modèle §3.

- [ ] `⌘⌃M`, trois panneaux, hunk par hunk, panneau central éditable
- [ ] les côtés portent le **nom de leur branche**, pas `ours`/`theirs`
- [ ] `continue` reste visible mais éteint tant qu'il reste des conflits, avec le compte
- [ ] fermer l'onglet ne perd rien : l'état vit dans l'index git
- [ ] « passer le reste à claude » réutilise le ticket 29

### 31. Fiche de branche

`feat` · design 4g · spec §7.5 · [ADR-0013](../docs/adr/0013-fiche-de-branche-dans-le-depot.md) · source : **design**

Le deuxième endroit où Ash écrit chez l'utilisateur — et le premier **dans son dépôt**.

- [ ] `⌘⌃I` — rendu à gauche, source à droite
- [ ] markdown standard uniquement : front matter, `- [ ]`, tableaux, `mermaid`
- [ ] Ash n'écrit **que** dans `<!-- ash:log -->`, avec sauvegarde et refus sur édition
      manuelle
- [ ] **mode local** quand l'équipe ne veut pas de `.ash/` dans le dépôt
- [ ] un conflit sur le bloc `ash:log` n'est **jamais** résolu par Ash

### 32. Groupe de raccourcis git

`feat` · design 4i · spec §4.4 · source : **design**

- [ ] `⌘⌃B` / `G` / `W` / `M` / `I` éditables dans les réglages
- [ ] `⌘⌃M` n'est actif que pendant un rebase ou un merge arrêté
- [ ] mêmes avertissements de conflit que le reste des raccourcis

---

## Ce qui n'est pas dans ce backlog, et pourquoi

- **Les questions ouvertes de la spec §12** (durée de `done`, onglets à `~`, ordre de
  `Cmd+1..9`, suffixe du worktree principal) : elles se tranchent à l'usage, pas dans un
  ticket. Elles seront ouvertes comme issues `question` séparées si besoin.
- **Le démon `ashd`** ([ADR-0009](../docs/adr/0009-cycle-de-vie-des-agents.md)) : chemin
  de sortie documenté, pas décidé.
- **Le portage Linux** : hors périmètre
  ([ADR-0005](../docs/adr/0005-sonde-cwd-libproc.md)).
- **`kimi` et `opencode`** : après `codex`, et seulement si l'enquête du ticket 21 est
  concluante.
