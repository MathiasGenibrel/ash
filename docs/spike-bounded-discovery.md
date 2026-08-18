# Spike — la découverte bornée suffit-elle ?

> Mesures du 2026-08-18, sur le Mac de l'auteur (Apple Silicon, APFS, SSD interne,
> volume de 926 Gio dont 520 utilisés). Issue #62, jalon J4, fait suite à #61 et #120.
> Verdict : **l'escalade vers Full Disk Access n'est pas justifiée**. La ligne qui close
> la question est dans [`spec.md` §6.1](./spec.md).

#61 découvre les outils par le `PATH`, par les dossiers conventionnels de `$HOME` et par
la sonde, sans demander aucune permission macOS. Ce document instruit la question
suivante : est-ce que ça suffit, ou existe-t-il des cas réels qui y échappent et qui
justifieraient de réclamer Full Disk Access ?

L'escalade coûte cher — accès à Mail, Messages, l'historique Safari, les sauvegardes
Time Machine et les données confinées des autres applications ; pas de demande par
programme, donc un détour par Réglages Système et un relancement ; et pour un terminal,
un coût de confiance que rien ne rembourse. Elle ne se justifierait donc que si la
découverte bornée laissait un trou **mesuré**.

---

## 1. Ce que la découverte actuelle parcourt exactement

Avant de dire ce qu'elle rate, il faut être précis sur ce qu'elle fait — et l'énoncé de
l'issue en donne une image légèrement trop large. **Ash ne parcourt rien.** Il n'y a
aujourd'hui aucun parcours de `$HOME`, à aucune profondeur. Il y a trois lectures
ponctuelles :

| Où | Quoi | Code |
|---|---|---|
| Rien, en mémoire | La table embarquée des outils connus, comparée au chemin / nom / `argv[0]` que la sonde a rendus. Fonction pure. | `agents/providers.rs`, `KNOWN_PROVIDERS` |
| Un `read_dir`, **un seul** | Le dossier conventionnel de l'adaptateur, et seulement pour proposer une valeur de champ quand l'écran s'ouvre sur un geste | `settings/verification.rs`, `Verifier::proposed_config` |
| Le `PATH` | Un `stat` par entrée du `PATH` pour un nom de commande donné (test 3) | `settings/system.rs`, `SystemCommands::locate` |

Une quatrième lecture existe — celle du `settings.json` d'un dossier déjà déclaré, pour
savoir s'il porte le marqueur `# ash:hook v` — mais elle est bornée au dossier que
l'entrée nomme, et mise en cache 5 s (`settings/recognition.rs`, `FRESHNESS`).

La table embarquée ne contient **qu'un seul dossier conventionnel** aujourd'hui :
`~/.claude`, porté par le profil `claude-code` assemblé dans `lib.rs::embedded_adapters`.
`generic` n'en a aucun, par construction.

---

## 2. Où les quatre outils rangent réellement leur configuration

`✓ machine` = vérifié sur ce disque le 2026-08-18. `✓ doc` = lu dans la documentation
officielle, référence et date citées. Rien ici n'est supposé sans être marqué comme tel.

### claude — Claude Code

| Chemin | Profondeur depuis `$HOME` | Source |
|---|---|---|
| `~/.claude/` (avec `settings.json`, `projects/`) | **1** | ✓ machine, ✓ doc |
| `~/.claude.json` (OAuth, serveurs MCP) — un fichier, pas un dossier | 1 | ✓ machine (134,9 Kio), ✓ doc |
| `~/.claude-perso/` — un second compte, dossier arbitraire imposé par `CLAUDE_CONFIG_DIR` | 1 | ✓ machine |
| `<projet>/.claude/settings.json` — configuration de dépôt | hors `$HOME` conventionnel | ✓ machine (4 dépôts), ✓ doc |
| `/Library/Application Support/ClaudeCode/managed-settings.json` — politique d'entreprise | hors `$HOME` | ✓ doc ; **absent** de cette machine (✓ machine) |
| `CLAUDE_CONFIG_DIR` déplace le dossier utilisateur n'importe où | — | ✓ doc |

Source : <https://code.claude.com/docs/en/settings>, consultée le 2026-08-18.

### codex — Codex CLI (OpenAI)

