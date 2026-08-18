# Spike — ce que `codex` expose de son cycle de vie

> Enquête du 2026-08-18. Issue #21, jalon J4. Verdict : **hook**. Codex CLI a un système
> de hooks de cycle de vie complet, de la même famille que celui de Claude Code, avec un
> événement dédié à l'attente d'approbation. L'adaptateur s'écrit **sans toucher au cœur**.

Ce document répond à la **question ouverte n°1** de la [spec §12](./spec.md) — « ce que
codex, kimi et opencode exposent réellement » — pour `codex` seul. Il dit ce qui a été
vérifié, comment, et ce qui reste à vérifier sur une machine où `codex` tourne vraiment.

## 1. La question, et pourquoi elle bloque le jalon

Toute la conception des états d'Ash repose sur des hooks
([ADR-0007](./adr/0007-etats-par-hooks.md)) : `waiting` — le seul état qui justifie
d'interrompre l'utilisateur, donc le cœur du produit — **n'a jamais d'autre source qu'un
hook**. Un outil sans point d'instrumentation tombe sur `generic`, et son utilisateur ne
voit que ce que la sonde sait dire : `idle`, `working`, `done`, `error`. Jamais `waiting`,
donc jamais de notification.

Le critère de sortie du jalon J4, posé par
[ADR-0008](./adr/0008-abstraction-adapter.md), est **« un deuxième outil supporté sans
modification du cœur »**. Il n'est vérifiable que si un deuxième outil expose quelque
chose. Tant que la réponse n'est pas écrite, on ne sait ni si le trait `Adapter` tient sa
promesse, ni s'il faut un second moteur d'états heuristique — explicitement écarté
jusqu'ici, et qui contredirait ADR-0007.

D'où l'ordre imposé au ticket : enquête d'abord, code ensuite.

## 2. Ce qui a été vérifié, et ce qui ne l'est pas

### 2.1 Sur la machine — vérifié

| Fait | Comment | Résultat |
|---|---|---|
| `codex` n'est pas dans le `PATH` | `command -v codex` | absent |
| aucun binaire `codex` sur le disque aux endroits conventionnels | `ls /opt/homebrew/bin`, `/usr/local/bin`, `npm root -g`, `~/.bun/install/global/node_modules`, `~/.cargo/bin`, `/Applications` | absent partout |
| `~/.codex/` **existe** | `ls -la ~/.codex` | `config.toml`, `sessions/`, `skills/`, `shell_snapshots/`, `logs_2.sqlite`, `state_5.sqlite`, `memories_1.sqlite`, `goals_1.sqlite`, `installation_id` |
| une session y a tourné | `~/.codex/sessions/2026/08/17/rollout-2026-08-17T16-22-12-*.jsonl` | 9 lignes, 25 Kio |
| la version utilisée | `sqlite3 ~/.codex/state_5.sqlite "select cli_version, source, thread_source from threads"` | `0.144.1`, `source=exec`, `thread_source=user` |
| `~/.codex/config.toml` ne contient qu'un `[mcp_servers]` vide | `cat` | aucun hook déclaré, aucun `notify` |
| il n'y a **pas** de `~/.codex/hooks.json` | `ls -la ~/.codex` | absent |

**Aucune écriture, aucune commande `codex` lancée** — l'outil n'est de toute façon plus
installé. Tout ce qui précède est de la lecture de fichiers déjà présents.

Le format du fichier de session, lu ligne à ligne (`~/.codex/sessions/…/rollout-*.jsonl`,
une ligne de JSON par événement, champ `type`) :

| `type` | Charge utile observée |
|---|---|
| `session_meta` | `session_id`, `cwd`, `originator` (`codex_sdk_ts`), `cli_version`, `source`, `model_provider`, les instructions de base |
| `turn_context` | `turn_id`, `cwd`, `approval_policy`, `sandbox_policy`, `model`, `multi_agent_version: "v1"` |
| `world_state` | l'environnement, les skills disponibles |
| `response_item` | les messages échangés avec le modèle |
| `event_msg` | `task_started` / `user_message` / `task_complete` — avec `turn_id`, `started_at`, `completed_at`, `duration_ms` |

La session observée est une session `exec` triviale (« What is 2+2? »), sans approbation :
elle **ne prouve donc pas** l'absence d'événements d'attente dans ce format, seulement
qu'aucun n'est apparu là où il n'y avait rien à approuver.

