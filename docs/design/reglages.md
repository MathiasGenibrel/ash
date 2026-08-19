# Réglages — ce que la planche montre

Document de référence tiré de la planche de design **« Ash — réglages, fenêtre séparée,
800 × 600 »** (projet Claude Design « Ash : Terminal augmenté », fichier
`Ash-3-reglages.dc.html`). Il est écrit pour quelqu'un qui ne verra jamais la planche.

**Il décrit, il ne décide pas.** Là où la planche contredit la spec, une ADR ou le code
livré, la contradiction est posée avec ses deux côtés — jamais tranchée. Les arbitrages
sont rassemblés au §6.

**Les libellés anglais sont reproduits mot pour mot**, tels que la planche les écrit
(minuscules comprises) : c'est ce qui finira à l'écran, le reformuler serait le perdre.
Les commentaires en français de la planche sont, eux, des notes de l'auteur du design ;
ils sont cités comme tels.

La planche compte **quatorze cadres**, numérotés `3a` … `3n` :

| | Cadre | Sujet |
|---|---|---|
| 01 | `3a` | outils — cas nominal, sombre |
| 02 | `3b` | outils — le même écran, clair |
| 03 | `3c` | les 5 états de vérification d'un chemin, sombre |
| 04 | `3d` | les mêmes, clair |
| 05 | `3e` | entrée invalide, correction proposée, hooks bloqués |
| 06 | `3f` | doublon après réinitialisation |
| 07 | `3g` | conflit de hooks + diff |
| 08 | `3h` | formulaire d'ajout, et état vide |
| 09 | `3i` | les 5 états de la ligne `hooks` |
| 10 | `3j` | **raccourcis** — au repos, en capture, en conflit |
| 11 | `3k` | raccourcis, clair |
| 12 | `3l` | **apparence** |
| 13 | `3m` | notifications |
| 14 | `3n` | notifications, clair |

La sidebar de l'écran principal en thème clair n'est **pas** sur cette planche : elle
renvoie à une autre planche, `Ash-2-theme-clair.dc.html#2b`, qui n'est pas au dossier.

---

## 0. Le cadre commun, et la question du système de design

### 0.1 La fenêtre

Tous les cadres sont dessinés à **800 × 600 exactement** — la taille réelle de la fenêtre
(`inner_size(800, 600)` et `min_inner_size(800, 600)` dans
`src-tauri/src/features/settings/commands.rs`). Rien à arbitrer là-dessus.

- **Barre de titre** : 36 px de haut, fond `#16171a`, bordure basse `#24262b`, trois pastilles
  de 12 px (`#ff5f57` / `#febc2e` / `#28c840`), titre centré `settings — ash` en 11,5 px
  `#8b9099`.
- **Colonne de navigation** : 152 px, fond `#121316` (= `--ash-bg-sidebar` en sombre),
  bordure droite `#24262b`, `padding: 10px 0`, `gap: 1px`.
  Entrées `tools` · `shortcuts` · `appearance` · `notifications`, en 11,5 px, `padding:
  5px 12px`, couleur `#7c8089` ; l'entrée **active** passe en `#e8eaee`, fond `#1c1e23`,
  `border-left: 2px solid #8b9099` et son `padding-left` tombe à 10 px pour compenser le rail.
  En bas de la colonne, collé au pied : `tab / ⌥↑↓ to move`, 9,5 px, `#3f434a`.
- **En-tête de section** : titre 13 px `#e8eaee`, compteur 10,5 px `#5a5e66`, boutons à
  droite en 10,5 px, bordure 1 px, rayon 4 px, `padding: 3px 8px`.
- **Pied de fenêtre** : bande 9,5 px `#4a4e56`, fond `#101114`, `border-top: 1px solid
  #1d1f24`, `padding: 7px 16px`. Chaque section a le sien, et il dit une **conséquence**, pas
  une aide.

Un détail de forme qui se répète et vaut règle : **la planche n'utilise aucune icône
décorative**. Les seuls SVG sont des signes d'état ou d'action (`rotate-ccw`, `plus`,
`triangle-alert`, `check`, `x`, `circle`, `copy`, `folder`, `chevron-down`), tracés dans une
boîte `viewBox="0 0 24 24"`, `stroke-width: 1.75`, sans remplissage, rendus à 10–13 px. C'est
exactement le repère de `shared/agent-state` (« la boîte 24×24 est celle des glyphes de la
fenêtre de réglages »).

### 0.2 La palette de la planche est déjà celle d'Ash

Point important, et vérifié couleur par couleur : **les valeurs en dur de la planche sont
les tokens d'`src/app/styles.css`**, dans les deux thèmes.

| Rôle | Planche sombre | Token | Planche clair | Token |
|---|---|---|---|---|
| fond sidebar / nav | `#121316` | `--ash-bg-sidebar` | `#f2f2f0` | `--ash-bg-sidebar` |
| accent (`waiting`) | `#ff8a5c` | `--ash-accent` | `#c8481f` | `--ash-accent` |
| texte sur accent | `#f2ded6` | `--ash-accent-fg` | `#5c2a16` | `--ash-accent-fg` |
| `working` | `#6fb2d2` | `--ash-working` | `#2a7ea8` | `--ash-working` |
| `done` | `#63b995` | `--ash-done` | `#2f7d5c` | `--ash-done` |
| `error` | `#cf4f4f` | `--ash-error` | `#b03a30` | `--ash-error` |
| `idle` | `#565a62` | `--ash-idle` | `#a9abaf` | `--ash-idle` |
| avertissement | `#d9a441` | `--ash-warning` | `#9a6b10` | `--ash-warning` |
| bordure d'avertissement | `#3d3423` | `--ash-warning-border` | `#e0d3b6` | `--ash-warning-border` |
| bordure d'avertissement forte | `#6b5a2f` | `--ash-warning-border-strong` | `#d8c69c` | idem |
| filet d'avertissement | `#241f17` | `--ash-warning-header-rule` | `#ece2cd` | idem |