| Chemin | Profondeur | Source |
|---|---|---|
| `~/.codex/config.toml` | **1** | ✓ machine, ✓ doc |
| `$CODEX_HOME/config.toml` — déplace tout le dossier ; l'exemple de la doc est `$HOME/dotfiles/codex` | — | ✓ doc |
| `<projet>/.codex/config.toml` — chargé si le dépôt est approuvé | hors `$HOME` | ✓ doc |

Sources : <https://developers.openai.com/codex/config-basic> et
<https://github.com/openai/codex/blob/main/docs/config.md>, via recherche du 2026-08-18.
`~/.config/codex/` **n'existe pas** sur cette machine, et la doc ne le mentionne pas.

### kimi — Kimi Code CLI (Moonshot)

| Chemin | Profondeur | Source |
|---|---|---|
| `~/.kimi-code/` — `config.toml`, `tui.toml`, `mcp.json`, `credentials/`, `sessions/`, `bin/` | **1** | ✓ machine, ✓ doc |
| `$KIMI_CODE_HOME` déplace l'intégralité de la racine de données | — | ✓ doc |

Source : <https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/data-locations.html>,
consultée le 2026-08-18. La correspondance est exacte : les six sous-dossiers documentés
sont ceux trouvés sur le disque.

### opencode

| Chemin | Profondeur | Source |
|---|---|---|
| `~/.config/opencode/opencode.json` | **2** | ✓ machine, ✓ doc |
| `~/.config/opencode/tui.json`, `agents/`, `commands/`, `skills/`, `themes/`… | 2 | ✓ doc |
| `~/.local/share/opencode/` — base SQLite, journaux | 3 | ✓ machine (non documenté sur cette page) |
| `~/.local/state/opencode/locks/` | 3 | ✓ machine |
| `OPENCODE_CONFIG` / `OPENCODE_CONFIG_DIR` déplacent le fichier ou le dossier | — | ✓ doc |
| `/Library/Application Support/opencode/` — configuration administrée | hors `$HOME` | ✓ doc ; **absent** ici (✓ machine) |

Source : <https://opencode.ai/docs/config/>, consultée le 2026-08-18.

`XDG_CONFIG_HOME` n'est pas défini sur cette machine (✓ machine) ; `~/.config` est donc le
défaut, et c'est bien là qu'opencode s'est installé.

### Le compte

**Sur quatre outils, un seul range sa configuration au-delà du premier niveau de
`$HOME` : opencode, à `~/.config/opencode`, profondeur 2.**

Et il faut être exact sur ce que ça veut dire : `~/.config/opencode` n'est pas une
découverte à faire, c'est une **constante documentée**. L'atteindre demande une ligne de
plus dans la table des adaptateurs, pas un parcours de disque, et donc **aucune
permission**. Aucun des quatre outils ne range quoi que ce soit dans un emplacement
protégé par TCC.

---

## 3. Le coût en temps d'un scan plus large — mesuré

Toutes les mesures ci-dessous ont été prises sur ce disque, en lecture seule, avec la
commande exacte donnée. Trois passages consécutifs par profondeur ; le premier est le plus
froid dont on dispose — le cache d'inœuds ne se purge pas sans `sudo`, ce qui est noté en
§6 comme une limite.

```sh
/usr/bin/time -p sh -c 'find "$HOME" -maxdepth N -type d > /dev/null 2> err.txt'
```

| Profondeur | Dossiers atteints | run 1 (froid) | run 2 | run 3 |
|---:|---:|---:|---:|---:|
| 1 | 66 | 0,00 s | 0,00 s | 0,00 s |
| 2 | 426 | 0,04 s | 0,00 s | 0,01 s |
| 3 | 5 194 | 0,56 s | 0,03 s | 0,03 s |
| 4 | 9 373 | 0,99 s | 0,23 s | 0,24 s |
| 5 | 89 792 | 4,80 s | 2,47 s | 0,63 s |
| 6 | 194 960 | 14,89 s | 15,55 s | 16,98 s |
| 8 | 318 903 | 36,91 s | 37,40 s | 39,60 s |
| 20 (≈ complet) | 614 917 | **78,67 s** | 72,98 s | 73,79 s |

Le même parcours complet en écartant `~/Library` — 444 440 dossiers — coûte 48,74 s puis
53,73 s :

```sh
/usr/bin/time -p sh -c 'find "$HOME" -name Library -prune -o -type d -print > /dev/null'
```

Un scan ciblé sur des motifs de fichiers plutôt que sur les dossiers ne change pas
l'ordre de grandeur :

