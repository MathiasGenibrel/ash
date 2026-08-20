import { describe, expect, it } from "bun:test";

import { conflicted, MergeViewBuilder, unreadableConflict } from "@/shared/ipc/builders";
import { find, findAll, plainText } from "@/shared/ui";

import { mergeScreen, NO_SELECTION, type MergeActions, type MergeSelection } from "./screen";

/** Des gestes qui n'attendent rien : le sujet des tests est ce qui s'affiche. */
const inert: MergeActions = {
    selectFile: () => undefined,
    selectHunk: () => undefined,
    edit: () => undefined,
    take: () => undefined,
    apply: () => undefined,
    proceed: () => undefined,
    handOverRest: () => undefined,
};

const draw = (view: ReturnType<MergeViewBuilder["build"]>, selection: MergeSelection = NO_SELECTION) =>
    mergeScreen(view, selection, null, inert);

describe("l'onglet de merge", () => {
    it("Given a stopped rebase, when the three panels are drawn, then the sides carry branch names and never say ours or theirs", () => {
        // Given — le critère du ticket : « les côtés portent le nom de leur branche, pas
        // `ours`/`theirs` »
        const view = MergeViewBuilder.create().build();

        // When
        const heads = findAll(draw(view), "merge-panel-head").map((head) => plainText(head));

        // Then
        expect(heads[0]).toContain("main");
        expect(heads[2]).toContain("feat");
        expect(plainText(draw(view))).not.toContain("ours");
        expect(plainText(draw(view))).not.toContain("theirs");
    });

    it("Given the same two branches merged instead of rebased, when the panels are drawn, then the left side keeps its name and changes its role", () => {
        // Given — c'est l'inversion que la spec nomme : le `ours` de git désigne la
        // branche courante en merge et la cible en rebase. Si l'écran gardait le jargon,
        // ce test et le précédent ne pourraient pas passer ensemble.
        const rebase = findAll(draw(MergeViewBuilder.create().build()), "merge-panel-head");
        const merge = findAll(draw(MergeViewBuilder.create().merging().build()), "merge-panel-head");

        // When
        const left = { rebase: plainText(rebase[0]!), merge: plainText(merge[0]!) };

        // Then
        expect(left.rebase).toContain("main");
        expect(left.merge).toContain("main");
        expect(left.rebase).toContain("rebasing onto");
        expect(left.merge).toContain("you are on");
    });

    it("Given a conflict still to settle, when the screen is drawn, then continue stays visible, dark, and says how many are left", () => {
        // Given — « `continue` reste visible mais éteint tant qu'il reste des conflits,
        // avec le compte ». Éteint, pas masqué : le masquer ferait croire qu'il n'existe pas.
        const view = MergeViewBuilder.create()
            .withFiles(conflicted("src/probe.rs"), conflicted("src/main.ts"))
            .build();

        // When
        const proceed = find(draw(view), "merge-continue");

        // Then
        expect(proceed).not.toBeNull();
        expect(plainText(proceed!)).toBe("git rebase --continue");
        expect(proceed?.attrs["disabled"]).toBe("");
        expect(proceed?.attrs["title"]).toContain("2 conflict(s)");
        expect(plainText(find(draw(view), "merge-count")!)).toBe("2 left");
    });

    it("Given every conflict settled, when the screen is drawn, then continue lights up", () => {
        // Given
        const view = MergeViewBuilder.create().withFiles(conflicted("src/probe.rs", 0)).build();

        // When
        const proceed = find(draw(view), "merge-continue");

        // Then
        expect(proceed?.attrs["disabled"]).toBeUndefined();
    });

    it("Given more conflicts than git listed, when the screen is drawn, then the count says so and continue stays dark", () => {
        // Given — la liste des chemins est bornée à cent, le compte ne l'est pas
        const view = MergeViewBuilder.create()
            .withFiles(conflicted("src/probe.rs", 0))
            .withHidden(2_999)
            .build();

        // When
        const drawn = draw(view);

        // Then
        expect(plainText(find(drawn, "merge-count")!)).toBe("2999 left");
        expect(find(drawn, "merge-continue")?.attrs["disabled"]).toBe("");
    });

    it("Given a hunk under the eyes, when the middle panel is drawn, then it holds what was typed and nothing else", () => {
        // Given — Ash ne choisit pas de côté : le panneau central part vide, et ce qui s'y
        // trouve est ce que l'utilisateur y a mis
        const view = MergeViewBuilder.create().build();

        // When
        const empty = find(draw(view), "merge-editor");
        const typed = find(
            draw(view, { path: "src/probe.rs", hunk: 0, draft: "main and feat\n" }),
            "merge-editor",
        );

        // Then
        expect(plainText(empty!)).toBe("");
        expect(plainText(typed!)).toBe("main and feat\n");
    });

    it("Given a path git had to quote, when the file strip is drawn, then it is listed, counted, and refused", () => {
        // Given — dé-échapper un chemin de git juste avant d'écrire dedans, c'est réécrire
        // son analyseur de chemins dans un dépôt qu'on n'a pas choisi
        const view = MergeViewBuilder.create()
            .withFiles(unreadableConflict('"src/\\303\\251.rs"'))
            .build();

        // When
        const entry = findAll(draw(view), "merge-file")[0];

        // Then
        expect(entry?.attrs["disabled"]).toBe("");
        expect(entry?.attrs["title"]).toContain("resolve it in an editor");
        expect(plainText(find(draw(view), "merge-count")!)).toBe("1 left");
    });

    it("Given a stopped rebase, when the foot is drawn, then abort and skip are text and nothing offers to run them", () => {
        // Given — spec §7.4 : « `abort` et `skip` restent visibles ». Visible n'est pas
        // exécutable : `--abort` jette le travail (ADR-0015).
        const view = MergeViewBuilder.create().build();

        // When
        const escapes = find(draw(view), "merge-escapes");

        // Then
        expect(plainText(escapes!)).toContain("git rebase --abort");
        expect(plainText(escapes!)).toContain("git rebase --skip");
        expect(findAll(escapes!, "ui-button")).toHaveLength(0);
        expect(plainText(find(draw(view), "merge-rescue")!)).toContain("ORIG_HEAD 80eca44");
    });

    it("Given an operation finished elsewhere, when the tab is drawn, then it says so and stays open", () => {
        // Given — le rebase a été terminé dans un terminal, ou par un agent. Refermer
        // l'onglet sous les yeux de l'utilisateur serait un geste qu'il n'a pas fait
        // ([ADR-0010]).
        const view = MergeViewBuilder.create().finished().build();

        // When
        const drawn = draw(view);

        // Then
        expect(plainText(drawn)).toContain("Nothing is stopped in this worktree any more.");
        expect(find(drawn, "merge-continue")).toBeNull();
    });
});