**Ce qui n'a pas de token**, et qu'il faudra donc trancher (§6.6) :

- la famille « capture en cours », froide et bleue : `#6fb2d2` en texte et bordure, fond
  `#131417`, bordure de bloc `#2c3540`, halo `0 0 0 3px rgba(111,178,210,0.16)` — en clair
  `#2a7ea8`, fond `#f4f8fa`, bordure `#bcd6e2`, halo `rgba(42,126,168,0.14)`. La teinte
  est celle de `--ash-working`, mais ses fonds et bordures n'existent pas ;
- le gris de sélection neutre de l'apparence : `#8b9099` (rail de nav, pastille radio
  cochée, jauge de taille) et son halo `rgba(139,144,153,0.18)` ;
- les fonds de blocs `#131417`, `#141517`, `#101114`, `#16130f` — des demi-tons entre
  `--ash-bg` et `--ash-bg-card` que la feuille de style d'Ash n'a pas nommés.

La planche importe **une seule fois** un composant du système de design shadcn :
`ShadcnUi.Switch` (36 × 20), pour les trois interrupteurs de notification. C'est le seul
endroit où elle sort de son propre vocabulaire, et Ash ne doit pas le suivre : sa couche
`shared/ui` n'a pas d'interrupteur, et la section livrée rend des boutons `on`/`off`.

La police de la planche est **JetBrains Mono** partout, y compris dans les champs — donc
conforme au brief et au dépôt.

---

## 1. Raccourcis (`3j` sombre, `3k` clair)

C'est la section la plus riche de la planche, et celle que le code n'a pas commencée : la
section livrée (`src/features/settings/components/shortcuts.ts`) est en **lecture seule
assumée**, avec le pied `read-only: these come from the native menu, and ash can't rebind
them yet.`

### 1.1 L'écran

En-tête : `shortcuts` (13 px) · compteur **`2 changed`** (10,5 px `#5a5e66`) · à droite un
bouton **`reset all`** précédé de l'icône `rotate-ccw` (11 px).

Le corps est une pile de groupes (`gap: 14px`), chaque groupe étant :

- un intitulé de groupe en 9,5 px `#4a4e56`, `letter-spacing: 0.06em`, `padding-bottom: 5px` ;
- ses lignes, `gap: 2px`.

Les trois groupes dessinés sont **`navigation`**, **`tabs`**, **`terminal`**.

Pied de fenêtre, en deux moitiés : à gauche, l'icône `rotate-ccw` puis
`only appears on changed rows.` ; à droite `tab walks the rows · ⏎ opens capture`.

### 1.2 Les états d'une ligne

**a. Au repos.** `display: flex`, `padding: 5px 8px`, rayon 4 px, fond transparent. À gauche
l'action en 12 px `#c3c6cc` ; à droite la combinaison dans une pastille : 11,5 px `#e8eaee`,
bordure `1px solid #2c2f36`, rayon 4 px, `padding: 2px 8px`, fond `#141517`.

**b. Survolée / focus.** La même ligne prend le fond `#131417`. Rien d'autre ne change.

**c. Ligne modifiée.** Elle porte, à droite de sa pastille, un bouton-icône `rotate-ccw` de
11 px, `#6b6f78`, `padding: 3px`, rayon 3 px, `title="back to default"`, qui vire à
`#c3c6cc` sur fond `#1c1e23` au survol. Le pied de fenêtre le dit :
`only appears on changed rows.`

**d. En capture.** La ligne **s'agrandit sur place** — elle n'ouvre pas de modale ; la note
française de la planche insiste : « la ligne en capture s'agrandit au lieu d'ouvrir une
modale : le contexte reste lisible pendant qu'on appuie ». Le bloc fait `padding: 8px`,
`gap: 7px`, fond `#131417`, bordure `1px solid #2c3540`, rayon 4 px. Le nom de l'action passe
en `#e8eaee`. À la place de la pastille :

> `press a key combination`

en `#6fb2d2`, bordure `1px solid #6fb2d2`, fond `#0c0d0f`, `padding: 2px 9px`, halo
`0 0 0 3px rgba(111,178,210,0.16)`, suivi d'un **caret** de 6 × 12 px en `#6fb2d2` qui
clignote (`ash-caret 1.1s step-end infinite`).

Dessous, une ligne d'aide en 10,5 px `#5a5e66`, dont les touches sont en `#9aa0a8` :

> `esc` `cancel`  ·  `⌫` `no shortcut`  ·  `⏎` `confirm`  · *(à droite)* `was:` `⌘⇧N`

**Trois issues, donc** : échapper (`esc`), **effacer** le raccourci sans en mettre d'autre
(`⌫` → `no shortcut`), confirmer (`⏎`). Et l'ancienne valeur reste lisible pendant qu'on
tape (`was: ⌘⇧N`).

**e. Avertissement macOS, pendant la capture.** Sous un filet `1px solid #1d1f24` à 7 px,
une icône `triangle-alert` `#d9a441` et le texte en 10,5 px `#9aa0a8` :

> `⌘⌥⎋` `is reserved by macOS (force quit) — ash will never receive it`

La combinaison citée est en `#c3c6cc`. **Ce n'est pas un refus** : la note française dit
« une combinaison prise par macOS ou avalée par le terminal n'est pas interdite — elle est
annoncée comme inefficace, au moment de la capture ».

**f. Combinaison avalée par le terminal.** Ligne au repos, dans le groupe `terminal` :

> `interrupt the agent` … *(à droite)* `swallowed by the terminal — never reaches ash` puis
> la pastille `⌃C`

La pastille est **éteinte** : texte `#7c8089`, bordure `#24262b`, fond `#101114` — le même
gris que le message. Elle porte quand même le bouton `rotate-ccw` de retour au défaut.