La base `state_5.sqlite` porte une table `threads` (avec `rollout_path`, `cwd`, `git_branch`,
`updated_at_ms`), des tables `agent_jobs` / `agent_job_items` / `thread_spawn_edges`, et un
champ `multi_agent_version` dans le contexte de tour : Codex a bien une notion de
sous-agents en 0.144.

### 2.2 Dans la documentation officielle — vérifié

Sources consultées le **2026-08-18** :

- `https://raw.githubusercontent.com/openai/codex/main/docs/config.md` — devenu un renvoi de
  15 lignes vers `developers.openai.com`, plus une section « Lifecycle hooks » qui mentionne
  `allow_managed_hooks_only` dans `requirements.toml`. C'est ce renvoi qui a mis sur la piste.
- `https://developers.openai.com/codex/hooks` — **redirection 308** vers
  `https://learn.chatgpt.com/docs/hooks`. C'est la page de référence des hooks de Codex.
- `https://github.com/openai/codex/blob/main/codex-rs/hooks/src/engine/command_runner.rs`
  (lu via l'API GitHub) — la façon dont un hook est **exécuté**.
- `https://github.com/openai/codex/blob/main/codex-rs/protocol/src/shell_environment.rs` —
  la liste des variables d'environnement **non héritées** par les enfants.
- `https://github.com/openai/codex/blob/main/codex-rs/hooks/src/engine/discovery.rs` —
  la découverte de `hooks.json` à côté d'un dossier de configuration actif.
- `https://api.github.com/repos/openai/codex/commits?path=codex-rs/hooks/src/lib.rs` — le
  crate `codex-hooks` a été **extrait** dans son propre crate le **2026-02-10**
  (« Extract hooks into dedicated crate (#11311) ») : les hooks lui préexistent donc.
- `https://github.com/openai/codex/blob/main/codex-rs/hooks/src/legacy_notify.rs` — le
  mécanisme `notify`, et son unique variante d'événement.
- `https://github.com/openai/codex/blob/main/codex-rs/protocol/src/protocol.rs` — les ~80
  variantes d'`EventMsg`, dont `exec_approval_request` et `apply_patch_approval_request`.
- `https://learn.chatgpt.com/docs/config-file/config-advanced` — `notify`, et les
  notifications de la TUI.
- `https://learn.chatgpt.com/docs/changelog` — la version stable publiée la plus récente est
  `0.147.0` (2026-08-07), qui porte encore du travail sur les hooks et sur le multi-agent v2.

**Les onze événements de cycle de vie de Codex** (page `hooks`, section « Hooks run at
different points in a conversation ») :

| Moment | Événements |
|---|---|
| pendant un tour | `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PreCompact`, `PostCompact`, `UserPromptSubmit`, `SubagentStop`, `Stop` |
| au démarrage d'une session ou d'un sous-agent | `SessionStart`, `SubagentStart` |
| à la fin du fil principal | `SessionEnd` (ne part pas pour les sous-agents) |

**Où ils se déclarent** — quatre emplacements, dont deux sous le foyer de l'utilisateur :

> `~/.codex/hooks.json`, `~/.codex/config.toml`, `<repo>/.codex/hooks.json`,
> `<repo>/.codex/config.toml`

**La forme de `hooks.json`**, citée de la documentation :

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          { "type": "command", "command": "/usr/bin/python3 …", "timeout": 30 }
        ]
      }
    ]
  }
}
```

C'est, à la clé de tête près, **exactement** la forme du `settings.json` de Claude Code :
un objet `hooks`, une clé par événement, un tableau de groupes, chaque groupe portant un
`matcher` facultatif et un tableau de handlers `{ "type": "command", "command": … }`.

**Comment un hook reçoit sa charge** :

> « Every command hook receives one JSON object on `stdin`. »

Les champs communs : `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `model`, et
`permission_mode` pour la plupart des événements. Les événements de tour ajoutent `turn_id`.
`SubagentStart` et `SubagentStop` ajoutent **`agent_id` et `agent_type`** — les deux clés
exactes qu'`ash-event` lit déjà sur son entrée standard (ADR-0007, amendement du
2026-08-13), plus `agent_transcript_path` et `last_assistant_message` pour `SubagentStop`.

**L'événement qui produit `waiting`**, cité mot pour mot :

