import { ElementBuilder } from "@/shared/ui";

/**
 * La balise quelconque, pour les vues de cette feature.
 *
 * `shared/ui/` ne fournit que des primitives qui **portent une règle** — un bouton qu'on ne
 * peut pas éteindre en silence, un champ qui ne lit pas le DOM. Le rendu d'un document
 * markdown, lui, a besoin de `h2`, `td`, `blockquote` : des balises, sans règle. Elles
 * n'ont rien à faire dans le socle partagé (« un composant n'y monte que s'il sert au moins
 * deux features et ne porte la règle d'aucune » — la seconde condition est remplie, pas la
 * première), et elles ne méritent pas non plus une primitive chacune.
 */
class Tag extends ElementBuilder {
    constructor(name: string, ...classes: readonly string[]) {
        super(name, ...classes);
    }
}

export function tag(name: string, ...classes: readonly string[]): Tag {
    return new Tag(name, ...classes);
}

export type { Tag };