**g. Conflit entre deux actions.** Les deux lignes fautives sont réunies dans **un seul
bloc d'avertissement** : fond `#16130f`, bordure `1px solid #3d3423`, rayon 4 px,
`padding: 8px`, `gap: 7px`. Chaque ligne y garde sa forme, avec en plus une étiquette 10,5 px
`#d9a441` avant la pastille, et une pastille bordée `#6b5a2f` :

> `clear the scrollback` — `already assigned` — `⌘K`
> `open the command palette` — `just now` — `⌘K`

Puis, sous un filet `1px solid #241f17` à 7 px : icône `triangle-alert`, le diagnostic en
10,5 px `#9aa0a8`, et **deux boutons** poussés à droite :

> `two actions on ⌘K — the last one set would silently win`
> [ `give ⌘K to the palette` ] [ `keep the old one` ]

Le premier est le bouton conséquent (texte `#e8eaee`, bordure `#3a3d45`), le second le
secondaire (`#9aa0a8`, bordure `#2c2f36`). Note française : « un conflit interne se résout
par un choix explicite : ash ne réattribue jamais en silence ».

### 1.3 Les lignes dessinées, mot pour mot

| Groupe | Libellé de la planche | Combinaison | État montré |
|---|---|---|---|
| `navigation` | `select the n-th tab` | `⌘1…9` | repos |
| `navigation` | `collapse / expand the sidebar` | `⌘B` | repos (survolée) |
| `tabs` | `new tab in the current workspace` | `⌘N` | repos |
| `tabs` | `new tab at ~ (new workspace)` | — (`was: ⌘⇧N`) | **capture** + avertissement macOS |
| `tabs` | `close the tab` | `⌘W` | repos |
| `terminal` | `clear the scrollback` | `⌘K` | **conflit** |
| `terminal` | `open the command palette` | `⌘K` | **conflit** |
| `terminal` | `interrupt the agent` | `⌃C` | avalée par le terminal |

Le cadre clair `3k` ne rejoue que trois de ces lignes (repos, capture, conflit) dans un
panneau de 800 px de large, sans chrome de fenêtre : fond `#fcfcfb`, bordure `#dedcd8`,
pastilles `#f2f2f0`/`#fff`, capture `#f4f8fa` bordée `#bcd6e2` avec l'accent `#2a7ea8`,
conflit `#faf6ee` bordé `#e0d3b6` avec `#9a6b10`.

### 1.4 Ce que la planche ne montre pas

- **Le geste qui ouvre la capture à la souris** : le pied dit `⏎ opens capture` au clavier,
  mais aucun cadre ne montre un clic, un bouton « edit », ni un état de survol qui l'annonce.
- **Le retour au défaut global** : `reset all` existe en en-tête, mais aucune confirmation
  n'est dessinée. Le compteur `2 changed` est sa seule contrepartie.
- **L'état « aucun raccourci »** après un `⌫` : la ligne est décrite comme possible
  (`no shortcut`), jamais dessinée.
- **La famille git** (`⌘⌃B/G/W/M/I`) est **absente** de la planche — voir §6.2.

### 1.5 Écart avec le code livré

| Planche | Code aujourd'hui |
|---|---|
| groupes `navigation` / `tabs` / `terminal` | groupes du menu natif : `application`, `terminal`, `view` (`descriptor()` dans `src-tauri/src/menu.rs`) |
| `new tab in the current workspace` `⌘N` | `New Tab` `⌘T` — la spec §4.4 a été **amendée le 2026-08-12** (`⌘N` ne fait plus rien) |
| `new tab at ~ (new workspace)` `⌘⇧N` | `New Tab at ~` `⌘⇧T` |
| `select the n-th tab` `⌘1…9` | `Tab 1 … Tab 9` `⌘1 … ⌘9` |
| `collapse / expand the sidebar` `⌘B` | `Toggle Sidebar` `⌘B` |
| `close the tab` `⌘W` | `Close Tab` `⌘W` |
| `clear the scrollback` `⌘K` | `Clear Scrollback` `⌘K` |
| `open the command palette` | **n'existe pas** — ni dans la spec, ni dans le menu |
| `interrupt the agent` `⌃C` | n'est pas un raccourci d'Ash : `⌃C` part au shell |
| — | `Settings…` `⌘,`, `Select Next Tab` `⌃⇥`, `Select Previous Tab` `⌃⇧⇥`, et les trois pas de taille de police (`⌘+`, `⌘-`, `⌘0`) sont dans le menu et **absents** de la planche |
| libellés en minuscules, phrasés en action | libellés en Title Case, tels que le menu les écrit |

Autrement dit : la planche a été dessinée sur le tableau de la spec §4.4 *avant* son
amendement, et sur un brief qui listait `Cmd+N`. **Les combinaisons de la planche ne
peuvent pas être reprises telles quelles** ; sa **forme** et son **comportement**, si.

---

## 2. Apparence (`3l`)

Un seul cadre, sombre. Titre de la planche : « apparence — le thème se choisit sur la
sidebar, pas sur trois boutons ».

### 2.1 Le thème : trois aperçus réels

En-tête de bloc : `theme` (13 px `#e8eaee`) suivi, sur la même ligne de base, de
`preview: the sidebar and its five states` (10,5 px `#5a5e66`).

Puis une grille `repeat(3, 1fr)`, `gap: 10px`. Chaque tuile fait **150 px de haut**, bordure
`1px solid #2c2f36`, rayon 5 px, `overflow: hidden`. La tuile **sélectionnée** échange sa
bordure pour `#8b9099` et prend un halo `0 0 0 3px rgba(139,144,153,0.18)`.

Sous chaque tuile, sa ligne de choix : une **pastille radio** de 11 × 11 px (bordure
`1px solid #3a3d45` ; cochée : fond et bordure `#8b9099`), le nom en 11,5 px, et une mention
poussée à droite en 10 px `#5a5e66`.

| Tuile | Nom | Mention | Contenu |
|---|---|---|---|
| 1 | `system` | `follows macOS` | **les deux thèmes à la fois** |
| 2 | `light` | *(aucune)* | la sidebar en clair |
| 3 | `dark` | `active` | la sidebar en sombre, sélectionnée |

