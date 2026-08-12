import type { KeyChord } from "./key-bindings";

/**
 * Test Data Builders de la feature terminal : pour l'instant, un accord de touches.
 *
 * Le fichier n'est pas un `*.test.ts`, et c'est ce qui permet de le partager : importer un
 * fichier de test depuis un autre y réenregistrerait ses `describe`, et chaque test de la
 * saisie tournerait deux fois. Il vit dans la feature, comme
 * [`shared/ipc/builders.ts`](../../shared/ipc/builders.ts) et
 * [`features/settings/builders.ts`](../settings/builders.ts), et pour la même raison : deux
 * tables de raccourcis — la saisie (`key-bindings.ts`) et les actions (`key-actions.ts`) —
 * décrivent le **même** accord. Deux fabriques divergeraient, et le jour où l'une d'elles
 * cesserait de décrire ce que l'autre décrit, la disjonction des deux tables ne serait plus
 * vérifiée que sur le papier.
 *
 * Rien ici n'est importé par le code de production : il n'entre donc pas dans le bundle.
 */

/**
 * Un accord de touches, décrit par ce qu'on presse.
 *
 * Les défauts sont ceux d'une frappe nue — un `keydown`, aucun modificateur — parce que
 * c'est le cas qui doit rester intact : tout ce qu'aucune table ne nomme repart vers
 * xterm.js, et une flèche seule appartient à l'historique de `zsh`.
 *
 * Chaque modificateur rend un nouveau builder plutôt que de muter le sien : une liste
 * d'accords se compose alors à partir d'un même `press("ArrowUp")` sans qu'un scénario
 * teigne le suivant.
 */
export class ChordBuilder {
    private constructor(private readonly chord: KeyChord) {}

    static press(key: string): ChordBuilder {
        return new ChordBuilder({
            type: "keydown",
            key,
            altKey: false,
            ctrlKey: false,
            metaKey: false,
            shiftKey: false,
        });
    }

    withOption(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, altKey: true });
    }

    withCommand(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, metaKey: true });
    }

    withControl(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, ctrlKey: true });
    }

    withShift(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, shiftKey: true });
    }

    released(): ChordBuilder {
        return new ChordBuilder({ ...this.chord, type: "keyup" });
    }

    build(): KeyChord {
        return this.chord;
    }
}

/** Le point d'entrée usuel : `press("ArrowUp").withCommand()`. */
export const press = (key: string): ChordBuilder => ChordBuilder.press(key);

/**
 * Les accélérateurs déclarés dans `src-tauri/src/menu.rs`.
 *
 * macOS les consomme dans `performKeyEquivalent:` avant que la webview ne voie un
 * `keydown`, donc aucune table de raccourcis ne doit les nommer : l'entrée serait morte
 * ici, et vivante le jour où le menu perdrait la sienne. La liste est ici plutôt que
 * recopiée dans chaque test parce qu'elle décrit le menu, pas une table — quand le menu
 * gagne un accélérateur, c'est ce seul endroit qui doit suivre.
 */
export const menuAccelerators = (): ChordBuilder[] => [
    press("n").withCommand(),
    press("n").withCommand().withShift(),
    press("w").withCommand(),
    press("k").withCommand(),
    press("b").withCommand(),
    press(",").withCommand(),
    press("c").withCommand(),
    press("v").withCommand(),
    ...["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((digit) => press(digit).withCommand()),
];