| Recherche | Durée | Trouvés |
|---|---:|---:|
| `find "$HOME" -maxdepth 5 -name settings.json -type f` | 5,52 s | 19 |
| `find "$HOME" -maxdepth 5 -name config.toml -type f` | 2,33 s | 5 |

Et l'autre bout de l'échelle, pour comparaison — c'est ce que fait Ash aujourd'hui, et ce
que ferait une découverte bornée un peu élargie :

| Geste | Durée (2 passages) |
|---|---:|
| `ls ~/.claude` — la lecture unique de `proposed_config` | 0,00 s / 0,00 s |
| `command -v` sur les quatre commandes — le parcours du `PATH` | 0,00 s / 0,00 s |
| **16 racines candidates connues, lues une fois chacune** | **0,03 s / 0,02 s** |
| `find` profondeur 2 sur `~/.config` et `~/Library/Application Support` | 0,09 s / 0,00 s |

Les seize racines mesurées sont `~/.claude`, `~/.claude-perso`, `~/.codex`,
`~/.kimi-code`, `~/.config/opencode`, `~/.config/codex`, `~/.config/claude`,
`~/.local/share/opencode`, `~/.local/share/claude`, `~/.local/state/opencode`,
`~/.cache/claude`, `~/.cache/opencode`, `~/.gemini`, `~/.copilot`, `~/.junie`,
`~/.agents`.

**Le rapport est de 1 à 2 500 entre lire seize emplacements connus (30 ms) et parcourir
`$HOME` (74 s), pour une réponse qui n'est pas meilleure.**

---

## 4. Ce qu'un scan trouverait vraiment — et ce qu'il ferait trouver de faux

Les 19 `settings.json` remontés à profondeur 5 se répartissent ainsi :

- **2** sont de vraies configurations utilisateur de Claude Code : `~/.claude` et
  `~/.claude-perso` — les deux à **profondeur 1**, donc déjà à portée ;
- **4** sont des `<projet>/.claude/settings.json`, c'est-à-dire des fichiers **versionnés
  dans un dépôt**. Les instrumenter serait écrire le chemin absolu de `ash-event` dans le
  dépôt d'un utilisateur : exactement ce qu'ADR-0007 interdit ;
- **12** appartiennent à d'autres applications — `ccstatusline`, `copilot`, `gemini`,
  `arduinoIDE`, `OpenVPN Connect`, `Postman`, `Figma`, `Code - Insiders`, `smithery`,
  `.t3`, `.astro`, `.vscode` ;
- **1** est un faux positif qui **passerait la vérification** : `~/.lmstudio/settings.json`
  a un dossier `projects/` frère, donc il satisfait le test 2 tel qu'il est écrit
  (`signature: ["projects"]`). Un scan large le proposerait comme une configuration Claude
  Code plausible.

Les 5 `config.toml` : deux vrais (`~/.kimi-code`, `~/.codex`, tous deux à profondeur 1),
trois sans rapport (`mise`, `docker`, `rtk`).

Autrement dit, **le scan large ne rapporte rien que la table ne sache déjà nommer, et
rapporte du bruit que les quatre tests ne filtrent pas.**

---

## 5. Ce que Full Disk Access débloquerait — mesuré, lui aussi

Le parcours complet de `$HOME` s'est heurté à **156 refus** (`Operation not permitted`),
que la sortie d'erreur nomme un par un. Aucune invite macOS n'est apparue : sans
autorisation, le noyau rend `EPERM` et le parcours continue. La liste est intégralement
composée de données personnelles Apple :

`~/.Trash`, `Library/Mail`, `Library/Messages`, `Library/Safari`,
`Library/Containers/com.apple.mail`, `…/com.apple.MobileSMS`, `…/com.apple.Notes`,
`…/com.apple.Safari`, `…/com.apple.Home`, `Library/Calendars`,
`Library/Application Support/AddressBook`, `…/CallHistoryDB`, `…/MobileSync`,
`…/com.apple.TCC`, `Library/Cookies`, `Library/Biome`, `Library/Suggestions`,
`Library/PersonalizationPortrait`, `Library/Caches/com.apple.findmy.*`, etc.

**Aucun de ces 156 emplacements ne contient la configuration d'un outil de code, et aucun
ne pourrait en contenir** : ce sont des conteneurs d'applications Apple, dont un CLI
tiers ne peut pas écrire.

Full Disk Access donne accès aux dossiers protégés. Il ne donne **aucune connaissance
supplémentaire** de l'endroit où un outil a mis sa configuration.