**La tuile `system` est le geste de design le plus notable de la section** : elle superpose
exactement les deux rendus, et découpe celui du dessus en triangle —
`clip-path: polygon(0 0, 100% 0, 0 100%)`. Le coin haut-gauche est donc **clair**, le coin
bas-droit **sombre**, séparés par une diagonale nette. Aucun libellé n'explique le découpage :
la forme est le message.

### 2.2 Ce que contient un aperçu, ligne par ligne

C'est le seul élément que le brief textuel ne pouvait pas remplacer, donc voici sa
composition exacte. Le cadre est une **miniature de la sidebar** — pas une capture, un
redessin à l'échelle :

- fond `#121316` en sombre / `#f2f2f0` en clair (le token `--ash-bg-sidebar`), `padding: 6px 0`,
  colonne, `gap: 2px` entre les lignes ;
- **une ligne de dépôt** : `omelette-web`, 8,5 px, `#e8eaee` (clair : `#24262a`),
  `padding: 1px 8px`, sans glyphe ;
- **cinq lignes d'agent**, 8,5 px, `padding: 1px 8px 1px 14px`, `gap: 5px` entre glyphe, nom
  et durée. La durée est poussée à droite par un `flex: 1`.

| # | Nom | Glyphe | Couleur du glyphe | Fin de ligne | Particularité |
|---|---|---|---|---|---|
| 1 | `claude` | `◍` (caractère) | `--ash-working` | `15m` en `#7c8089` | — |
| 2 | `codex` | `❯` (caractère) | `--ash-accent` | `2m` **en accent** | **fond teinté** `rgba(255,138,92,0.11)`, `border-left: 2px solid` accent, `padding-left` 12 px, texte `--ash-accent-fg` |
| 3 | `claude-perso` | `check` (SVG) | `--ash-done` | `8m` en `#7c8089` | — |
| 4 | `bash` | `circle` (SVG) | `--ash-idle` | `3h` en `#5a5e66` | nom en `#9aa0a8` |
| 5 | `kimi` | `x` (SVG) | `--ash-error` | `exit 1` **en `--ash-error`** | nom en `#9aa0a8` |

Les trois SVG sont dessinés à 12 × 12 dans la boîte 24 × 24, `stroke-width: 1.75` — donc
`M20 6 9 17l-5-5` pour `done`, `<circle r="10">` pour `idle`, deux traits croisés pour
`error`.

**Ce qui change quand on bascule de thème** : rien d'autre que la palette. Les cinq formes,
les positions, les retraits, la teinte de la ligne `waiting` et son rail sont identiques ;
seules les valeurs passent d'une colonne à l'autre du tableau du §0.2. C'est la
démonstration que la planche veut faire : **le thème clair ne perd ni la hiérarchie ni
l'urgence de `waiting`**, parce que l'urgence tient au rail + fond teinté + accent, pas à la
luminosité.

Dernier détail, présent uniquement sur la tuile `dark` : le glyphe `❯` y porte l'animation
(`animation: {{ waitAnim }}`), pilotée par une propriété de la planche `waitingMotion`, dont
les valeurs sont `respiration` (défaut), `balayage`, `statique`. Les deux autres tuiles sont
figées. Les keyframes déclarés en tête de planche sont `ash-breathe` (opacité 0,5 → 1 → 0,5)
et `ash-sweep` (translation −2 px → +1 px → −2 px).

### 2.3 Les trois autres réglages

Sous un filet `1px solid #1d1f24`, une grille `120px 1fr`, `gap: 12px 14px`,
`align-items: center`. Les intitulés sont en 11 px `#7c8089`.

**`font`** — un menu déroulant de 200 px : valeur `JetBrains Mono` en 11,5 px `#c3c6cc`,
chevron `chevron-down` 10 px `#5a5e66`, bordure `#2c2f36`, rayon 4 px, `padding: 4px 9px`,
fond `#0c0d0f`. À droite, en 10,5 px `#5a5e66` :

> `7 monospace fonts installed`

**`size`** — un **curseur**, pas des boutons de pas : rail de 200 × 2 px `#24262b`, partie
remplie 96 px `#8b9099`, poignée ronde de 9 px `#c3c6cc`. À droite la valeur
`13 px` en 11,5 px `#c3c6cc` avec `font-variant-numeric: tabular-nums`, puis un **échantillon
vivant rendu à la taille choisie** :

> `❯ bun test src/sidebar` (13 px, `#7c8089`)

**`density`** — un segmenté de deux boutons collés, bordure `#2c2f36`, rayon 4 px,
`overflow: hidden` ; chaque segment en 11 px, `padding: 4px 11px`, séparés par
`border-left: 1px solid #2c2f36` :

> [ `comfortable` ] [ `compact` ]

Le segment actif (`comfortable`) est en `#e8eaee` sur `#1c1e23`, l'autre en `#7c8089`.
À sa droite, **deux miniatures abstraites** qui montrent la différence : trois barres de
60 × 5 px espacées de 3 px (`#24262b`), puis quatre barres de 60 × 4 px espacées de 1 px
(`#1d1f24`) ; et la mesure, en 10,5 px `#5a5e66` :

> `24 px / row · 18 px when compact`

**Pied de la section — c'est une décision, pas une aide** :

> `sidebar width is set by dragging its right edge in the main window — 180 to 420 px, ⌘B to
> collapse. no setting here.`

### 2.4 Écart avec le code livré

`src/features/settings/components/appearance.ts` rend deux lignes seulement, et le dit
lui-même : « l'aperçu qui montrerait les cinq états d'agent, la police au choix et la densité
de la sidebar attendent les planches de l'issue #22 ».