> « `PermissionRequest` runs when Codex is about to ask for approval, such as a shell
> escalation or managed-network approval. It can allow the request, deny the request, or
> decline to decide and let the normal approval prompt continue. It doesn't run for commands
> that don't need approval. »

et, plus loin :

> « If no matching hook decides, Codex uses the normal approval flow. »

Un hook qui sort en 0 sans rien écrire ne décide donc rien, et l'invite d'approbation
apparaît : l'utilisateur attend. C'est la définition même de `waiting`, déclarée par
l'outil, pas devinée.

**Comment la commande est lancée** — `command_runner.rs`, `build_command` :

```rust
let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
let mut command = Command::new(shell);
command.arg("-lc");
```

La ligne de commande d'un hook est donc passée à `$SHELL -lc`. Deux conséquences directes
pour Ash, et toutes deux favorables :

- `"$ASH_TAB_ID"` est **développé par le shell**, comme chez Claude Code — la corrélation
  d'ADR-0007 fonctionne sans rien changer à l'invocation ;
- le marqueur `# ash:hook v2` en fin de ligne est **inerte** : c'est un commentaire de shell,
  exactement comme dans le `settings.json` de Claude Code.

`scrub_non_inheritable_env_vars` ne retire que quatre variables, toutes propres à OpenAI
(`CODEX_EXEC_SERVER_NOISE_AUTH_TOKEN`, `OPENAI_FEDERATION_RULE_ID`,
`OPENAI_IDENTITY_TOKEN_FILE`, `OPENAI_WORKLOAD_IDENTITY_CONTEXT`). `ASH_TAB_ID` traverse.

**Les hooks sont actifs par défaut**, et se coupent par `[features] hooks = false` dans
`config.toml`. Un administrateur peut aussi poser `allow_managed_hooks_only = true` dans
`requirements.toml`, ce qui fait **ignorer** les hooks de l'utilisateur, du projet, de la
session et des plugins — un Codex sous gestion d'entreprise rendra donc les entrées d'Ash
inertes, sans erreur.

**La revue de confiance**, et c'est la seule vraie contrainte nouvelle :

> « Before a non-managed command hook can run, Codex requires you to review and trust the
> exact hook definition. Codex records trust against the hook's current hash, so new or
> changed hooks are marked for review and skipped until trusted. »
>
> « Use `/hooks` in the CLI to inspect hook sources, review new or changed hooks, trust
> hooks, or disable individual non-managed hooks. »

Écrire le fichier ne suffit donc pas : tant que l'utilisateur n'a pas tapé `/hooks` dans
Codex et approuvé, **les hooks d'Ash sont chargés mais ignorés**. Et comme la confiance est
enregistrée contre le *hash* de la définition, toute montée de version du bloc d'Ash
redemande une revue.

### 2.3 Les trois autres mécanismes, et pourquoi aucun ne remplace les hooks

Ils ont été instruits parce qu'ils reviendront dans la conversation, et il vaut mieux qu'ils
soient écartés par écrit une fois pour toutes.

**`notify`** — vérifié dans `codex-rs/hooks/src/legacy_notify.rs` : `~/.codex/config.toml`
accepte `notify = ["programme", "arg"]`, et Codex lance ce programme en lui passant la charge
JSON **en argument de ligne de commande**, pas sur l'entrée standard. L'énumération
`UserNotification` du code source n'a **qu'une seule variante**, `agent-turn-complete`. Le
nom du fichier dit le reste : c'est le mécanisme *legacy*, absorbé par le crate des hooks.
Il ne peut produire ni `waiting`, ni `working`, ni la vie d'un sous-agent, et il demanderait
à `ash-event` un second chemin d'entrée. Écarté.

**`tui.notifications`** — un mécanisme distinct, qui connaît lui `approval-requested` en plus
d'`agent-turn-complete`, et qui l'émet dans le terminal sous forme de **séquence
d'échappement** (`osc9` ou `bel`, réglé par `tui.notification_method`). Ash tient le PTY, donc
Ash *pourrait* la lire. **C'est exactement ce qu'ADR-0007 interdit** : un état d'agent ne se
déduit jamais de ce que l'outil écrit dans son terminal, et une séquence OSC n'est pas moins
de la sortie qu'un spinner. Le fait est noté ; il n'ouvre aucune piste.

