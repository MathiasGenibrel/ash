import { readFileSync } from "node:fs";

import { describe, expect, it } from "bun:test";

import { AccountUsageBuilder, noAccountUsage } from "@/shared/ipc/builders";
import type { SessionUsage } from "@/shared/ipc";
import { findAll, plainText } from "@/shared/ui";

import {
    composeContextGauge,
    composeQuotas,
    composeUsagePopover,
    inStatusBar,
    remainingUntil,
    type QuotaSegment,
} from "./usage";

/**
 * L'usage de la ligne de statut : ce qui se lit, et ce qui ne se lit **pas**.
 *
 * La moitié des règles de cette tranche sont des absences — un outil muet ne coûte rien à
 * l'affichage, un quota qu'on n'a pas disparaît sans dire pourquoi (ADR-0016, condition 3).
 * Ce sont elles qui sont vérifiées ici en premier : une valeur inventée à leur place ne se
 * verrait qu'à l'écran, et seulement chez quelqu'un dont le jeton est absent.
 *
 * Les couleurs, elles, se vérifient dans la feuille — comme la confirmation de fermeture le
 * fait déjà : un palier qui écrirait un hexadécimal en dur sortirait des trois thèmes sans
 * qu'aucun test de modèle ne bronche.
 */

/** Le décor de la maquette : `2h14` de session, et un weekly à trois jours. */
const NOW = 1_787_241_600_000;

function quotas(usage = new AccountUsageBuilder().build()): readonly QuotaSegment[] {
    return composeQuotas(usage, NOW);
}

/**
 * Une mesure telle que le backend l'envoie.
 *
 * Le modèle a un défaut **absent** et non un nom plausible : les scénarios de pourcentage ne
 * parlent pas de lui, et leur en donner un ferait passer sous silence les cas où il manque.
 */
function measured(
    usedTokens: number,
    windowTokens: number | null,
    model: string | null = null,
): SessionUsage {
    return { usedTokens, windowTokens, model };
}

/** La feuille de la feature, lue comme un texte : c'est tout ce que `bun test` peut en faire. */
const SHEET = readFileSync(new URL("./terminal.css", import.meta.url), "utf8");