| Planche | Code aujourd'hui |
|---|---|
| trois **aperçus** de sidebar de 150 px, radio + halo | trois **boutons** `system` / `light` / `dark` avec `aria-pressed` |
| tuile `system` en diagonale, mention `follows macOS` | note en prose : `system follows macOS, and changes with it.` |
| `font` — liste des monospace installées + `7 monospace fonts installed` | **rien** ; `font = "JetBrains Mono"` est réservé dans `config.toml` (spec §9) et jamais lu par un écran |
| `size` — curseur + valeur + échantillon vivant | `N pt` + trois boutons de pas (`bigger` / `smaller` / `default`), doublés du menu Vue |
| `density` — segmenté + miniatures + `24 px / row · 18 px when compact` | **rien** ; `sidebar_density` est réservé dans `config.toml` |
| largeur de sidebar : **pas un réglage**, poignée de glissement 180–420 px | **rien** — ni réglage, ni poignée ; `sidebar_width = 240` est dans `config.toml` |
| intitulé `size` | intitulé `terminal font` |

Deux remarques de fidélité :

1. l'aperçu de la planche **ne barre pas** le nom de l'agent en `error`, alors que
   `shared/agent-state` pose `struck: true` sur cet état (et le rail `error`). Une miniature
   fidèle devrait le barrer, ou la planche a simplifié à 8,5 px ;
2. l'aperçu montre des **lignes d'agent seulement** — ni compteur agrégé (`1 waiting /
   7 agents`, spec §4.1), ni worktree, ni ligne fille de sous-agent. C'est une réduction
   volontaire à ce qui porte la couleur.

---

## 3. Notifications (`3m` sombre, `3n` clair)

Section **déjà livrée** (PR #124). Elle est décrite ici pour mémoire et pour les écarts.

### 3.1 L'écran

- L'entrée `notifications` de la colonne de navigation porte, **quand l'autorisation
  manque**, un `triangle-alert` de 10 px `#d9a441` poussé à droite. C'est le seul cas de la
  planche où la navigation elle-même signale un état.
- En-tête : `notifications` + `only when ash isn't in the foreground`.
- **Bandeau d'autorisation** (fond `rgba(217,164,65,0.06)`, bordure `1px solid #3d3423`,
  rayon 5 px, `padding: 11px 12px`, `gap: 9px`) :

  > `macOS hasn't granted notification permission` — [ `open System Settings` ]
  >
  > `System Settings › Notifications › Ash › Allow notifications`
  > `meanwhile the states stay visible in the sidebar and the window footer — nothing is
  > lost, only the alert is missing.`

  Les `›` sont en `#5a5e66`, le chemin en 10,5 px `#7c8089`, `line-height: 1.65`.
- **Trois lignes**, `padding: 12px 2px`, séparées par `border-bottom: 1px solid #1d1f24`.
  Colonne de gauche fixée à **120 px**, portant le **nom de l'état dans sa couleur** ; au
  centre, la phrase en 11,5 px `#c3c6cc` puis, sur une seconde ligne, la raison en 10,5 px
  `#5a5e66` ; à droite l'interrupteur (`ShadcnUi.Switch`, 36 × 20).

| État | Couleur | Phrase | Seconde ligne | Position |
|---|---|---|---|---|
| `waiting` | accent | `an agent is waiting for a reply` | `the only event that blocks you: until you answer, nothing moves` | **on** |
| `error` | `--ash-error` | `an agent failed` | `non-zero exit code, or process killed` | **on** |
| `done` | `--ash-done` | `an agent finished` | `off by default: across seven agents it rings constantly` | **off** |

- Note de bas de section (10,5 px `#5a5e66`) :

  > `tools on the generic adapter never emit waiting — they can only notify error and done.`

- Pied de fenêtre :

  > `clicking a notification opens the matching tab and brings it to the front.`

Le cadre clair `3n` est un panneau de 520 px qui reprend les mêmes lignes en abrégé :
`macOS permission not granted`, bouton `open`, chemin réduit à
`System Settings › Notifications › Ash`, colonne d'état à 96 px, et les phrases sans leur
seconde ligne.

### 3.2 Écart avec le code livré

Rien de bloquant, et deux divergences **délibérées** du code, documentées dans
`src/features/settings/components/notifications.ts` :

1. **Aucun bouton `open System Settings`** — « ouvrir le panneau des Réglages Système à la
   place de l'utilisateur serait un geste qu'Ash n'a pas à faire, et le chemin se lit en
   trois mots ». La planche en dessine un dans les deux thèmes ;
2. **des boutons `on`/`off` au lieu d'un interrupteur** — la couche `shared/ui` n'a pas de
   `Switch`, et celui de la planche vient de shadcn (§0.2). Le bouton dit la position, pas
   le geste, et porte `aria-pressed`.

Deux différences mineures de plus : le code préfixe chaque ligne du **glyphe** de l'état
(`presentAgentState`), la planche non ; et les phrases de la planche (`means`) sont plus
longues que celles que le backend envoie aujourd'hui — elles sont reproduites ci-dessus si
l'on veut les reprendre, mais elles appartiennent au backend, pas à la vue.

---

## 4. Outils (`3a` → `3i`) — écarts seulement

Section **livrée et substantielle**. La planche et le code se recouvrent très largement :
les cinq états de vérification, les quatre pastilles de test numérotées, les cinq états de
la ligne `hooks`, le doublon signalé sur les deux lignes, l'écran de conflit avec son diff,
le formulaire d'ajout, l'état vide, la désinstallation globale — tout cela existe. Ce
paragraphe ne relève donc **que ce que la planche montre en plus**, ou autrement.

### 4.1 Les libellés que la planche fixe, et que le code n'a pas

