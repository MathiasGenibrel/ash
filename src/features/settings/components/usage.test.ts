import { describe, expect, it } from "bun:test";

import { findAll, plainText, type UiChild, type UiElementNode } from "@/shared/ui";

import { usageReport } from "../builders";
import { usageSection, type UsageActions } from "./usage";

function said(children: readonly UiChild[]): string {
    return children.map(plainText).join("");
}

/** Les boutons de la section — l'interrupteur, et rien d'autre. */
function buttons(children: readonly UiChild[]): UiElementNode[] {
    return children.flatMap((child) => findAll(child, "ui-button"));
}

const IDLE: UsageActions = { setPolling: () => undefined };

describe("la section usage de la fenêtre de réglages", () => {
    it("Given calls that ash is making, when the section is composed, then it names the host before offering to cut them", () => {
        // Given — la condition 3 d'ADR-0016 a deux moitiés, et la première est « savoir
        // qu'Ash appelle ». Un interrupteur sans destination nommée demanderait à
        // l'utilisateur de couper quelque chose dont il ignore ce que c'est
        const calling = usageReport().build();

        // When
        const composed = said(usageSection(calling, IDLE));

        // Then — l'adresse vient du backend, qui la tire de la constante que le code appelle,
        // et elle se lit **avant** la ligne de l'interrupteur
        expect(composed).toContain("https://api.anthropic.com/api/oauth/usage");
        expect(composed.indexOf("api.anthropic.com")).toBeLessThan(
            composed.indexOf("ask the host how much is left"),
        );
    });

    it("Given a switch clicked in the screen, when the click is played, then it asks the backend to cut the calls and changes nothing itself", () => {
        // Given — la fenêtre demande, elle n'applique pas (ADR-0009). Le portillon qui décide
        // d'un appel est en Rust, et un interrupteur qui se contenterait de cacher le chiffre
        // laisserait le paquet partir — c'est le paquet que l'ADR donne à couper
        const asked: boolean[] = [];
        const shown = usageReport().build();
        const composed = usageSection(shown, {
            setPolling: (enabled) => asked.push(enabled),
        });

        // When
        const toggle = buttons(composed).find(
            (one) => one.attrs["aria-label"] === "usage quota calls",
        );
        toggle?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(asked).toEqual([false]);
        expect(shown.polling).toBe(true);
    });

    it("Given calls the user has cut, when the section is composed, then the switch shows his choice and not the default", () => {
        // Given — la seule chose qu'un écran de réglages doive à son utilisateur. Un
        // interrupteur redessiné à son défaut ferait croire à un réglage perdu, et le ferait
        // rejouer — donc rallumer les appels qu'il vient de couper
        const cut = usageReport().withCallsCut().build();

        // When
        const positions = buttons(usageSection(cut, IDLE)).map((one) => [
            one.attrs["aria-label"],
            one.attrs["aria-pressed"],
        ]);

        // Then
        expect(positions).toEqual([["usage quota calls", "false"]]);
    });

    it("Given a keychain that refused the token, when the section is composed, then it says which of the silences this one is", () => {
        // Given — les conséquences d'ADR-0017 : un refus, un item absent et une panne donnent
        // tous le même écran vide côté barre d'état. Sans cette ligne, l'utilisateur n'a
        // aucun moyen de savoir s'il doit se connecter, autoriser, ou attendre un correctif
        const refused = usageReport()
            .withToken("refused", "the keychain did not give up claude code's token")
            .build();
        const absent = usageReport()
            .withToken("absent", "no claude code token in the keychain")
            .build();

        // When
        const onRefusal = said(usageSection(refused, IDLE));
        const onAbsence = said(usageSection(absent, IDLE));

        // Then — et le chemin du trousseau est là dans les deux cas, comme le chemin des
        // Réglages Système l'est dans la section `notifications`
        expect(onRefusal).toContain("did not give up");
        expect(onAbsence).toContain("no claude code token");
        expect(onRefusal).toContain("Keychain Access ▸ login ▸ Claude Code-credentials");
    });

    it("Given two claude accounts, when the section is composed, then it admits it cannot tell which one the quotas belong to", () => {
        // Given — ADR-0017 dit de documenter cette limite plutôt que de la résoudre :
        // « afficher un quota en le rattachant au mauvais compte serait pire que de ne rien
        // rattacher du tout ». ADR-0007 prévoit deux dossiers de configuration, donc deux
        // comptes ; le trousseau, lui, ne porte qu'un jeton
        const shown = usageReport().build();

        // When
        const composed = said(usageSection(shown, IDLE));

        // Then
        expect(composed).toContain("no way to tell which");
    });

    it("Given the backend has not answered yet, when the section is composed, then it says what it is waiting for rather than showing a switch it does not hold", () => {
        // Given — la navigation traverse la section (`⌥↓`) : un panneau vide se lirait comme
        // une panne. Et un interrupteur dessiné à son défaut avant la réponse ferait lire à
        // qui a coupé les appels un réglage qui n'est pas le sien
        const composed = usageSection(null, IDLE);

        // Then
        expect(said(composed)).toContain("reading what ash knows");
        expect(buttons(composed)).toEqual([]);
    });
});