**Les modes lisibles par machine** — `codex exec --json`, `codex app-server` (JSON-RPC sur
stdio), `codex mcp`, `@openai/codex-sdk`. Tous partagent le même défaut rédhibitoire :
**ils exigent que Codex soit lancé *par* l'observateur.** Aucun ne s'attache à une session
qu'un utilisateur démarre en tapant `codex` dans un onglet d'Ash, ce qui est le seul cas qui
nous intéresse. Écartés sans regret : les hooks sont le seul mécanisme piloté par la
configuration, donc sans enrobage du processus.

**Le fichier de session** est traité au §3.

### 2.4 Ce qui reste supposé — non vérifié

Ces points n'ont **pas** pu être établis, faute d'un `codex` installé sur cette machine.
Ils sont à confirmer avant de considérer l'adaptateur comme livré :

1. **Que la version 0.144.1 porte déjà `hooks.json` et `PermissionRequest`.** Le crate
   `codex-hooks` existe depuis février 2026 et la page de documentation décrit « the current
   release » (la dernière stable publiée est `rust-v0.147.0`, le 2026-08-07), mais rien de ce
   qui a été lu ne date l'introduction de chaque événement. La documentation prévient
   elle-même : « The linked main branch schemas may include hook fields that are not in the
   current release. »
2. **Que `ash-event` satisfait la sortie attendue de `Stop` et `SubagentStop`.** La
   documentation écrit : « `Stop` expects JSON on stdout when it exits 0. Plain text output is
   invalid for this event. » `ash-event` n'écrit rien sur sa sortie standard — seulement sur
   `stderr`, en cas d'usage invalide — et la règle générale dit « Exit 0 with no output is
   treated as success and Codex continues ». Les deux phrases se concilient, mais ce n'est
   pas prouvé tant qu'on ne l'a pas vu tourner.
3. **Le comportement réel de `PermissionRequest` en mode `approval_policy = "never"`** : il
   « ne part pas pour les commandes qui n'ont pas besoin d'approbation », donc un utilisateur
   en mode permissif ne verra jamais ce `waiting`-là. Il lui restera celui de `Stop`.
4. **L'effet du `-lc`** — un shell de *login*, qui relit les fichiers de profil. Rien
   n'indique qu'un profil réécrirait `ASH_TAB_ID`, mais le coût d'une session réelle pour le
   vérifier est nul.
5. **Le format des `rollout-*.jsonl`** au-delà des cinq `type` observés. La documentation dit
   du transcript qu'il « isn't a stable interface for hooks and may change over time ».
   Ça n'a pas d'importance ici — voir §3.
6. **Que les hooks partent bien depuis la TUI interactive.** C'est le point le plus important
   de cette liste, et **aucune phrase de la documentation officielle ne le dit**. Les
   événements eux-mêmes plaident fortement pour — `UserPromptSubmit` et `PermissionRequest`
   n'ont aucun sens hors d'une session interactive, et le `notify` legacy porte un champ
   `client: "codex-tui"` qui prouve au moins que *ce* mécanisme-là part de la TUI — mais
   c'est une inférence, pas un fait vérifié. **Rien ne doit être livré avant de l'avoir vu
   tourner.**
7. **Que `PreToolUse` couvre autre chose que le shell.** Deux sources secondaires
   concordantes affirment qu'il n'intercepte **que** l'outil `Bash`, et que `apply_patch`,
   les éditions de fichiers et les outils MCP ne le déclenchent pas ; la documentation
   officielle, elle, donne un tableau de couverture qui dit le contraire (`apply_patch`,
   outils MCP et autres outils locaux y figurent en « Yes »). La contradiction n'est pas
   tranchée. Elle est **sans gravité** : `UserPromptSubmit` produit déjà `working` à chaque
   tour, donc même un `PreToolUse` réduit au shell ne laisse aucun trou.

## 3. Le verdict

> **Hook.**

Codex CLI expose un système de hooks de cycle de vie de la même famille que celui de Claude
Code : même forme de fichier, même transport de la charge utile (un objet JSON sur l'entrée
standard), même exécution par un shell, et un événement — `PermissionRequest` — qui dit
précisément ce qu'aucune observation extérieure ne peut deviner : **Codex s'apprête à
demander quelque chose à l'utilisateur**.

Les deux autres pistes sont écartées, et il vaut mieux dire pourquoi que les laisser
traîner :

