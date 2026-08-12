/**
 * De quoi faire refuser une divergence par `tsc`, plutôt que la faire remarquer par un
 * humain.
 *
 * Le contrat Rust ↔ TypeScript est écrit **deux fois** : une fois en `struct` sérialisées,
 * une fois en interfaces à la main. Les secondes portent la prose qui explique le produit —
 * pourquoi `location` peut être `null`, ce que `upstream` absent veut dire — et c'est ce
 * qui justifie qu'elles restent écrites à la main. Ce qui ne se justifie pas, c'est que
 * rien ne les rattache aux premières : un champ renommé côté Rust laissait jusqu'ici le
 * TypeScript compiler sans broncher, et rendait `undefined` à l'exécution (#16, #48).
 *
 * `ts-rs` tire de chaque `struct` un type dans `shared/ipc/generated/`. Les alias
 * ci-dessous sont ce qui **confronte** les deux : une assertion de type, sans aucune valeur
 * à l'exécution, que `bun run typecheck` évalue.
 *
 * La chaîne complète tient en deux des six vérifications obligatoires, dans cet ordre :
 * `cargo test` régénère `generated/`, puis `bun run typecheck` compare. Sauter la première
 * laisse comparer un contrat périmé — c'est la seule maille de ce filet.
 */

/**
 * Efface les `readonly` de part et d'autre avant de comparer.
 *
 * Nécessaire, et sans conséquence : `readonly` est un renforcement propre au TypeScript —
 * « la fenêtre ne modifie pas ce que le backend lui a donné » — qui ne veut rien dire sur
 * le fil. Sans cet effacement, `readonly string[]` et `string[]` se déclareraient
 * divergents alors qu'ils décrivent le même JSON, et le filet crierait au loup sur chacune
 * des quatre listes du contrat.
 */
type Writable<T> = T extends readonly (infer Element)[]
    ? Writable<Element>[]
    : T extends object
      ? { -readonly [Key in keyof T]: Writable<T[Key]> }
      : T;

/**
 * `true` quand les deux types décrivent exactement le même JSON.
 *
 * Les **deux** directions sont vérifiées, et chacune attrape une faute que l'autre laisse
 * passer : un champ que le Rust envoie et que la main a oublié se voit dans un sens, un
 * champ que la main invente et que le Rust n'envoie pas dans l'autre. Une seule direction
 * aurait laissé passer #16 — un champ ajouté côté Rust, absent côté fenêtre.
 *
 * Les `[…]` autour des deux membres empêchent la distribution sur les unions : sans eux,
 * `GitHead` serait comparé variante par variante, et une variante perdue passerait.
 */
export type Mirrors<Rust, HandWritten> = [Writable<Rust>] extends [Writable<HandWritten>]
    ? [Writable<HandWritten>] extends [Writable<Rust>]
        ? true
        : "the hand-written type says something the Rust type never sends"
    : "the hand-written type does not accept what the Rust type sends";

/**
 * `true` quand ce que la fenêtre **envoie** est acceptable pour le type Rust qui le reçoit.
 *
 * Une direction seulement, et c'est voulu : un formulaire a le droit d'être plus strict que
 * ce que le backend tolère — `ToolDraft` n'envoie que du texte là où `NewTool` accepte
 * aussi l'absence. Exiger l'égalité obligerait la fenêtre à porter un `| null` qu'elle ne
 * produit jamais.
 */
export type Accepts<Rust, Sent> = [Writable<Sent>] extends [Writable<Rust>]
    ? true
    : "the window sends something the Rust type would refuse to deserialize";

/**
 * Le point où la comparaison devient une erreur de compilation.
 *
 * `Assert<Mirrors<…>>` ne se réduit à rien quand les deux types s'accordent, et refuse la
 * contrainte sinon — en affichant la phrase que [`Mirrors`] a choisie, qui dit **dans quel
 * sens** ça diverge.
 */
export type Assert<Verdict extends true> = Verdict;
