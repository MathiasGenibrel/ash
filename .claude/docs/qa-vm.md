# La VM de QA — Ash-dev tourne ailleurs que sur ton bureau

> **État au 2026-08-28 : un cycle complet a tourné.** Tart 2.32.1, image
> `macos-sequoia-base` (macOS 15.7.7), sur un Mac Apple Silicon. `up → install → fixture →
> run → shot → down` a été joué de bout en bout, **aucune fenêtre n'est apparue sur le
> bureau de l'hôte**, et la capture montre les cinq états d'agent produits par `ash-event`
> seul, sans qu'aucun agent d'IA ne soit installé dans la VM.
>
> Les cinq points ouverts plus bas sont **levés** — ils disent maintenant ce qui a été
> observé. Quatre défauts du script ont été trouvés en l'exécutant ; ils sont corrigés, et
> chacun est expliqué à l'endroit où il vivait.

## Pourquoi

L'agent `qa` construit **et lance** Ash-dev. Le build ne dérange personne, c'est du CPU ; le
lancement, lui, prend le focus, le Dock et le WindowServer — de la machine qui sert de
terminal quotidien. C'est la raison pour laquelle `qa` est « sur demande », donc la raison
pour laquelle il tourne rarement.

Aucun pilote ne règle ça sur macOS. `tauri-driver` ne supporte que Linux et Windows (il
s'appuie sur `WebKitWebDriver` et `msedgedriver`, et le `safaridriver` d'Apple ne s'attache
pas à une `WKWebView` tierce) ; Playwright pilote des navigateurs, pas des applications
natives. Ce qui règle le problème n'est pas un outil de test, c'est **une seconde session
graphique**.

D'où la forme : **l'hôte construit, la VM lance.** `bun run package:debug` reste sur l'hôte,
où la toolchain est déjà là ; la VM n'a donc ni Rust, ni Xcode, ni second `target/` de
plusieurs gigaoctets.

## Le coût réel

- **Disque** : l'image de base `macos-sequoia-base` pèse **~30 à 50 Go** tirée, et la VM
  clonée en occupe autant à nouveau (le clone est paresseux au départ, mais grossit).
  Compte **60 à 100 Go** pour une image et une VM.
- **Le plafond d'Apple : 2 VM macOS par hôte.** C'est une limite du framework de
  virtualisation, pas de Tart. Si l'Ash de l'utilisateur, un autre projet ou une CI locale
  font déjà tourner deux VM macOS, `tart run` échouera — et le script rendra son erreur.
- **RAM et CPU** : la VM en prend une part réelle pendant le cycle. Le bureau reste utilisable,
  mais ce n'est pas gratuit.
- **Temps** : le premier `tart pull` se compte en dizaines de minutes sur une connexion
  ordinaire. Il n'est **jamais** déclenché par le script.

## Amorçage, une fois

```bash
brew install cirruslabs/cli/tart
tart pull ghcr.io/cirruslabs/macos-sequoia-base:latest   # des dizaines de Go, à faire en connaissance de cause
scripts/qa/vm.sh doctor                                   # dit ce qui manque encore
scripts/qa/vm.sh up                                       # clone puis démarre, sans écran
scripts/qa/vm.sh console                                  # ← la SEULE étape qui ouvre une fenêtre
```

`console` sert à accorder deux autorisations TCC **dans la VM**, ce qui ne peut pas se faire
par ssh : une autorisation se donne devant un écran.

1. Réglages système → Confidentialité et sécurité → **Accessibilité** → ajouter
   `/usr/libexec/sshd-keygen-wrapper` — sans quoi `System Events` refuse toute frappe
   (erreur `-1719`), donc aucun onglet ne s'ouvre.
2. Réglages système → Confidentialité et sécurité → **Enregistrement de l'écran** → le même —
   sans quoi `screencapture` rend un PNG où ne figure que le fond d'écran.

Cette préparation n'appartient à **aucun** cycle de QA : un cycle
`up → install → run → shot → down` ne l'appelle jamais, et n'ouvre donc aucune fenêtre.