| Où | Libellé de la planche | État dans le code |
|---|---|---|
| en-tête `tools` | `3 declared · 3 verified`, `3 declared · 1 invalid` | équivalent (`describeToolCount`) |
| sous l'en-tête | `one command = one tool. ash re-runs the tests on every path or adapter change.` / `tests · 1 folder readable · 2 adapter signature · 3 command in PATH · 4 command uses this folder` | **présent** (`chrome.ts`) |
| champ `config` | bouton-icône `copy path` (icône `copy`, 13 px) **dans** le champ | **absent** |
| champ `config` | bouton `Browse…` (icône `folder`, `title="choose in Finder"`) | **absent** — laissé vide sciemment (`view.ts`) : rien ne sait ouvrir le Finder |
| état vide | `ash found these commands in your PATH`, trois lignes `claude` / `claude-perso` / `codex` avec leur `adaptateur · chemin` et un bouton `declare` chacune, puis `declare all` et la note `detected by binary name and the presence of a config folder — every suggestion is verified before any write` | **absent** — laissé vide sciemment ; inventer des candidats afficherait les données d'exemple de la maquette |
| formulaire d'ajout | encart `found in PATH, not declared yet` avec trois pastilles cliquables `opencode` `claude-work` `aider` | **absent** |
| ligne `hooks` d'une carte | le **chemin du fichier** affiché à côté du résumé : `~/.claude/settings.json` | présent partiellement (le fichier est montré ou tu selon `verification-state.ts`) |
| écran de conflit | boutons `copy path` et `open in editor` en tête du diff | **absent** |
| écran de conflit | `outside the ash block the file is untouched — 148 lines intact.` | **absent** (le code dit la sauvegarde, pas le décompte) |
| écran de conflit | `while the conflict lasts, claude keeps its v2 states in the sidebar: working / done / idle, no waiting.` | **absent** |
| pied de section | `every write is preceded by a copy settings.json.bak` | présent sous une autre forme |

### 4.2 Les phrases exactes des cinq états de vérification (`3c`)

Utiles si l'on veut aligner les textes du backend. Chaque état est présenté avec son numéro
(`01` … `05`), son nom, la ligne telle qu'elle apparaît, et une glose de la planche.

1. **`unverified`** — chemin `~/.claude-perso`, message `path changed — unverified`, bouton
   `verify`, ligne hooks `hooks — install unavailable`.
   Glose : `default state after every keystroke. ash re-runs on its own 400 ms after the last
   key, or right away on ⏎.`
2. **`verifying`** — message `folder recognised · test 4 of 4`, bouton `cancel`, la commande
   réellement lancée montrée en clair : `CLAUDE_CONFIG_DIR=~/.claude-perso claude-perso
   --version`, ligne hooks `hooks — waiting on test 4`.
   Glose : `the result lands in two passes: the row turns green on tests 1–3, then completes
   once the command has answered.`
3. **`valid`** — message `Claude Code 2.1.198 · 11 projects · last active 6 d`, bouton
   `re-verify`, ligne hooks `hooks — installed · v3`.
   Glose : `one row, what ash recognised: the version proves test 3, the project count test 2,
   the activity test 4.`
