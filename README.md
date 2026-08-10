# Ash

Un terminal macOS qui montre ce que font tes agents.

Ash n'est pas un client d'IA et ne remplace pas ton shell : c'est une coquille
autour de lui. Tu lances `claude`, `codex`, `kimi` comme d'habitude, dans un vrai
bash. Ash s'occupe du reste — regrouper tes onglets par dépôt et par worktree, te dire
en permanence qui travaille, **qui attend une réponse**, et qui a fini, et te donner
le git qui va avec : quel agent a écrit quel commit, et qui travaille dans le worktree
que tu t'apprêtes à bousculer.

## Documentation

- [Spécification](./docs/spec.md) — le produit, le modèle, l'interface, les jalons
- [Décisions d'architecture](./docs/adr/) — ce qui a été tranché, et ce que ça coûte
- [Briefs de design](./docs/design/) — ce qui a été demandé au design

## État

Cadrage terminé (2026-08-07), direction visuelle livrée et revue (2026-08-10), rien
n'est implémenté.

La revue du design a ajouté un domaine entier — git — et donc cinq ADR (0011 à 0015)
et un jalon J5. Trois ADR ont été amendées, une reformulée. La spec est à jour.

Prochaine étape : **jalon J1** — PTY, onglets, raccourcis, sidebar par dépôt et
worktree. Aucun état d'agent. Objectif : qu'Ash remplace le terminal quotidien avant
qu'on investisse dans les hooks.

Risque à lever en premier : la performance de xterm.js sous WKWebView sur une sortie
verbeuse.

## Point ouvert sur le design

Les écrans **3g à 3n** (conflit de hooks et diff, ajout et état vide, les 5 états de
hooks, raccourcis, apparence, notifications) n'ont pas pu être lus — le fichier
dépasse la limite de lecture de l'API. Les écrans **1x et 2b**, référencés par le
document, ne sont dans aucun fichier du projet. La §9 de la spec pourra bouger à la
marge une fois ces écrans relus.