describe("la jauge de contexte", () => {
    it("Given a tab whose tool says nothing about its context, when the gauge is composed, then there is nothing to draw", () => {
        // Given / When — un shell à son invite, un `vim`, un outil sans transcript : trois
        // absences que rien ne doit distinguer. Ni jauge à zéro, ni `ctx —`.
        const gauge = composeContextGauge(null);

        // Then
        expect(gauge).toBeNull();
    });

    it("Given a conversation below the first threshold, when the gauge is composed, then it reads fresh", () => {
        // Given — 138 k sur 200 k, soit 69 % : un cran sous le palier
        const gauge = composeContextGauge(measured(138_000, 200_000));

        // Then
        expect(gauge?.label).toBe("ctx 69%");
        expect(gauge?.share?.level).toBe("fresh");
    });

    it("Given a conversation that reaches seventy percent, when the gauge is composed, then it turns loaded", () => {
        // Given — le seuil est atteint, pas dépassé : c'est le cas qui décide de `>=`
        const gauge = composeContextGauge(measured(140_000, 200_000));

        // Then
        expect(gauge?.label).toBe("ctx 70%");
        expect(gauge?.share?.level).toBe("loaded");
    });

    it("Given a conversation that reaches ninety percent, when the gauge is composed, then it announces the compaction", () => {
        // Given — la maquette est formelle : corail, et aucune alerte modale. Un contexte
        // plein n'est pas une panne, et ce n'est pas un état d'agent (ADR-0007).
        const gauge = composeContextGauge(measured(180_000, 200_000));

        // Then
        expect(gauge?.share?.level).toBe("compacting");
    });

    it("Given a rounded percentage that crosses a threshold, when the gauge is composed, then the colour follows the number that is shown", () => {
        // Given — 139 400 / 200 000 fait 69,7 %, qui s'écrit `70%`. Une jauge qui resterait
        // bleue en affichant `ctx 70%` se lirait comme un bug : c'est le chiffre qui promet.
        const gauge = composeContextGauge(measured(139_400, 200_000));

        // Then
        expect(gauge?.label).toBe("ctx 70%");
        expect(gauge?.share?.level).toBe("loaded");
    });

    it("Given a conversation past its window, when the gauge is composed, then it stops at a hundred", () => {
        // Given — une conversation compactée peut déclarer plus que sa fenêtre le temps d'un
        // tour, et `ctx 143%` n'aurait aucun sens sur un cadran qui promet une part.
        const gauge = composeContextGauge(measured(286_000, 200_000));

        // Then
        expect(gauge?.share?.percent).toBe(100);
        expect(gauge?.label).toBe("ctx 100%");
    });

    it("Given a million token window, when the gauge is composed, then it reads what /context reads in that session", () => {
        // Given — le scénario du ticket, et la raison de toute la tranche : 57 200 tokens
        // dans une session `opus[1m]`. Le dénominateur supposé de 200 000 en faisait `ctx
        // 29%`, à un cran du premier seuil de couleur, sur une conversation à peine entamée.
        const gauge = composeContextGauge(measured(57_200, 1_000_000));

        // Then — les 6 % que `/context` affiche, et la teinte d'une conversation fraîche.
        expect(gauge?.label).toBe("ctx 6%");
        expect(gauge?.share?.level).toBe("fresh");
    });

    it("Given a window Ash could not name, when the gauge is composed, then the measure is shown without being put into a ratio", () => {
        // Given — aucune source ne nomme de modèle reconnu : ni `ANTHROPIC_MODEL`, ni le
        // `.claude/` du dépôt, ni celui du foyer. Le numérateur, lui, est exact — et
        // l'effacer avec le dénominateur serait perdre ce qu'Ash sait vraiment.
        const gauge = composeContextGauge(measured(57_200, null));

        // Then — un compte, pas une part : ni pourcentage, ni palier, donc ni barre ni
        // couleur de seuil quand la ligne le peindra.
        expect(gauge?.label).toBe("ctx 57k");
        expect(gauge?.share).toBeNull();
    });

    it("Given a small conversation whose window is unknown, when the gauge is composed, then the count is written as it is", () => {
        // Given — sous le millier, `0k` effacerait la seule chose qu'Ash mesure. La règle est
        // plate exprès : les milliers arrondis au-dessus de mille, le nombre tel quel dessous.
        const gauge = composeContextGauge(measured(900, null));

        // Then
        expect(gauge?.label).toBe("ctx 900");
    });

    it("Given a window announced as empty, when the gauge is composed, then the count survives the division by zero", () => {
        // Given — une fenêtre à zéro n'est pas une donnée, et se lit donc comme une fenêtre
        // inconnue : zéro ne se divise pas, mais 12 000 tokens ont bien été mesurés.
        const gauge = composeContextGauge(measured(12_000, 0));

        // Then
        expect(gauge?.label).toBe("ctx 12k");
        expect(gauge?.share).toBeNull();
    });
});