- **Le fichier de session** (`~/.codex/sessions/…/rollout-*.jsonl`) semble exploitable — il
  est horodaté, structuré, et porte `task_started` / `task_complete`. Il ne l'est pas, pour
  quatre raisons dont chacune suffirait :
  1. **Il ne porte pas l'attente.** Le protocole de Codex a bien des variantes
     `exec_approval_request` et `apply_patch_approval_request`, mais un filtre
     `is_persisted_rollout_item` décide de ce qui est écrit sur le disque, et les traces
     réelles observées n'en contiennent aucune. `waiting` — le seul état qui compte — n'y est
     pas.
  2. **Ce n'est pas une interface publique.** Le dossier `docs/` du dépôt n'a aucune page sur
     ce format, et la documentation dit du transcript qu'il « isn't a stable interface for
     hooks and may change over time ».
  3. **La corrélation serait illégale.** Rattacher un fichier à un onglet demanderait de
     passer par le `cwd` ou l'horloge ; ADR-0007 nomme `ASH_TAB_ID` comme la **seule**
     corrélation admise.
  4. **Il faudrait surveiller un dossier entier**, alors que le produit tient à ne rien
     scanner sur le disque.

  Le hook rend gratuitement, et en direct, ce que ce fichier coûterait cher à approcher mal.
- **Le parsing de la sortie du PTY** n'a pas été envisagé une seule fois, et ne le sera pas :
  ADR-0007 l'écarte, et le mécanisme existe.

## 4. Ce que ça implique pour `Adapter`

**Le trait suffit en l'état. Rien ne manque, et rien du cœur n'est à toucher.**
C'est la réponse au vrai test d'ADR-0008, et elle est bonne.

Point par point, ce qu'un `CodexAdapter` fait avec le trait tel qu'il existe aujourd'hui :

| Méthode | Ce qu'elle rend pour `codex` | Le cœur suffit-il ? |
|---|---|---|
| `id()` | `"codex"` | oui |
| `instrumentation(config_dir)` | `Instrumentation { file: config_dir.join("hooks.json"), entries, version }`, avec des `HookEntry { path: vec!["hooks", "<Événement>"], item }` | **oui** — `Instrumentation.file` est déjà un chemin libre sous `config_dir`, et non un nom en dur |
| `interpret(raw)` | les quatre mots communs, comme `claude-code` : la traduction `Stop → waiting` a lieu à l'écriture du bloc | oui |
| `child_event(raw)` | `ChildEvent::Ended` sur le verbe du `SubagentStop` de Codex | oui |
| `subagents()` | `SubagentSupport::Reported` | oui |

Les quatre raisons pour lesquelles ça tombe juste, et qui méritent d'être nommées parce
qu'elles n'étaient pas acquises :

1. **La feature `hooks` écrit du JSON générique, pas du Claude Code.** Elle « sait descendre
   un chemin de clés dans du JSON, reconnaître ses propres entrées, et compter celles des
   autres » (`features/hooks/mod.rs`). Le `hooks.json` de Codex a la même profondeur et la
   même forme que le `settings.json` de Claude Code : `["hooks", "Stop"]` mène au même genre
   de tableau. Rien à généraliser.
   *Le contre-exemple qu'on redoutait était le TOML* : Codex accepte aussi des tables
   `[[hooks.Stop]]` dans `config.toml`, et **cette forme-là, Ash ne saurait pas l'écrire**.
   Elle est heureusement facultative — la documentation recommande même de n'utiliser qu'une
   représentation par couche. Ash vise `hooks.json`, et le problème ne se pose pas.
2. **La feature `hooks` crée le fichier absent.** `install.rs` porte déjà
   `created_the_file: bool` : `~/.codex/hooks.json` n'existe pas chez la plupart des
   utilisateurs, et c'est un cas normal, pas un cas particulier à ajouter.
3. **Le marqueur `# ash:hook v` survit.** Codex exécute la ligne par `$SHELL -lc` : le
   commentaire de fin de ligne est inerte, exactement comme chez Claude Code. La règle
   d'ADR-0007 — « Ash n'écrit que ce qui lui appartient, et sait le reconnaître » — s'applique
   sans un mot de plus.
4. **`agent_id` / `agent_type` arrivent déjà.** `ash-event` les lit sur son entrée standard,
   `EventFrame` les porte, `subagents.rs` les consomme. Codex les envoie sous ces noms exacts.

### Les trois frottements, et pourquoi ils ne changent rien au trait

