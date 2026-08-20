//! **Ce qu'Ash s'autorise à écrire dans la fiche.**
//!
//! Le jumeau de `hooks::document`, et la même méthode : la garantie vit **dans les types**,
//! pas dans la prudence des appelants. Elle n'a que trois formes, et il n'y en aura pas de
//! quatrième :
//!
//! | Forme | Ce qu'elle peut perdre |
//! |---|---|
//! | [`CardDocument::with_log`] — le corps du bloc remplacé | **rien hors du bloc** : les octets d'avant l'ouverture et d'après la fermeture sont recopiés tels quels |
//! | [`CardDocument::with_block`] — le bloc ajouté en fin de fichier | **rien du tout** : c'est une addition, aucun octet existant n'est touché |
//! | [`CardDocument::fresh`] — la fiche entière | **rien du tout** : elle ne se compose que pour un fichier qui n'existe pas |
//!
//! Ce que la forme du fichier change par rapport aux `settings.json`, et que
//! `CLAUDE.md` nomme : là-bas, ce sont des **entrées marquées** qui cohabitent avec celles
//! de l'utilisateur au milieu d'un objet JSON ; ici, c'est un **bloc délimité**, parce qu'il
//! n'y a rien à entrelacer dans un `.md` — la zone d'Ash est un paragraphe, et le reste du
//! document appartient à l'utilisateur et aux agents
//! ([ADR-0013](../../../../docs/adr/0013-fiche-de-branche-dans-le-depot.md)).
//!
//! Les constructeurs sont `pub(super)`, exactement comme ceux de `hooks::document`, et pour
//! la même raison : [`CardDocument`] traverse la frontière de la feature — le port
//! [`CardFiles`](super::CardFiles) l'a dans sa signature — mais hors de `features::card`, il
//! ne se **compose** pas. C'est le compilateur qui ferme la porte à un `write(path, texte
//! quelconque)`, pas un test.

use std::ops::Range;

/// Le seul texte qu'Ash sache écrire dans une fiche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDocument(String);

impl CardDocument {
    /// Le fichier d'origine, dont **seul** l'intérieur du bloc a changé.
    ///
    /// La plage vient de [`block::locate`](super::block::locate), qui l'a lue dans le
    /// fichier ; une plage qui ne tomberait pas sur une frontière de caractère rendrait le
    /// fichier **inchangé** plutôt qu'un découpage au jugé. C'est le même parti que
    /// `hooks::document` : dans le fichier de quelqu'un d'autre, ne rien faire est toujours
    /// préférable à faire à peu près.
    pub(super) fn with_log(original: &str, inner: Range<usize>, body: &str) -> Self {
        let (Some(before), Some(after)) = (original.get(..inner.start), original.get(inner.end..))
        else {
            return Self(original.to_owned());
        };
        Self(format!("{before}{body}{after}"))
    }

    /// Le fichier d'origine, **suivi** du bloc qu'il n'avait pas.
    ///
    /// C'est le seul geste de la feature qui écrit hors d'un bloc existant, et il fallait le
    /// peser : la lettre d'ADR-0013 est « Ash n'écrit que dans une seule zone ». Mais une
    /// fiche écrite à la main — le cas normal, puisque ce sont l'utilisateur et les agents
    /// qui la rédigent — n'a pas de zone, et refuser ici reviendrait à ne jamais journaliser
    /// que les fiches qu'Ash a créées lui-même.
    ///
    /// Ce qui rend le geste acceptable est qu'il **ne peut rien perdre** : il n'écrase aucun
    /// octet, il ajoute à la fin. Comme toute écriture de la feature, il est précédé d'une
    /// sauvegarde et montré en diff avant d'être proposé.
    pub(super) fn with_block(original: &str, block: &str) -> Self {
        let separator = if original.is_empty() || original.ends_with("\n\n") {
            ""
        } else if original.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        Self(format!("{original}{separator}{block}"))
    }

    /// La fiche entière — **et seulement pour un fichier qui n'existe pas**.
    ///
    /// L'appelant le prouve ([`write`](super::write)) : il n'y a alors rien de l'utilisateur
    /// à préserver, et rien à sauvegarder. C'est le même cas que la première pose d'un
    /// `settings.json` par `hooks::install`.
    pub(super) fn fresh(text: String) -> Self {
        Self(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_card_whose_body_the_user_wrote_when_the_log_is_replaced_then_nothing_outside_the_block_moves(
    ) {
        // Given — la moitié de la garantie d'ADR-0013 : « tout le reste du fichier
        // appartient à l'utilisateur et aux agents ».
        let original = "# pourquoi\n\ndu texte\n\n<!-- ash:log -->\nvieux\n<!-- /ash:log -->\n\n## hors périmètre\n";
        let inner = original.find("vieux").unwrap_or_default()
            ..original.find("<!-- /ash:log -->").unwrap_or_default();

        // When
        let written = CardDocument::with_log(original, inner, "neuf\n");

        // Then
        assert_eq!(
            written.as_str(),
            "# pourquoi\n\ndu texte\n\n<!-- ash:log -->\nneuf\n<!-- /ash:log -->\n\n## hors périmètre\n"
        );
    }

    #[test]
    fn given_a_card_that_does_not_end_with_a_newline_when_the_block_is_appended_then_the_last_line_survives(
    ) {
        // Given — un fichier sans retour final. Coller le bloc à la suite ferait de
        // `hors périmètre<!-- ash:log -->` une seule ligne, et le marqueur ne serait plus
        // reconnaissable au tour suivant.
        let original = "## hors périmètre";

        // When
        let written = CardDocument::with_block(original, "<!-- ash:log -->\n<!-- /ash:log -->\n");

        // Then
        assert_eq!(
            written.as_str(),
            "## hors périmètre\n\n<!-- ash:log -->\n<!-- /ash:log -->\n"
        );
    }
}