describe("le modèle qui tourne", () => {
    it("Given a session whose backend named its model, when the gauge is composed, then the name travels beside the label", () => {
        // Given — le scénario du ticket : 57 200 tokens dans une session `opus[1m]`, dont le
        // transcript nomme `claude-opus-5`. Le nom est **déjà** en mots quand il arrive : la
        // table qui traduit `claude-opus-5` vit dans l'adaptateur, à côté de celle des
        // fenêtres, et n'a aucune copie ici.
        const gauge = composeContextGauge(measured(57_200, 1_000_000, "Opus 5 1M"));

        // Then — le nom se lit à droite du libellé, et le libellé n'a pas changé pour autant.
        expect(gauge?.label).toBe("ctx 6%");
        expect(gauge?.model).toBe("Opus 5 1M");
    });

    it("Given a model the backend could not name, when the gauge is composed, then the segment disappears without touching the gauge", () => {
        // Given — un identifiant que l'adaptateur ne reconnaît pas, sur une fenêtre
        // parfaitement connue. Les deux absences sont indépendantes : celle du nom ne doit pas
        // emporter le pourcentage, et un tiret à sa place occuperait la barre pour dire qu'on
        // ne sait pas.
        const gauge = composeContextGauge(measured(140_000, 200_000, null));

        // Then
        expect(gauge?.model).toBeNull();
        expect(gauge?.label).toBe("ctx 70%");
        expect(gauge?.share?.level).toBe("loaded");
    });

    it("Given a window Ash could not name but a model it could, when the gauge is composed, then the name survives the missing ratio", () => {
        // Given — l'absence inverse, et elle existe vraiment : le transcript nomme ce qui a
        // tourné, et aucune configuration ne dit dans quelle fenêtre. Effacer le nom avec le
        // dénominateur serait perdre ce qu'Ash sait.
        const gauge = composeContextGauge(measured(57_200, null, "Opus 5"));

        // Then
        expect(gauge?.label).toBe("ctx 57k");
        expect(gauge?.share).toBeNull();
        expect(gauge?.model).toBe("Opus 5");
    });
});

describe("les deux quotas du compte", () => {
    it("Given an account Ash knows nothing about, when the quotas are composed, then no segment is written at all", () => {
        // Given — jeton absent, refusé, hôte injoignable, ou appels coupés : quatre raisons,
        // un seul rendu (ADR-0016, condition 3). L'écran ne signale rien non plus — il ne
        // sait pas laquelle s'applique.
        const nothing = composeQuotas(noAccountUsage(), NOW);

        // Then
        expect(nothing).toEqual([]);
    });

    it("Given both quotas, when they are composed, then each reads as its letter, its share and its countdown", () => {
        // Given / When — la position de la maquette : `s 63%` et `w 28%`
        const [session, weekly] = quotas();

        // Then
        expect(session?.letter).toBe("s");
        expect(session?.percent).toBe("63%");
        expect(session?.resets).toBe("2h14");
        expect(weekly?.letter).toBe("w");
        expect(weekly?.resets).toBe("2d 17h");
    });

    it("Given a quota without a reset date, when it is composed, then its share is still written", () => {
        // Given — un compte sans fenêtre de limitation, ou une fenêtre qui n'a pas commencé.
        // N'avoir qu'une des deux moitiés vaut mieux que n'en avoir aucune.
        const [session] = quotas(new AccountUsageBuilder().withSession(41, null).build());

        // Then
        expect(session?.percent).toBe("41%");
        expect(session?.resets).toBeNull();
    });

    it("Given a reset date that has already passed, when the countdown is derived, then it stays silent instead of announcing zero", () => {
        // Given — le fil de fond n'a pas encore rappelé depuis la fin de la fenêtre. Écrire
        // `0m` annoncerait une remise à zéro qu'Ash n'a pas constatée.
        const left = remainingUntil(NOW - 60_000, NOW);

        // Then
        expect(left).toBeNull();
    });

    it("Given a fractional percentage from the host, when it is composed, then it is rounded to what the line can hold", () => {
        // Given — l'hôte rend parfois `62,7`
        const [session] = quotas(new AccountUsageBuilder().withSession(62.7).build());

        // Then
        expect(session?.percent).toBe("63%");
    });
});