## Un cycle

```bash
bun run package:debug                 # sur l'hôte — jamais dans la VM
scripts/qa/vm.sh up                   # démarre sans écran, rend l'adresse
scripts/qa/vm.sh install              # copie Ash-dev.app + pose le crochet de shell
scripts/qa/vm.sh fixture              # un dépôt git, deux worktrees
scripts/qa/vm.sh run                  # lance Ash-dev, ouvre cinq onglets, pose les cinq états
scripts/qa/vm.sh shot five-states     # → .qa-vm/shots/five-states.png
scripts/qa/vm.sh down
```

`up`, `install`, `fixture` et `down` sont idempotents.

### Les codes de retour sont une interface

Ils ne disent pas *où* ça a cassé mais **qui doit corriger** — c'est ce qui permet à l'agent
`qa` d'agir dessus au lieu de recopier un message d'erreur.

| Code | Ce qui s'est passé | Ce que l'appelant fait |
|---|---|---|
| `1` | usage — argument manquant, nom de capture ou verbe refusé | corrige l'appel ; rien à installer |
| `2` | prérequis manquant sur l'hôte (tart, image, build, `expect`) | `doctor` dit la commande à taper — **rends la main**, n'installe rien |
| `3` | tart n'a pas suivi : clonage, adresse, ssh, arrêt | c'est la VM, pas Ash — voir `.qa-vm/boot.log` |
| `4` | une étape a échoué **dans** la VM | c'est là, et seulement là, qu'un défaut d'Ash peut se lire |

La distinction qui compte est `2` contre `4` : un `2` n'est jamais un verdict sur le code de
la tâche, un `4` peut en être un.

Tout ce que le script accepte de la ligne de commande — un nom de capture, un rang d'onglet,
un verbe — est jugé **à un seul endroit** (`checked`, dans `vm.sh`), parce que ces valeurs
finissent dans un `bash -s` distant ou dans un AppleScript, où une valeur non jugée serait du
code. Même conduite que les trois frontières de sécurité du dépôt (`git_cli.rs`,
`token.rs`/`api.rs`, `links/target.rs`) : une fonction décide, tous les autres demandent.

## Les décisions, et pourquoi

**L'`.app` voyage par `rsync` sur ssh, pas par dossier partagé (`tart run --dir`).** Un
dossier virtiofs reste écrit par l'hôte pendant que le guest l'exécute — deux cycles de vie
pour un même bundle —, et `/Applications` est ce que LaunchServices indexe. Surtout, la VM
reste **autonome** une fois installée, donc un cycle est reproductible sans l'hôte.
`rsync` plutôt que `scp -r` parce que `scp -r` déréférence les liens symboliques du bundle
(`Frameworks`, `Resources`) : un bundle recopié en dur ne se lance pas.

**La quarantaine est vérifiée, pas supposée.** `com.apple.quarantine` est posé par
LaunchServices sur ce qu'un navigateur télécharge, et ni `scp` ni virtiofs ne le posent.
`install` lit donc l'attribut, dit ce qu'il a vu, et ne le retire que s'il est là.

**La VM se pilote par ssh, sans la moindre saisie.** Les images de Cirrus ouvrent un compte
`admin` avec ssh actif. La clé utilisée est **propre à la QA** (`.qa-vm/id_ed25519`) : la clé
quotidienne de l'utilisateur n'a rien à faire dans une VM jetable. Sa première installation
passe par `expect`, qui ship avec macOS — pas de `sshpass` à installer.

**Le dépôt de fixture est construit dans la VM, pas copié.** Un worktree git porte un `.git`
qui nomme son dépôt par **chemin absolu**, et le dépôt lui répond de même. Copier l'arbre
depuis l'hôte livrerait des pointeurs qui désignent des chemins de l'hôte : la résolution
worktree → dépôt (ADR-0011) verrait des dépôts cassés. Le construire sur place les rend
justes : un dépôt `hello` et deux worktrees `hello-feature-a` / `hello-feature-b`, ce qui
donne à la sidebar ses trois niveaux.

