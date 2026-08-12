import { ElementBuilder, text, type UiChild } from "./node";

/**
 * Ce qu'une vue dit quand elle a autre chose à montrer que des données : une bannière, et
 * un état vide.
 */

/**
 * La teinte d'une bannière, nommée par ce qu'elle **dit** — comme `StatusTone` dans la
 * ligne de statut : le même modèle se peint dans les deux palettes.
 */
export type UiTone = "info" | "warning" | "error";

class BannerBuilder extends ElementBuilder {
    constructor(message: string, tone: UiTone) {
        super("div", "ui-banner", `is-${tone}`);
        // `status` et pas `alert` : une bannière rend compte de ce qui vient d'arriver, elle
        // n'interrompt pas. `alert` couperait la parole à un lecteur d'écran à chaque rendu.
        this.attr("role", "status").add(text(message));
    }

    /** Ce qu'on peut faire de la bannière — `undo the reset`, `see the diff`. */
    action(...children: readonly UiChild[]): this {
        return this.add(...children);
    }
}

export function banner(message: string, tone: UiTone = "info"): BannerBuilder {
    return new BannerBuilder(message, tone);
}

class EmptyStateBuilder extends ElementBuilder {
    constructor(title: string) {
        super("div", "ui-empty");
        this.add(paragraph("ui-empty-title", title));
    }

    /**
     * Ce que le vide **coûte**.
     *
     * L'état vide du dépôt ne se contente pas de constater : « ash montre déjà vos onglets,
     * mais il ne sait pas lesquels sont des agents ». Le titre seul serait un cul-de-sac.
     */
    prose(sentence: string): this {
        return this.add(paragraph("ui-empty-prose", sentence));
    }
}

export function emptyState(title: string): EmptyStateBuilder {
    return new EmptyStateBuilder(title);
}

class Paragraph extends ElementBuilder {
    constructor(className: string, content: string) {
        super("p", className);
        this.add(text(content));
    }
}

function paragraph(className: string, content: string): Paragraph {
    return new Paragraph(className, content);
}

export type { BannerBuilder, EmptyStateBuilder };