4. **`valid with a caveat`** — message `folder recognised · command claude-perso not found in
   PATH`, bouton `locate…`, ligne hooks `hooks — installable, but nothing will fire them`
   (le cadre clair l'abrège en `hooks — installable, nothing will fire them`).
   Glose : `the folder is right, the pair isn't. ash still writes if you insist, and says so.`
5. **`invalid`** — chemin `~/dev/notes`, message `doesn't look like a Claude Code config — no
   settings.json, no projects/`, mention `stopped at test 2`, puis `suggested fix` →
   [ `generic adapter` ] [ `another folder…` ], ligne hooks `hooks — install unavailable`.
   Glose : `only valid and valid with a caveat allow hooks to be written. the block stays
   visible: button present, disabled, with its reason — never hidden.`

Et sous la planche : `re-verify all re-runs the whole list in parallel, tests 1–3 first.`

Les **pastilles de test** sont quatre carrés de 13 × 13 px, rayon 3 px, chiffre en 8,5 px,
fond teinté à 14 % de la couleur d'état (`rgba(99,185,149,0.14)` quand le test passe).

### 4.3 Les cinq états de la ligne `hooks` (`3i`)

En tête du cadre, la phrase qui définit ce qu'Ash écrit :

> `a delimited block in <config>/settings.json`, entre `// >>> ash v3 >>>` and
> `// <<< end ash block <<<` — `nothing else, ever.`

| État | Résumé | Action | Glose |
|---|---|---|---|
| installed and current | `installed · v3` | `remove` | `4 hooks written · ~/.claude/settings.json` — `remove deletes the block and its markers, leaves the rest of the file intact, and writes a .bak first.` |
| missing | `missing` | `install` | `the tool stays visible in the sidebar, but without waiting: ash can't tell that it is waiting.` / `install first shows the exact 4 lines to be written, then writes.` |
| older version | `v2 · v3 available` | `update` | `v2 does not write SubagentStop: sub-agents stay invisible in the sidebar.` / `until you update, ash keeps working — just coarser. nothing blinks.` |
| conflict | `block edited by hand` | `see the diff` | `ash does not write. it shows the diverging lines and lets you choose.` / `a conflict does not degrade the display: the hooks already in place keep working.` |
| blocked | `path unverified` | `install` (éteint) | `the button stays where it is, dimmed, with its reason on the left.` / `as soon as tests 1–3 pass it lights up — without waiting for test 4.` |

Pied du cadre :

> `before any write: settings.json → settings.json.2026-08-07T14-22.bak, in the same folder,
> never overwritten.`
> `remove ash from every file… in the section footer: lists every touched file, then removes
> the blocks in one pass.`

**Attention** : cette description du format écrit contredit ce qu'Ash fait réellement — voir
§6.4.

### 4.4 Le doublon (`3f`) et le mode dégradé (`3e`, `3h`)

Bandeau de doublon, en tête de liste, avec son action :

> `claude and claude-perso point at the same folder — one of the two will do nothing` —
> [ `undo the reset` ]

Sur chaque carte concernée, sous le champ : `duplicate · also claude-perso` /
`duplicate · also claude`, et sur celle qu'on vient de réinitialiser une ligne
`was ~/.claude-perso` avec un bouton `restore`. La ligne `test` de la seconde dit :

> `the folder is valid, but already declared by claude — two entries on the same config make
> no sense`

et sa ligne `hooks` :

> `already written by claude in this file`

Le mode dégradé est annoncé **avant** qu'on l'applique, en deux phrases :

> `without a dedicated adapter, ash reads the process output, not its hooks.`
> `kimi will show as idle · done · error — never waiting. no “waiting for a reply”
> notification for this tool.`

(Le code écrit `ash watches the process, not its hooks.` — une variante.)

Et pour l'entrée invalide, le détail attendu/trouvé :

> `expected: settings.json, projects/ — found: 12 .md files, 1 .git folder`
> `use the generic adapter instead?` — [ `apply` ] [ `choose another folder…` ]

---

## 5. Ce que la planche ne montre nulle part

À signaler plutôt qu'à combler :

- **la sidebar de l'écran principal en thème clair**, pourtant demandée au brief : elle
  renvoie à `Ash-2-theme-clair.dc.html#2b`, qui n'est pas au dossier. Ce que la planche
  livre à la place, ce sont les miniatures de 150 px du §2.2 ;
- **les lignes filles de sous-agents** (spec §6.5) : aucun réglage, aucune mention — alors
  que la durée de rémanence d'une ligne fille finie *est* un réglage injecté au superviseur,
  que la fenêtre ne porte pas encore ;
- **la désinstallation d'Ash** (#23) au-delà du `remove ash from every file…` de la section
  outils ;
- **le comportement au clavier** hors de deux phrases (`tab / ⌥↑↓ to move`,
  `tab walks the rows · ⏎ opens capture`) ;
- **les états de chargement** des sections `shortcuts`, `appearance` et `notifications` (le
  code en a : `reading them from the menu…`, `asking ash what it is set to…`, `asking
  macOS…`) ;
- **le redimensionnement** : tout est dessiné à 800 × 600, jamais plus grand.

---

## 6. Arbitrages posés, non tranchés

### 6.1 Toute capture de combinaison contredit « une seule liste »

**Ce que la planche suppose du backend.** Elle dessine une capture (`press a key
combination`), un compteur `2 changed`, un `reset all`, un retour au défaut par ligne, et
une résolution de conflit qui **réattribue** (`give ⌘K to the palette`). Cela demande :

1. un **magasin de liaisons** persistant (dans `~/.ash/config.toml` ou à côté), distinct de
   la table du menu, avec pour chaque action sa liaison courante **et** son défaut — sans
   quoi `2 changed`, `back to default` et `reset all` n'ont rien à comparer ;
2. la capacité de **reconstruire le menu natif à l'exécution**. `muda` ne change pas
   l'accélérateur d'un item existant : il faut rebâtir le menu et le reposer sur
   l'application ;
3. une **détection de collision** contre les autres actions (conflit interne, `already
   assigned`), contre macOS (`is reserved by macOS (force quit)`) et contre le terminal
   (`swallowed by the terminal — never reaches ash`). Les deux dernières supposent une table
   de combinaisons réservées, embarquée : macOS n'expose pas cette liste.

**Le côté d'en face.** Le critère dur de l'issue #110 est qu'il n'existe **qu'une seule
liste**, et la section livrée est en lecture seule pour cette raison (`shortcuts.ts` :
« deux listes finissent toujours par diverger, et c'est l'écran des réglages qu'on croit
quand elles ne disent pas la même chose »). Aujourd'hui, la liste unique est
`menu_shortcuts()` dans `src-tauri/src/menu.rs`, et l'écran la lit.

**La forme du compromis, si on la veut** : la liste unique **change de côté**. Le magasin de
liaisons devient la source, `descriptor()` cesse de porter les accélérateurs en dur, et le
menu est **dérivé** du magasin à chaque changement. Tant que ce renversement n'est pas fait,
toute capture dans les réglages crée une seconde liste — exactement ce que #110 interdit.
C'est une question d'architecture, pas de style.

### 6.2 La famille git n'a pas de surface, et la planche ne la dessine pas

**Bonne nouvelle, à consigner** : la planche **ne dessine aucune ligne** `⌘⌃B` / `⌘⌃G` /
`⌘⌃W` / `⌘⌃M` / `⌘⌃I`. Ses trois groupes sont `navigation`, `tabs`, `terminal`. Elle ne
crée donc pas de pression pour livrer des raccourcis dont les surfaces n'existent pas avant
J5, et dont la déclaration est l'issue #127.

**En revanche, elle en invente un autre** : `open the command palette` `⌘K`, qui sert de
second terme au conflit. Or Ash n'a **pas** de palette de commandes — ni dans la spec §4.4,
ni dans le menu, ni dans les jalons. Le conflit dessiné est donc un conflit **fictif**. Il
faudra soit le rejouer avec deux actions réelles (aucune paire de la liste actuelle n'entre
en collision), soit fabriquer la collision par une capture — ce qui est le cas d'usage réel
de toute façon.

### 6.3 Réinitialiser une entrée d'outil : la planche se contredit elle-même

Trois textes, deux réponses :

- **la planche, sur la carte** : le bouton-icône de l'en-tête porte
  `title="reset to the adapter default"` — c'est le brief (`claude-code → ~/.claude`) ;
- **la planche, dans sa note française de `3f`** : « un défaut par adaptateur ne peut être
  juste que pour une seule des deux entrées claude-code. il produit mécaniquement ce
  doublon. **proposition : le défaut est par entrée.** ash mémorise, pour chaque entrée, le
  dernier chemin qui a passé les quatre tests. “réinitialiser” y revient. tant qu'aucune
  vérification n'a réussi, l'entrée retombe sur le défaut de l'adaptateur » ;
- **la spec §9.1** : « Réinitialiser une entrée la ramène à sa **dernière valeur valide**,
  pas au défaut de son adaptateur. […] Tant qu'une entrée n'a jamais été valide, elle n'a
  rien à restaurer : le bouton reste visible et éteint, avec sa raison. »

Le **code livré suit la spec** (`lastValidConfig`, `describeReset`, `no verified folder to go
back to yet`, `already on the last folder that worked`). La note de la planche va dans le
même sens, à une nuance près : elle fait **retomber** l'entrée jamais vérifiée sur le défaut
de l'adaptateur, là où la spec **éteint** le bouton. C'est la seule divergence réelle, et
elle est petite — mais elle change ce qui se passe sur une entrée neuve.

Le `title` de la carte, lui, est à corriger dans tous les cas : il annonce le comportement du
brief, pas celui qui est implémenté.

### 6.4 Le format écrit dans `settings.json` : bloc délimité vs entrées marquées

La planche décrit, en toutes lettres et à deux endroits (`3i`, `3g`) :

> `a delimited block in <config>/settings.json`, entre `// >>> ash v3 >>>` and
> `// <<< end ash block <<<`

et son diff montre des commentaires `//` dans un JSON, une numérotation de lignes 34–41, et
un `v3`.

Ash écrit autre chose. `CLAUDE.md` et ADR-0007 posent la règle : **entrées marquées** dans un
`settings.json`, « dont chacune se reconnaît seule et cohabite avec celles de
l'utilisateur » ; le bloc délimité est réservé aux `.md` « où il n'y a rien à entrelacer ».
Le marqueur réel est `# ash:hook v`, et sa version courante est **v1**
(`src-tauri/src/features/hooks/document.rs`).

Conséquences à trancher : les libellés `installed · ash block v3`, `v2 · v3 available`,
`block edited by hand`, `remove deletes the block and its markers`, et tout le diff de `3g`
parlent d'un objet qui n'existe pas. La **forme** de l'écran de conflit (diff numéroté, ce
qui n'est pas touché, `.bak` horodatée annoncée avant l'action, trois issues) reste valable ;
son **vocabulaire** ne l'est pas.