**Les identifiants d'onglet passent par un crochet de shell.** `ash-event` a besoin d'un
`--tab <id>`, et rien n'expose cet identifiant hors du PTY. Un bloc délimité et idempotent
dans le `~/.zshrc` de la VM fait écrire à chaque onglet son `ASH_TAB_ID` dans
`~/.ash-qa/tabs`. Le pilotage des états se fait alors **par ssh**, sans frappe : seule
l'ouverture des onglets (`⌘T`) et le `cd` restent des frappes, parce que rien d'autre ne les
déclenche depuis l'extérieur.

## Les cinq états, sans aucun agent d'IA

Ce n'est pas un contournement. [ADR-0007](../../docs/adr/0007-etats-par-hooks.md) pose qu'un
état vient d'un **hook** et **jamais** d'une analyse de la sortie du PTY : `ash-event` est
donc le chemin nominal, celui-là même que le bloc écrit dans un `settings.json` emprunte.

| État | Comment on l'obtient |
|---|---|
| `idle` | `ash-event session-start --tab <id>` — un verbe de **session**, pas un état |
| `working` | `ash-event working --tab <id>` |
| `waiting` | `ash-event waiting --tab <id>` |
| `done` | `ash-event done --tab <id>` |
| `error` | `ash-event error --tab <id>` |

**`idle` n'est pas déclarable**, et c'est délibéré : l'adaptateur `claude-code` refuse le mot
(`interpret` ne connaît que `working`, `waiting`, `done`, `error`). `idle` naît soit de la
sonde qui ne voit aucun agent, soit de l'**ouverture d'une session** — d'où `session-start`,
le verbe qu'écrit le hook `SessionStart`. C'est le seul des cinq qui demande autre chose
qu'un verbe d'état.

Deux pièges de temps :

- **`done` et `error` s'effacent d'eux-mêmes 30 s après avoir été vus** (`LINGER`, dans
  `agents/machine.rs`), et la fenêtre de la VM est au premier plan donc « vue » tout de
  suite. La capture doit suivre `run` immédiatement.
- `working` posé par hook peut être repris par la sonde si un vrai processus démarre dans
  l'onglet. Dans la VM il n'y en a pas.

`ash-event` accepte aussi `--sock <chemin>` : seconde couture, utile si un jour on veut
écrire dans un socket qui n'est pas celui de `~/.ash/`.

## Ce que ce chemin ne prouve pas