- **Deux des hooks qu'Ash veut poser sont *décisionnels*, et c'est le vrai danger de cette
  intégration.** `PermissionRequest` peut répondre `{"decision":{"behavior":"allow"}}` et
  **approuver une commande à la place de l'utilisateur** ; `Stop` peut répondre
  `{"decision":"block","reason":…}` et **relancer l'agent** avec un nouveau prompt. Ash est un
  observateur : il doit sortir en 0 **sans rien écrire sur la sortie standard**, ce que
  `ash-event` fait déjà — il n'écrit que sur `stderr`, et seulement pour un usage invalide.
  Ce n'est pas un manque du trait, c'est une propriété à **tester** : un test de l'adaptateur
  doit affirmer que le bloc écrit ne contient aucune décision, et la vérification `qa` doit
  regarder qu'une session réelle n'approuve rien toute seule.
  [ADR-0015](./adr/0015-ash-compose-l-utilisateur-envoie.md) — « Ash ne valide rien à la
  place de l'utilisateur » — est ici littérale, pas analogique.

- **`SubagentStart` existe chez Codex, et pas chez Claude Code.** La documentation de
  `ChildEvent` explique que sa variante unique est « un constat, pas un oubli » : aucun outil
  n'annonçait la naissance d'un sous-agent, qui se lit donc au premier événement portant un
  `agent_id` inconnu. **Ce constat est désormais faux pour Codex.** Il n'oblige à rien : la
  naissance par `agent_id` inconnu continue de fonctionner, et l'adaptateur n'a qu'à ne pas
  installer `SubagentStart`. Mais la phrase du code doit être corrigée — dire « aucun outil »
  quand un outil le fait est le genre de commentaire qui égare la tâche suivante. Une
  variante `ChildEvent::Started` serait un enrichissement, pas un manque : à décider quand un
  besoin d'affichage l'exigera, pas dans ce ticket.
- **La revue de confiance de Codex (`/hooks`) n'a pas d'équivalent chez Claude Code.** Ash
  écrit le bloc, le marqueur est là, l'écran de réglages dira « Installed » — et rien ne
  remontera tant que l'utilisateur n'aura pas approuvé dans Codex. Ce n'est **pas** un manque
  du trait : c'est un fait à *dire* à l'écran, et l'écran est exactement l'endroit qui agit
  ([ADR-0010](./adr/0010-sidebar-informe-terminal-agit.md)). Ash ne doit surtout pas poser
  `--dangerously-bypass-hook-trust` à la place de l'utilisateur : ce serait valider à sa place,
  ce qu'[ADR-0015](./adr/0015-ash-compose-l-utilisateur-envoie.md) refuse dans l'esprit sinon
  dans la lettre.

## 5. Si le verdict avait été « rien »

Il ne l'est pas. Aucun amendement d'ADR-0008 ni d'ADR-0006 n'est à écrire pour dire que
`codex` reste en `generic`, et l'entrée correspondante de `KNOWN_PROVIDERS`
(`adapter: "generic"`) devient au contraire celle qu'il faut corriger.

Ce qui reste à écrire, c'est **la mise à jour de la spec §12** : la question ouverte n°1 est
répondue pour `codex`, elle ne l'est ni pour `kimi` ni pour `opencode`. Elle se reformule,
elle ne se ferme pas.

## 6. Le plan d'implémentation

Fichier par fichier, avec ce que chacun touche du cœur.

### Nouveau — `src-tauri/src/features/agents/adapters/codex.rs`

Le calque de `claude_code.rs`, avec sa propre table de traduction :

| Hook de Codex | Ce qu'il dit | État déclaré |
|---|---|---|
| `UserPromptSubmit` | l'utilisateur vient d'envoyer un prompt | `working` |
| `PreToolUse` | un outil démarre, donc après accord | `working` |
| `PermissionRequest` | Codex s'apprête à demander une approbation | `waiting` |
| `Stop` | le tour est fini, Codex rend le clavier | `waiting` |
| `SessionEnd` | la session se termine | `done` |
| `SubagentStop` | un enfant s'arrête — **verbe, pas état** | `subagent-stop` |

Les détails qui lui sont propres, et qui ne sortent pas de ce fichier :

- `file: config_dir.join("hooks.json")` ;
- `path: vec!["hooks".to_owned(), hook.to_owned()]` — la même profondeur que Claude Code ;
- un `matcher` là où Codex l'honore et où Ash veut tout voir : `PreToolUse` et
  `PermissionRequest` acceptent `"*"`. `UserPromptSubmit`, `Stop` et `SessionEnd` l'ignorent —
  l'omettre y est plus honnête que de l'écrire ;