---

## 6. Le seul trou réel — et ce qui le bouche (ce n'est pas FDA)

Les quatre outils acceptent une variable d'environnement qui déplace leur dossier de
configuration où l'utilisateur veut, y compris hors de `$HOME` :
`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `KIMI_CODE_HOME`, `OPENCODE_CONFIG_DIR` (✓ doc, les
quatre). Le cas n'est pas théorique : `~/.claude-perso` sur cette machine est exactement
ça, et l'exemple donné par la doc de Codex est `$HOME/dotfiles/codex`.

C'est le seul cas qu'une découverte bornée rate. Et il faut voir ce qu'il implique :

- **Un scan ne le résout pas non plus.** Un dossier arbitraire ne se reconnaît que par sa
  forme, et §4 vient de montrer que la reconnaissance par la forme produit des faux
  positifs (`~/.lmstudio`) tout en ne pouvant pas distinguer une configuration utilisateur
  d'un `.claude/` versionné dans un dépôt.
- **FDA ne le résout pas du tout** : le dossier visé n'est pas protégé, il est
  simplement inconnu.
- **Il existe une réponse exacte et sans permission** : la variable est dans
  l'environnement du processus, qu'Ash a lui-même lancé, et que `libproc` rend pour un
  processus du même utilisateur — le même mécanisme qu'ADR-0005 emploie déjà pour le
  `cwd`. Ce n'est pas une inférence sur le disque, c'est la valeur que l'outil utilise
  vraiment.

Cette piste-là mérite peut-être une issue ; ce spike ne la tranche pas, et ne l'implémente
pas. Elle est notée ici pour que « la découverte bornée rate un cas » ne se relise jamais
comme « il fallait donc FDA ».

---

## 7. Ce que l'utilisateur perd concrètement à taper un chemin à la main

Décrit à partir du code livré par #61 et #120, pas d'un parcours imaginé.

Le marqueur discret d'un agent reconnu mais non instrumenté ouvre la fenêtre de réglages
sur cet outil (`settings/index.ts::focusTool`). Le formulaire arrive **déjà rempli** :
`command`, `label` et `adapter` viennent de la reconnaissance, et `config` est demandé au
backend (`settings_proposed_config`). La séquence des quatre tests part immédiatement
(`relaunch.now`), sans attendre une frappe.

Deux cas, et ils sont très inégaux :

**Le dossier conventionnel existe** — c'est `~/.claude` sur cette machine. Le champ est
pré-rempli, les quatre tests passent, l'utilisateur n'a **rien à taper**. Zéro geste
perdu.

**L'adaptateur n'a pas de dossier conventionnel, ou il est absent.** `proposed_config`
rend `None` — la règle est explicite dans `verification.rs` : on ne propose que ce que le
test 1 accepterait, parce qu'« une proposition qui échoue est pire qu'un champ vide ». Le
champ reste vide, et l'utilisateur tape le chemin. Ce que ça lui coûte :

- une ligne de chemin à écrire, avec `~` accepté ;
- 400 ms après la dernière frappe (`RELAUNCH_DELAY`), les quatre tests se relancent seuls ;
- si le chemin est faux, le test 1 dit `nothing at <chemin>` et **propose le défaut de
  l'adaptateur en un clic** (`FixAction::UseFolder`) ;
- si l'outil doit être lancé (test 4), le délai est borné à 5 s (`PROBE_TIMEOUT`) ;
- c'est fait **une fois**, et l'entrée est persistée dans `~/.ash/config.toml`.

Et il y a un fait qui vide la question de sa substance : **aujourd'hui, ce cas ne concerne
aucun outil qu'Ash puisse instrumenter.** Le seul adaptateur qui pose des hooks est
`claude-code`, et son dossier conventionnel `~/.claude` est à profondeur 1, pré-rempli
depuis #120. Les trois autres outils tombent sur `generic`, dont
`GenericAdapter::instrumentation` rend `None` — donc `Instrumented::Unsupported` : leur
saisir un chemin ne poserait aucun hook, et l'écran le dit déjà au lieu de le laisser
croire.

**Le geste qu'on éviterait en escaladant est donc, à ce jour, un geste que personne n'a à
faire.**

---

## Verdict

**L'escalade vers Full Disk Access n'est pas justifiée. La question est close.**

Ce qui la fonde, dans l'ordre :

1. **Un seul emplacement sur quatre est hors du premier niveau de `$HOME`**
   (`~/.config/opencode`), et c'est une constante documentée : l'atteindre est une ligne
   dans la table des adaptateurs, pas une permission.
2. **Aucun des quatre outils ne range quoi que ce soit dans un emplacement protégé par
   TCC.** Les 156 refus mesurés sur un parcours complet de `$HOME` sont tous des données
   personnelles Apple — Mail, Messages, Safari, Notes, Contacts, HomeKit, Localiser.
   C'est très exactement ce que FDA débloquerait, et rien de ce qu'on cherche.
3. **Le coût est de 1 à 2 500** : 30 ms pour lire seize emplacements connus, 74 s pour
   parcourir 614 917 dossiers — pour une réponse qui n'est pas meilleure, et qui est même
   pire (19 `settings.json` remontés à profondeur 5, dont 2 vrais, 4 fichiers versionnés
   qu'il serait fautif d'écrire, et 1 faux positif qui passerait la vérification).
4. **Ce que la découverte bornée rate — un dossier déplacé par variable
   d'environnement — n'est pas ce que FDA débloque.** Ce cas se lit dans l'environnement
   du processus, sans aucune permission.
5. **Le coût pour l'utilisateur est déjà nul dans le seul cas qui compte** : `claude-code`
   est le seul adaptateur qui instrumente, et son champ est pré-rempli depuis #120.

Conformément à l'énoncé, ce spike ne produit donc pas d'issue de suivi mais une ligne dans
la spec (§6.1) qui ferme la question.

---

## Ce qui reste non vérifié

- **Le cache disque n'a pas été purgé** (`sudo purge` indisponible sans mot de passe). Le
  « run 1 » de chaque profondeur est le plus froid dont on dispose, pas un vrai départ à
  froid. Les durées réelles à froid seraient donc **plus longues**, ce qui ne fait que
  renforcer le verdict.
- **Un seul disque, un seul utilisateur.** 926 Gio, 614 917 dossiers, un profil de
  développeur lourdement outillé. Un disque plus petit scannerait plus vite ; l'ordre de
  grandeur — dizaines de secondes contre dizaines de millisecondes — ne changerait pas.
- **`codex` et `opencode` ne sont pas dans le `PATH` de cette machine**, alors que leurs
  dossiers de configuration existent. Leur emplacement est donc vérifié sur le disque,
  mais le test 3 de la vérification n'a pas été exercé sur eux en conditions réelles.
- **Les chemins d'entreprise n'ont pas été observés** :
  `/Library/Application Support/ClaudeCode/managed-settings.json` et
  `/Library/Application Support/opencode/` sont absents ici. Ils sont hors de `$HOME`,
  lisibles sans FDA, et Ash n'a de toute façon pas à y écrire.
- **Le processus de mesure n'est pas Ash.** Le shell qui a lancé ces `find` peut avoir des
  autorisations « Fichiers et dossiers » (`~/Documents`, `~/Desktop`, `~/Téléchargements`)
  qu'un `Ash.app` neuf n'aurait pas. Ces trois dossiers relèvent d'une catégorie TCC
  distincte de FDA, avec une invite par dossier — et aucun des quatre outils n'y range sa
  configuration, donc le point ne change pas le verdict.
- **Quatre outils, pas la population des outils de code.** Rien n'a été mesuré pour
  `gemini`, `copilot`, `junie`, `cursor` — dont ce disque porte pourtant les dossiers, tous
  à profondeur 1 de `$HOME` (✓ machine). C'est un indice, pas une preuve.

## Rejouer les mesures

```sh
# Le coût d'un scan par profondeur
for d in 1 2 3 4 5 6 8 20; do
  /usr/bin/time -p sh -c "find \"\$HOME\" -maxdepth $d -type d > /dev/null 2> /tmp/err$d.txt"
  wc -l < /tmp/err$d.txt   # les refus TCC
done

# Ce qu'un scan remonterait
find "$HOME" -maxdepth 5 -name settings.json -type f 2>/dev/null
find "$HOME" -maxdepth 5 -name config.toml   -type f 2>/dev/null

# Ce que fait Ash aujourd'hui
ls ~/.claude; for c in claude codex kimi opencode; do command -v "$c"; done
```

Tout est en lecture seule. Aucune de ces commandes ne déclenche d'invite macOS : les
dossiers protégés rendent `EPERM` sans rien demander.
