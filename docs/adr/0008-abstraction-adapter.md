# ADR-0008 — Un trait `Adapter` dès le premier jalon

- **Statut** : accepté (2026-08-07)
- **Complète** : [ADR-0007](./0007-etats-par-hooks.md)

## Contexte

Ash est d'abord un outil personnel. La tentation serait de câbler Claude Code en dur
et de généraliser plus tard. Mais l'exigence a été posée explicitement : *« il faut
que l'on puisse intégrer d'autres providers facilement »*, et l'intention du produit
est bien de superviser `claude`, `codex`, `kimi`, `opencode` indifféremment.

Or c'est précisément l'endroit où les outils divergent le plus : mécanisme
d'instrumentation, vocabulaire d'états, notion de subagent, emplacement de la
configuration. Généraliser après coup coûterait une réécriture du cœur.

## Décision

L'intégration d'un outil passe par un **trait `Adapter`**, présent dès le jalon J1,
avec `claude-code` comme première implémentation — traitée comme *une* implémentation
et non comme le cas normal.

```rust
trait Adapter {
    fn id(&self) -> &str;

    /// Ce que l'adaptateur doit écrire dans la configuration de l'outil.
    /// None si l'outil n'expose aucun point d'instrumentation.
    fn instrumentation(&self, config_dir: &Path) -> Option<Instrumentation>;

    /// Traduit un événement brut en état Ash.
    fn interpret(&self, raw: RawEvent) -> Option<StateChange>;

    /// L'outil expose-t-il des sous-tâches ?
    fn subagents(&self) -> SubagentSupport;
}
```

Le cœur d'Ash ne connaît que le vocabulaire commun :
`idle · working · waiting · done · error`, et une hiérarchie agent → subagents.

Un adaptateur `generic` sert de socle : ni instrumentation ni subagents, états
déduits de la seule sonde (`idle` / `done` / `error`). Tout outil inconnu tombe
dessus et reste utilisable.

## Conséquences

- Ajouter un outil = écrire un adaptateur, sans toucher au cœur ni à l'UI.
- Le critère de sortie du jalon J4 devient vérifiable : un deuxième outil supporté
  sans modification du cœur.
- Les particularités d'un outil ne peuvent pas fuir dans la sidebar ni dans le moteur
  d'états — le vocabulaire commun est la frontière.
- Coût : une indirection dès J1 alors qu'un seul outil est réellement supporté.
  Assumé, c'est le but.
- Le vocabulaire commun est un pari. Si un outil expose un état qui ne s'y réduit
  pas (par exemple « en attente d'approbation d'un outil » distinct de « attend une
  réponse »), il faudra enrichir l'énumération — un changement central, mais borné.
- `SubagentSupport` isole le fait que la notion de subagent n'existe pas partout.

## Alternatives écartées

- **Câbler Claude Code en dur, généraliser plus tard** : plus rapide à J2, mais
  contredit l'exigence explicite et coûterait une refonte du cœur au moment où
  d'autres outils arriveront.
- **Adaptateurs en scripts externes** (un exécutable par outil, contrat sur stdin/
  stdout) : maximalement ouvert et scriptable en bash, mais ajoute une frontière de
  processus et un contrat à versionner pour un besoin encore théorique. À reconsidérer
  si des tiers veulent contribuer des adaptateurs.