- un `timeout` **court** sur chaque handler. Le défaut de Codex est de 600 secondes, et
  `PermissionRequest` bloque l'invite d'approbation tant que le hook n'a pas rendu la main :
  `ash-event` écrit une ligne sur un socket et sort, donc 5 secondes sont deux ordres de
  grandeur au-dessus du réel, et un socket absent ne peut plus figer l'invite de l'utilisateur ;
- `BLOCK_VERSION` propre à cet adaptateur, à 1.

Ses tests : la suite contractuelle `check_adapter_contract`, l'aller-retour
« ce que le bloc écrit, `interpret` le relit », le refus des noms de hooks bruts, la
neutralisation des métacaractères de shell dans le chemin d'`ash-event`, et **deux tests
spécifiques à Codex** : le bloc, relu comme JSON, a la forme que `hooks.json` attend ; et
aucune entrée du bloc ne porte de champ décisionnel — c'est le garde-fou d'ADR-0015 posé à
la compilation plutôt que confié à la vigilance.

### Modifié — `src-tauri/src/features/agents/adapters/mod.rs`

`mod codex;` et `pub use codex::CodexAdapter;`. Deux lignes, et c'est la définition même de
« ajouter un outil, c'est ajouter un fichier ici » que le module documente déjà.

### Modifié — `src-tauri/src/features/agents/providers.rs`

L'entrée `codex` passe de `adapter: "generic"` à `adapter: "codex"`. `installed_at` reste
vide tant qu'on n'a pas observé une installation réelle : `codex` garde son nom, donc le
deuxième signal (le nom de l'exécutable) suffit.
C'est une **table de données**, pas le cœur : aucune règle ne change.

### Modifié — `src-tauri/src/lib.rs` (composition root)

Un second `AdapterProfile`, à côté de celui de `claude-code` :

- `default_config: Some("~/.codex")` ;
- `config_env: Some("CODEX_HOME")` — la variable par laquelle Codex se voit imposer un
  dossier, l'équivalent de `CLAUDE_CONFIG_DIR` ;
- `signature: vec!["sessions"]` — le sous-dossier qu'une installation ayant déjà tourné
  possède, et **surtout pas** `hooks.json`, qui est précisément le fichier qu'Ash s'apprête à
  écrire (c'est l'erreur que le commentaire de `claude-code` a déjà nommée) ;
- `probe_args: vec!["--version"]`.

C'est le composition root : l'assembler est son travail, pas une entorse.

### Non modifiés — et c'est le résultat

`adapter.rs`, `machine.rs`, `supervisor.rs`, `wire.rs`, `socket.rs`, `subagents.rs`,
`contract.rs`, toute la feature `hooks`, tout le TypeScript. **Le critère de sortie du jalon
J4 est tenu.**

### Ce qui n'est pas dans ce ticket, et qu'il faut noter quelque part

1. **Dire la revue de confiance à l'écran.** Après installation sur `~/.codex`, l'écran de
   réglages devrait apprendre à l'utilisateur qu'il lui reste à taper `/hooks` dans Codex.
   Sans ça, la fonctionnalité paraîtra cassée à quelqu'un qui a tout fait correctement.
   Un ticket d'interface, à ouvrir.
2. **Corriger le commentaire de `ChildEvent`**, qui affirme qu'aucun outil n'annonce le
   démarrage d'un sous-agent. Une ligne, dans ce ticket ou le suivant.
3. **Vérifier sur une machine où `codex` tourne** les sept points du §2.4, dans cet ordre de
   gravité : **(a)** que les hooks partent bien depuis la TUI interactive ; **(b)** qu'Ash
   n'approuve jamais rien tout seul ; **(c)** que `Stop` accepte une sortie standard vide ;
   **(d)** que `PermissionRequest` existe dans la version installée. C'est du ressort de
   l'agent `qa`, avec un `CODEX_HOME` jetable, jamais sur le `~/.codex` de l'utilisateur —
   la même prudence que celle que `CLAUDE_CONFIG_DIR` impose déjà à Ash-dev.
4. **`kimi` et `opencode`** restent en `generic`, et la question ouverte n°1 de la spec reste
   ouverte pour eux.
