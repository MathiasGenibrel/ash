import { ElementBuilder, type UiChild } from "./node";

/**
 * Les deux conteneurs : une rangée, une colonne.
 *
 * Ils ne portent qu'une classe — la mise en forme est du CSS, pas du TypeScript. Ce qu'ils
 * apportent est ailleurs : ils acceptent directement d'autres constructeurs, donc une vue
 * s'écrit comme elle se lit.
 */
class Stack extends ElementBuilder {
    constructor(className: string, children: readonly UiChild[]) {
        super("div", className);
        this.add(...children);
    }

    /** Pousse ce qui suit à l'autre bout — le `flex: 1` des trois vues du dépôt. */
    spacer(): this {
        return this.add(new Stack("ui-spacer", []));
    }
}

export function row(...children: readonly UiChild[]): Stack {
    return new Stack("ui-row", children);
}

export function column(...children: readonly UiChild[]): Stack {
    return new Stack("ui-column", children);
}

export type { Stack };