- **Aucun outil réel n'est reconnu ni instrumenté.** Ni Claude Code ni codex dans la VM :
  ADR-0006 (la reconnaissance par le chemin de l'exécutable) et l'écriture d'un bloc de
  hooks dans un `settings.json` restent hors de portée.
- **Rien sur les performances.** Une VM ne dit rien de fiable sur le coût de rendu de
  xterm.js — qui est le risque n°1 du projet.
- **Les quotas d'usage et le trousseau** n'ont pas de doublure ici : c'est la tâche jumelle
  (#190).
- **La CI n'est pas concernée** : cette VM est locale, le workflow reste sur `macos-15`.
- Ce n'est **pas** une suite E2E, et ça n'en tient pas lieu. « Monter un DOM dans `bun test` »
  reste une décision ouverte et séparée (`.claude/docs/testing.md`).

## Les cinq points, et ce que l'exécution a montré

Aucun n'était levé quand ce fichier a été écrit. Tous l'ont été depuis, le 2026-08-28.

1. **`screencapture` sans écran attaché — ça marche.** `tart run --no-graphics` n'ôte pas
   l'écran *virtuel* : il n'en affiche pas la fenêtre sur l'hôte. La capture rend un PNG
   complet de 1,7 Mo, bureau et Dock compris. Le repli « VM avec écran sur un Space séparé »
   n'a pas eu à servir, et le critère « aucune fenêtre » de #189 tient donc **entièrement**.
2. **La session graphique est déjà ouverte.** L'image `macos-sequoia-base` ouvre la session
   `admin` toute seule : `System Events` frappe et `screencapture` voit, sans rien préparer.
   C'était l'inconnue la plus sérieuse — elle se multipliait avec la précédente.
3. **Les autorisations TCC n'ont pas été nécessaires** sur cette image : `run` et `shot` ont
   abouti sans passer par `console`. À reposer sur une autre image ou une autre version de
   macOS ; le message d'erreur du script reste la bonne porte si ça change.
4. **Le dialogue de notifications ne s'est pas présenté** au premier `run`. Le clic
   AppleScript reste en place — il est sans effet quand le dialogue est absent, et c'est la
   conduite voulue.
5. **La quarantaine est bien absente** d'une copie par `rsync` sur ssh : `install` le
   vérifie et le dit (« quarantaine absente, comme attendu d'une copie par ssh »). Observé,
   plus supposé.

## Les quatre défauts trouvés en exécutant

Aucun ne pouvait se voir à la lecture, et c'est l'intérêt du cycle réel.

1. **`Too many authentication failures`.** `-i <clé>` **ajoute** une identité, il n'en
   restreint aucune : ssh proposait d'abord toutes les clés de l'agent de l'utilisateur, et
   le `sshd` de la VM atteignait son `MaxAuthTries` avant d'essayer la nôtre. Corrigé par
   `IdentitiesOnly=yes` et `IdentityAgent=none`, aux deux endroits qui parlent à la VM.
2. **`up` n'était pas idempotent.** `vm_running` cherchait `"Name":"…"` dans
   `tart list --format json` ; tart **indente** son JSON (`"Name" : "ash-qa"`), donc le motif
   ne matchait jamais et `up` croyait toujours devoir démarrer. Corrigé en passant par
   `tart get "$VM_NAME"`, qui prend le nom en **argument** : il n'y a plus rien à apparier.
3. **Un seul verbe partait sur cinq.** `ash-event` lit son **entrée standard** pour en tirer
   `agent_id` / `agent_type` (ADR-0007, amendement du 2026-08-13) : dans
   `while read -r tab; do … done <"$tabs"`, il héritait du fichier des onglets et en avalait
   les lignes restantes. La boucle s'arrêtait au premier tour — et annonçait quand même
   « cinq verbes envoyés ». Corrigé par une redirection depuis `/dev/null`, et par un garde
   qui compte les envois au lieu de les supposer.
4. **`pgrep -x Ash-dev` ne matchait jamais** : l'exécutable du bundle s'appelle `ash`. Chaque
   `run` ouvrait donc une instance de plus — treize onglets après trois cycles. On reconnaît
   maintenant la nôtre par son chemin complet.

## Exercer les doublures d'usage (#190) dans la VM

`run` transmet `ASH_DEV_USAGE` de l'hôte à la VM, quand elle est **non vide** — une variable
présente mais vide est un refus explicite côté Ash, et l'application s'arrête au démarrage
(voir `features/usage/rehearsal.rs`).

```bash
ASH_DEV_USAGE="keychain=readable,host=ok,session=95@2m,weekly=28@3d" scripts/qa/vm.sh run
```

**`launchctl setenv` depuis une session ssh ne remonte pas jusqu'au domaine graphique** :
c'est pour ça que `run` lance le binaire du bundle directement, avec la variable en ligne,
au lieu de passer par `open`. C'est le seul chemin qui la fasse arriver.

Vérifié à l'écran le 2026-08-28 : la barre de statut affiche `s 95% · 1m`, la section
`usage` des réglages dit « ash can read claude code's token » ; avec `keychain=refused` elle
dit « the keychain did not give up claude code's token » en rouge ; et avec
`host=unreachable`, alors même que la variable décrit 95 %, la barre de statut **n'affiche
plus rien** — la valeur disparaît, ni zéro ni tiret, comme ADR-0016 le demande.