### 6.5 La largeur de la sidebar : un réglage qui disparaît

- **La planche** la retire de l'écran, et le dit dans le pied de la section apparence :
  `sidebar width is set by dragging its right edge in the main window — 180 to 420 px, ⌘B to
  collapse. no setting here.`
- **La spec §9** réserve `sidebar_width = 240` dans `[ui]`, et l'énoncé de #22 la range dans
  les réglages restants.

Les deux tiennent ensemble **si** la poignée de glissement existe et persiste sa valeur dans
`config.toml`. Elle n'existe pas aujourd'hui : suivre la planche, c'est déplacer le travail
de la fenêtre de réglages vers la fenêtre principale, pas le supprimer. Les bornes
`180 … 420` sont, elles, une valeur neuve que la spec ne donne pas.

Même remarque, plus bénigne, pour la densité : `24 px / row · 18 px when compact` sont deux
mesures que ni la spec ni la feuille de style d'Ash ne portent aujourd'hui — aucune hauteur
de ligne de sidebar n'est nommée dans `src/app/styles.css`.

### 6.6 Ce qui n'a pas d'équivalent dans les tokens d'Ash

À trancher au moment d'implémenter, sans importer le système de la planche :

1. **la famille « capture »** (bleu froid, fond, bordure, halo — §0.2). Sa teinte est
   `--ash-working`, mais un état de saisie n'est pas un agent qui travaille : réutiliser le
   token ferait dire deux choses à une couleur ;
2. **le gris de sélection neutre** `#8b9099` et son halo, utilisés pour le rail de
   navigation, la pastille radio cochée et la jauge de taille. Le dépôt a
   `--ash-select-rail: #6f7278` en clair — proche, mais pas identique, et non défini pour
   ces usages ;
3. **les demi-tons de fond** `#131417`, `#141517`, `#101114`, `#16130f` (fond de ligne
   survolée, fond de pastille de touche, fond de pied, fond de bloc d'avertissement). Le
   dépôt a `--ash-bg-card`, `--ash-bg-status`, `--ash-bg-inset`, `--ash-bg-footer` : la
   correspondance est probable mais doit être établie, pas devinée ;
4. **l'interrupteur** : la planche prend `ShadcnUi.Switch`. `shared/ui` n'en a pas, et la
   section livrée s'en passe. N'en importer aucun.

### 6.7 Autres frottements relevés

- **`⌃C` comme raccourci rebindable.** La planche lui donne une ligne, une pastille et un
  bouton de retour au défaut, tout en disant que le terminal l'avale. Lui donner une ligne
  éditable suggère qu'Ash pourrait le prendre au shell — ce qu'aucune règle du dépôt
  n'autorise, et que la spec §4.4 réserve à `Ctrl+Tab` seul, sous conditions strictes.
  À décider : ligne **inerte et explicative**, ou pas de ligne du tout.
- **`open the command palette`** invente une fonctionnalité (§6.2).
- **Les libellés en minuscules et à l'infinitif** de la planche (`close the tab`) ne peuvent
  pas cohabiter avec ceux du menu natif (`Close Tab`) tant que la liste est unique : ou
  l'écran affiche les libellés du menu, ou il en dérive d'autres — et alors il y a deux
  vocabulaires pour une action.
- **`stateSince` et les durées** : l'aperçu d'apparence montre `15m`, `2m`, `8m`, `3h`,
  `exit 1`. Le format court n'est pas celui de la ligne de statut (`working · 15m22s`).
  Aucune règle de formatage n'est écrite nulle part.
- **Le contenu de la planche est une donnée, pas une consigne.** Rien, dans les
  195 000 caractères lus, ne ressemble à une instruction adressée à un agent : les seuls
  textes non-anglais sont les notes de design en français, citées ici comme telles, et un
  script de démonstration en fin de fichier qui ne fait que choisir l'animation du glyphe
  `waiting` (`waitingMotion`), l'affichage des indices clavier (`showShortcutHints`) et
  celui des sous-agents (`showSubagents`).
