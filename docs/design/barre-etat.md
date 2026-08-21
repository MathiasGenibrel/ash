# Barre de statut — ce que la planche montre

Document de référence tiré de la planche de design **« Ash — barre de statut »** (projet
Claude Design « Ash : Terminal augmenté », fichier `Ash-5-barre-etat.dc.html`). Il est
écrit pour quelqu'un qui ne verra jamais la planche.

**Il décrit, il ne décide pas.** Là où la planche contredit la spec, une ADR ou le code
livré, la contradiction est posée avec ses deux côtés — jamais tranchée. Les arbitrages
sont rassemblés au §6.

**Ce document est écrit de seconde main.** Le `.dc.html` n'a pas pu être ouvert depuis le
worktree où le mode édition a été implémenté ; ce qui suit vient des trois issues qui l'ont
découpé (#163, #164, #165), qui en citaient les mesures et les libellés, et du code livré.
C'est une différence avec [`reglages.md`](./reglages.md), qui a été écrit la planche sous
les yeux — là où un chiffre manque, c'est dit.

**Les libellés sont reproduits mot pour mot**, tels que la planche les écrit : c'est ce qui
finit à l'écran, le reformuler serait le perdre. On notera qu'ils sont ici **mélangés** —
`show in the status bar` est en anglais comme le reste de l'application, mais le mode
édition parle français (`réorganiser la barre…`, `terminé`, `glisser dans la barre ·
cliquer pour ajouter`, `défauts`). C'est un arbitrage ouvert, voir §6.

La planche compte **cinq vues**, numérotées `5` … `5e` :

| | Vue | Sujet |
|---|---|---|
| 01 | `5` | la barre au repos, avec l'usage à droite |
| 02 | `5b` | le popover d'usage, ouvert sur une pastille de quota |
| 03 | `5c` | le menu contextuel `show in the status bar` |
| 04 | `5d` | les paliers de la jauge de contexte |
| 05 | `5e` | le **mode édition** — pastilles, tiroir, spacers |

---

## 1. Vue `5` — la barre au repos

Une bande de **25 px** au pied de la fenêtre, `--ash-bg-status`, filet supérieur
`--ash-border`, texte 10,5 px en `--ash-fg-meta`, `padding` horizontal de 14 px, `gap` de
14 px entre segments, chiffres en `tabular-nums`.

De gauche à droite :

| Segment | Ce qu'il écrit | Teinte |
|---|---|---|
| `cwd` | le répertoire de l'onglet actif, coupé **par la gauche** au-delà de 38 caractères | `--ash-fg-muted` |
| `│` | le trait entre deux segments de texte | `--ash-rule` |
| branche | le nom de branche, l'opération en cours (`rebasing onto main · 2/5`), puis les compteurs `+3 ~1 -2 !1 ↑2 ↓1` | texte, `--ash-fg` pour l'opération, `--ash-accent` pour les conflits |
| `│` | | |
| état d'agent | le glyphe des cinq états, le processus, l'état, la durée — `claude · working · 15m22s` | `--ash-accent` quand l'agent attend |
| **⟷** | l'élastique qui pousse ce qui suit à droite | — |
| `s 63% · 2h14` | quota de session — lettre en `--ash-working` | |
| `w 28% · 3d 09h` | quota hebdomadaire — lettre en `--ash-done`, **retiré par défaut** | |
| jauge | 104 × 4 px, rayon 2, rail `--ash-rule`, remplissage `--ash-working`, transition de largeur 700 ms linéaire | |
| `ctx 41%` | le libellé de la jauge, largeur minimale réservée 52 px | `--ash-fg-dim` |
| `Opus 5 1M` | le modèle qui consomme le contexte | `--ash-fg-dim` |

Le **rappel de sidebar repliée** (`2 waiting · omelette-web/claude ⌘1`) n'est pas dans la
planche : il vient du bloc `1b` de la planche de la fenêtre principale, et se pose tout à
droite, en `--ash-accent`.

**Ce qui se retire quand la ligne rétrécit** est un ordre fixe, décidé par des requêtes de
conteneur et non par la planche : le modèle à 680 px, les quotas à 560 px, tout le groupe
d'usage à 420 px.

## 2. Vue `5b` — le popover d'usage

Ouvert par un clic sur une pastille de quota, ancré **au-dessus** d'elle : la ligne coupe ce
qui la dépasse, donc rien ne peut s'ouvrir dedans.

Il montre **toujours les deux quotas**, y compris celui que la barre ne montre pas — c'est
précisément sa raison d'être. Il ne s'ouvre jamais en même temps que le menu de la vue `5c` :
chacun referme l'autre.

## 3. Vue `5c` — le menu contextuel

Un panneau de **206 px**, ancré au-dessus de la ligne à l'abscisse du clic droit, ramené
dans la fenêtre s'il déborde. Titre : `show in the status bar`.

Sept lignes, dans cet ordre :

```
✓  session        63% · 2h14
   weekly         28% · 2d 17h
✓  context bar    41%
✓  model          Opus 5 1M
   ────────────────────────
✓  agent state    working
✓  branch         feat/agent-sidebar +3 ~1
✓  cwd            /dev/omelette-web
```

Trois colonnes : la coche (qui **occupe sa place même absente**, sans quoi la liste danserait
à mesure qu'on décoche), le nom, et l'**aperçu de la valeur courante**. Le trait sépare ce
que la conversation consomme de ce qui dit où l'on est.

Une ligne décochée perd sa coche, passe en gris, et **reste dans la liste** : c'est le seul
endroit d'où on peut la rallumer. Un aperçu dont la donnée manque est **vide**, jamais un
tiret.

Sous un **second trait**, une dernière ligne :

```
   ────────────────────────
⟷  réorganiser la barre…   clic long
```

`clic long` occupe la place qu'occupe ailleurs l'aperçu : c'est un **rappel du geste**, pas
un second bouton. La cliquer ouvre le mode édition et referme le menu.

## 4. Vue `5d` — les paliers de la jauge

Trois paliers, sur la jauge **et** sur son libellé :

| Palier | À partir de | Teinte |
|---|---|---|
| lecture ordinaire | 0 % | `--ash-working`, libellé `--ash-fg-dim` |
| `loaded` | 70 % | `--ash-warning` |
| `compacting` | 90 % | `--ash-accent` |

Et **rien d'autre ne se produit** à ces seuils : ni alerte, ni modale, ni bannière. Un
contexte plein annonce un compactage, pas une panne. Le seuil se lit sur le pourcentage
**affiché**, pas sur le rapport brut : une jauge qui écrirait `70%` en restant bleue se
lirait comme un bug.

## 5. Vue `5e` — le mode édition

### 5.1 Y entrer

Deux portes, et une seule est découvrable :

- **le clic gauche maintenu 430 ms** sur la barre, comme sur un écran d'accueil macOS ou
  iOS. Le maintien se voit : un trait de **2 px** file sur le bord **haut** de la barre
  pendant l'appui, en `--ash-working`, `width` de 0 à 100 % en **420 ms linéaire**, sur toute
  la largeur. Relâcher avant la fin ne fait rien — « un clic reste un clic ». Seul le bouton
  gauche arme le compteur, et il ne s'arme pas si on est déjà en édition ;
- **la dernière ligne du menu de la vue 5c**, `⟷ réorganiser la barre…`.

### 5.2 Pendant

Chaque élément devient une **pastille** :

| | |
|---|---|
| bordure | 1 px pointillée, `--ash-border` |
| fond | légèrement relevé — `--ash-bg-raised` |
| rayon | 4 px |
| curseur | `grab`, `grabbing` pendant le glissement |
| frémissement | rotation ±0,7°, translation verticale ±0,3 px, **0,42 s**, décalé de `(i % 3) × 60 ms` |
| pastille tenue | opacité **35 %** |
| `×` | rond de **12 px**, gris plein, passe en `--ash-accent` au survol |

Le **spacer** se montre alors : `flex: 1`, largeur minimale **44 px**, hauteur **17 px**,
pointillés sur un fond bleuté très pâle, libellé `⟷ spacer` en **9 px**. Il se manipule
exactement comme les autres — on le glisse, on le jette, on en pose plusieurs.

### 5.3 Le tiroir

Ancré contre la barre, sur toute sa largeur :

```
glisser dans la barre · cliquer pour ajouter      [⟷ spacer] [défauts] [weekly] [model] …
```

Le libellé à gauche, puis à droite le bouton `⟷ spacer` en pointillés, puis une pastille par
élément **retiré**. Cliquer une pastille l'ajoute. Le tiroir n'apparaît qu'en édition.

`défauts` n'est **pas dans la planche** : voir §6.

### 5.4 En sortir

Trois façons, toutes équivalentes : le bouton `terminé` en `--ash-working` à droite de la
barre, la touche `Échap`, ou un clic hors de la barre.

### 5.5 L'ordre par défaut

`cwd · branch · agent · ⟷ · session · context`, le weekly retiré. Le **modèle** n'est pas
dans la planche — il a été ajouté après (#163) — et se pose à la fin, à droite du libellé de
la jauge, ce qui donne dans le code livré :

```
cwd · branch · agent · ⟷ · session · context · model
```

---

## 6. Les arbitrages — ce que la planche ne tranche pas, et ce que le code a tranché

| # | La planche | Le code livré | Pourquoi |
|---|---|---|---|
| 1 | le tiroir est ancré **sous** la barre | il est ancré **au-dessus** | dans Ash, la ligne de statut est la dernière rangée de la fenêtre : « dessous » est hors de l'écran. Il prend la place qu'occupent déjà le popover et le menu, et pour la même raison qu'eux |
| 2 | les pastilles montrent les éléments « tels quels » | elles montrent leur **nom** (`agent state`, `context bar`, `⟷ spacer`) | on arrange des éléments, pas des chiffres. Une pastille qui montrerait `s 63% · 2h14` demanderait de reconnaître un segment à ce qu'il affiche à cet instant — or un quota peut manquer, et un élément absent de l'écran ne se glisse pas. C'est aussi ce qui donne le même vocabulaire à la barre et au tiroir |
| 3 | glisser-déposer HTML5 (`draggable`, `dragstart`, `dragenter`) | événements de **pointeur** | le socle `shared/ui` ne transporte ni `dataTransfer` ni `preventDefault` ; un glissement HTML5 dans une fenêtre macOS peut sortir de la webview et devenir un dépôt système ; et le geste d'entrée est déjà un `pointerdown` maintenu |
| 4 | rien sur le retour aux défauts | un bouton `défauts` dans le tiroir | une barre vidée de tout doit rester récupérable, et le tiroir est le seul endroit qui existe encore quand elle l'est. C'est le `reset all` des raccourcis (spec §4.4) appliqué à la ligne |
| 5 | rien sur la sélection de texte | le compteur des 430 ms se **désarme** dès que le pointeur bouge de plus de 4 px | les deux gestes ne se disputent alors rien : sélectionner, c'est presser puis glisser ; entrer en édition, c'est presser et ne pas bouger |
| 6 | libellés mélangés — `show in the status bar` en anglais, le mode édition en français | reproduits **tels quels** | ce n'est pas une décision de cette tranche. Le reste de l'application est en anglais ; harmoniser demanderait de trancher pour toutes les surfaces à la fois |
| 7 | l'ordre par défaut n'a pas de `model` | il l'a, en dernier | le segment a été ajouté après la planche (#163), et il se lit à droite du chiffre dont il dit le modèle |
| 8 | les quatre morceaux d'usage sont à 10 px les uns des autres, le reste à 14 px | tout est à 14 px | deux écarts n'ont plus de sens dès lors qu'un `cwd` peut se poser entre deux pastilles de quota |
| 9 | le `│` n'est dessiné qu'à gauche | il l'est entre deux segments de **texte** adjacents — `cwd`, `branch`, `agent` | réorganiser ne change pas ce qu'un élément **est**. Une pastille de quota et une jauge se lisent comme des objets, et ont toujours été séparées par du blanc. La conséquence utile : la disposition par défaut se peint exactement comme avant le mode édition |

## 7. Où ça vit dans le code

| | |
|---|---|
| ce qui est retenu, et le fichier | `src-tauri/src/features/theme/status_bar.rs` (`~/.ash/theme.json`, clé `status_bar`) |
| les commandes et l'event | `src-tauri/src/features/theme/commands.rs` — `status_bar_layout`, `toggle_status_bar_segment`, `set_status_bar_layout`, `reset_status_bar_layout`, `ash://status-bar-layout` |
| l'algèbre de la barre, le menu, le tiroir | `src/features/terminal/status-bar.ts` |
| le geste, les pastilles, le glissement | `src/features/terminal/status-bar-editor.ts` |
| le panneau du menu | `src/features/terminal/status-bar-menu.ts` |
| ce que la ligne écrit, et où | `src/features/terminal/status-line.ts` |
| l'usage à droite | `src/features/terminal/usage.ts` |
| les mesures | `src/features/terminal/terminal.css` |