describe("ce que la barre montre, et ce que le popover montre", () => {
    it("Given both quotas, when the status bar is filled, then only the session one is on it", () => {
        // Given / When — le défaut de la maquette (vue 5c) : le weekly est masqué dans la
        // barre. Les six interrupteurs qui le rallumeraient ne sont dans aucun ticket.
        const bar = inStatusBar(quotas());

        // Then
        expect(bar.map((quota) => quota.kind)).toEqual(["session"]);
    });

    it("Given a weekly quota hidden from the bar, when the popover is composed, then it shows up there with its own countdown", () => {
        // Given / When — c'est précisément ce que le clic sur une pastille sert à révéler :
        // sans le popover, un quota qu'Ash connaît n'aurait aucun endroit où se lire.
        const popover = composeUsagePopover(quotas()).build();

        // Then
        expect(findAll(popover, "status-usage-name").map(plainText)).toEqual([
            "session",
            "weekly",
        ]);
        expect(findAll(popover, "status-usage-resets").map(plainText)).toEqual([
            "resets in 2h14",
            "resets in 2d 17h",
        ]);
    });

    it("Given an open popover, when its foot is read, then it names the window and the shortcut it hints at", () => {
        // Given / When
        const popover = composeUsagePopover(quotas()).build();

        // Then — `5h window` est un **libellé**, pas une donnée : l'API ne rend aucune durée
        // de fenêtre. `⌘⌥U` est un indice : la vue d'usage complète n'existe pas, et aucune
        // liaison n'est réclamée pour cette combinaison.
        const foot = findAll(popover, "status-usage-foot").map(plainText);
        expect(foot).toEqual(["5h window⌘⌥U"]);
    });

    it("Given two quotas, when the popover is composed, then each bar measures its quota and none measures the elapsed window", () => {
        // Given / When — une barre de fenêtre serait inventée de bout en bout : `Quota` ne
        // porte que `percent` et `resetsAt`.
        const popover = composeUsagePopover(quotas()).build();

        // Then
        const widths = findAll(popover, "status-usage-fill").map((fill) => fill.attrs["style"]);
        expect(widths).toEqual(["width: 63%", "width: 28%"]);
    });
});

describe("la mise en forme des segments d'usage", () => {
    it("Given the three context levels, when their colours are read, then each names a token instead of a hexadecimal", () => {
        // Given — les quatre couleurs de la maquette sont déjà des tokens d'Ash. Un
        // hexadécimal écrit ici sortirait des trois thèmes sans qu'aucun modèle ne bronche.
        const rules = [...SHEET.matchAll(/^\.[^\n{]*status-usage[^\n{]*\{([^}]*)\}/gm)];

        // When
        const painted = rules.map((rule) => rule[1] ?? "").join("\n");

        // Then
        expect(rules.length).toBeGreaterThan(0);
        expect(painted).not.toMatch(/#[0-9a-f]{3,8}\b/i);
    });

    it("Given a line too narrow to hold everything, when the segments withdraw, then the model goes before the quotas, the quotas before the gauge, and what says where we are never goes", () => {
        // Given — l'ordre de retrait est un critère : le modèle d'abord, les quotas ensuite,
        // la jauge et son libellé en dernier, et jamais le `cwd` ni l'état de l'agent.
        const blocks = [
            ...SHEET.matchAll(/@container statusline \(max-width: (\d+)px\) \{\n([\s\S]*?)\n\}/g),
        ].map((match) => ({ width: Number(match[1]), body: match[2] ?? "" }));

        // When — les seuils sont cherchés par ce qu'ils **retirent**, jamais par leur rang
        // dans la feuille : trois `@container` disjoints se lisent pareil dans n'importe quel
        // ordre, et un test qui dépendrait de leur rang tomberait sur un déplacement qui ne
        // change rien à l'écran.
        const modelGoes = blocks.find((block) => block.body.includes(".status-usage-model"));
        const quotasGo = blocks.find((block) => block.body.includes(".status-usage-quota"));
        const groupGoes = blocks.find(
            (block) =>
                !block.body.includes(".status-usage-model") &&
                !block.body.includes(".status-usage-quota"),
        );

        // Then — le seuil qui emporte tout le groupe est le plus étroit : on n'y arrive
        // qu'après avoir retiré le modèle, puis les pastilles.
        expect(groupGoes?.body).toContain(".status-usage");
        expect(modelGoes?.width).toBeGreaterThan(quotasGo?.width ?? 0);
        expect(quotasGo?.width).toBeGreaterThan(groupGoes?.width ?? 0);
        // Rien de ce qui dit **où l'on est** n'est visé par un retrait.
        const survivors = ["status-path", "status-text", "ash-glyph", "status-main"];
        for (const block of blocks) {
            for (const survivor of survivors) expect(block.body).not.toContain(survivor);
        }
    });
});
