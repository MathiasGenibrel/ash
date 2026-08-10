---
description: Nettoie les worktrees de tâche d'Ash dont la pull request a été fusionnée (ou fermée) — une tâche = un worktree, supprimé une fois la PR validée. Sur ce projet, chaque worktree emporte plusieurs Go de target/ Cargo.
argument-hint: [<numéro d'issue ou ref> ...] — sans argument, passe en revue tous les worktrees de tâche
allowed-tools: Bash, Read, AskUserQuestion
---

# /worktree-clean — Ash

Tu nettoies les worktrees de tâche. Convention : **une tâche = un worktree**, dans
`.claude/worktrees/<ref>`, supprimé **après fusion de la PR**.

Cibles : `$ARGUMENTS` — une ou plusieurs références (`42`, `feat/pty-tabs`), ou rien pour
tout passer en revue.

## Marche à suivre

1. **Inventaire** — chaque worktree de tâche, sa branche, l'état de sa PR :

   ```bash
   .claude/scripts/worktree.sh list
   ```

2. **Simulation** — ce qui serait supprimé, sans rien toucher :

   ```bash
   .claude/scripts/worktree.sh clean --dry-run
   ```

3. **Présente le résultat** : ce qui sera nettoyé (`merged` / `closed`) et ce qui est
   conservé, avec la raison (PR encore ouverte, aucune PR trouvée, modifications non
   commitées).

   **Ajoute la taille récupérée.** Sur ce projet elle est significative — chaque worktree
   porte son propre `target/` Cargo, souvent plusieurs gigaoctets :

   ```bash
   du -sh .claude/worktrees/*/ 2>/dev/null
   ```

4. **Demande confirmation** (AskUserQuestion) avant toute suppression.
5. **Nettoie** — worktrees éligibles et leur branche locale :

   ```bash
   .claude/scripts/worktree.sh clean
   ```

6. **Récapitule** : supprimés, conservés, pourquoi, et l'espace disque récupéré.

## Règles

- Le script ne supprime **jamais** un worktree dont la PR est encore ouverte, ni un
  worktree contenant des **modifications non commitées** — ce garde-fou ne se contourne
  pas.
- Ne touche pas aux worktrees `agent-*` : ils appartiennent au harnais Claude Code, pas à
  la boucle de tâches.
- Ne supprime aucune branche distante : la fusion côté GitHub s'en charge.
- Un worktree supprimé emporte ses `node_modules` **et son `target/` Cargo** : c'est
  voulu, ils ne sont partagés avec personne. La contrepartie est qu'un worktree rouvert
  plus tard recompilera tout.
- Si un worktree doit disparaître malgré du travail non commité, c'est une décision
  explicite de l'utilisateur : montre-lui d'abord `git -C <worktree> status --porcelain`.
- Ne cible que `.claude/worktrees/` : les worktrees ouverts ailleurs par l'utilisateur ne
  te regardent pas.

**Un mot sur Ash lui-même** : le produit qu'on développe ici affiche et nettoie des
worktrees. Ne confonds pas les deux — cette command nettoie les worktrees *de
développement d'Ash*, pas ceux qu'Ash gérera pour ses utilisateurs.
