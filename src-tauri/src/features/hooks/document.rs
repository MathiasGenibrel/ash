//! **Ce qu'Ash s'autorise à écrire dans le fichier d'un autre.**
//!
//! C'est le cœur de la garantie d'[ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md),
//! et son amendement du 2026-08-12 l'a **reformulée, pas retirée** :
//!
//! | Avant | Après |
//! |---|---|
//! | Ash n'écrit que **dans son bloc** | Ash n'écrit que **ce qui lui appartient**, et sait le reconnaître |
//!
//! Le mécanisme change parce que le bloc délimité rendait la fusion impossible : un
//! `settings.json` qui portait déjà une clé `hooks` bloquait la fonction centrale du
//! produit, et la seule issue proposée était « déplace-les toi-même ». La promesse, elle,
//! ne change pas — et elle vit toujours **dans les types**, pas dans la prudence des
//! appelants :
//!
//! - un [`Document`] ne se compose que d'[`Edit`]s ;
//! - un [`Edit`] ne retire et ne remplace que du texte **qui porte le marqueur d'Ash**
//!   ([`Ours::covering`] refuse le reste) ;
//! - un [`Edit`] n'ajoute que du texte **qui porte le marqueur d'Ash**
//!   ([`AshText::new`] refuse le reste).
//!
//! La question que la passe d'architecture de #13 avait posée — « peut-on ajouter une
//! fonction qui écrirait hors des marqueurs sans qu'un seul test ne tombe ? » — se repose
//! donc telle quelle, et la réponse reste non : une telle fonction ne peut pas **fabriquer
//! ses arguments**. Les deux constructeurs sont totaux et vérifient, à l'exécution, la seule
//! chose qu'un type ne peut pas porter seul : que ces octets-là sont bien ceux d'Ash.

use std::ops::Range;

use crate::features::agents::HOOK_MARK;

/// Une plage du fichier qui porte le marqueur d'Ash — donc qu'il a le droit d'effacer.
#[derive(Debug, PartialEq, Eq)]
pub struct Ours(Range<usize>);

impl Ours {
    /// La plage, **si** le texte qu'elle couvre est bien d'Ash.
    ///
    /// `None` sur toute autre plage, y compris celle qui contiendrait *aussi* du texte de
    /// l'utilisateur : le marqueur doit être là, mais c'est l'appelant — le planificateur,
    /// qui a lu la structure — qui borne la plage sur les entrées d'Ash et sur elles seules.
    pub fn covering(content: &str, span: Range<usize>) -> Option<Self> {
        let text = content.get(span.clone())?;
        text.contains(HOOK_MARK).then_some(Self(span))
    }
}

/// Du texte qu'Ash reconnaîtra plus tard comme le sien.
///
/// C'est ce qui rend la désinstallation possible : chaque objet écrit porte son marqueur,
/// donc se retrouve dans un fichier que l'utilisateur a depuis réindenté, réordonné, ou
/// complété de ses propres hooks.
#[derive(Debug, PartialEq, Eq)]
pub struct AshText(String);

impl AshText {
    pub fn new(text: String) -> Option<Self> {
        text.contains(HOOK_MARK).then_some(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Une modification, et il n'y en a que trois formes.
#[derive(Debug, PartialEq, Eq)]
pub enum Edit {
    /// Ash retire ce qu'il avait écrit.
    Remove(Ours),
    /// Ash remplace ce qu'il avait écrit par ce qu'il écrirait aujourd'hui.
    Rewrite(Ours, AshText),
    /// Ash ajoute du texte à lui, à un point que le planificateur a lu dans la structure.
    ///
    /// Une insertion ne peut **rien perdre** : elle n'écrase aucun octet. C'est la raison
    /// pour laquelle la position est un simple index et non un troisième type — la seule
    /// chose à garantir est que le texte ajouté soit d'Ash, et [`AshText`] s'en charge.
    Add(usize, AshText),
}

impl Edit {
    fn at(&self) -> usize {
        match self {
            Edit::Remove(Ours(span)) | Edit::Rewrite(Ours(span), _) => span.start,
            Edit::Add(at, _) => *at,
        }
    }

    fn span(&self) -> Range<usize> {
        match self {
            Edit::Remove(Ours(span)) | Edit::Rewrite(Ours(span), _) => span.clone(),
            Edit::Add(at, _) => *at..*at,
        }
    }

    fn text(&self) -> &str {
        match self {
            Edit::Remove(_) => "",
            Edit::Rewrite(_, text) | Edit::Add(_, text) => text.as_str(),
        }
    }
}

/// Le seul texte qu'Ash sache écrire dans un fichier de l'utilisateur.
#[derive(Debug, PartialEq, Eq)]
pub struct Document(String);

impl Document {
    /// Le fichier d'origine, dont **seules** les plages nommées ont changé.
    ///
    /// Les modifications sont appliquées de la fin vers le début : chacune est décrite dans
    /// les coordonnées du fichier d'origine, et les appliquer dans l'autre sens décalerait
    /// toutes les suivantes. Une plage qui ne tomberait pas dans le texte ne vient de nulle
    /// part — elle est ignorée, parce que découper au jugé se paierait dans le
    /// `settings.json` de l'utilisateur.
    pub fn edited(original: &str, mut edits: Vec<Edit>) -> Self {
        edits.sort_by_key(|edit| std::cmp::Reverse(edit.at()));
        let mut composed = original.to_owned();
        for edit in edits {
            let span = edit.span();
            if composed.get(..span.start).is_none() || composed.get(span.end..).is_none() {
                continue;
            }
            composed.replace_range(span, edit.text());
        }
        Self(composed)
    }

    /// Le fichier qu'Ash écrit quand il n'y en avait pas.
    ///
    /// Il n'y a alors rien de l'utilisateur à préserver, et c'est la seule raison pour
    /// laquelle un document entier peut se composer.
    pub fn fresh(body: &AshText) -> Self {
        Self(format!("{{{}\n}}\n", body.as_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Un document quelconque, **réservé aux tests**.
    ///
    /// Les tests de l'adaptateur système vérifient l'écriture — son atomicité, l'absence de
    /// résidu — et n'ont rien à dire de ce qu'on écrit. Cette porte est `#[cfg(test)]` pour
    /// que le code de production n'en dispose à aucun moment.
    #[cfg(test)]
    pub fn verbatim(text: &str) -> Self {
        Self(text.to_owned())
    }
}

/// Ne reste-t-il qu'un objet vide ?
///
/// C'est la question de la désinstallation : un `settings.json` qui ne contient plus que
/// `{}` est un fichier qu'Ash a créé pour lui seul, et le laisser derrière serait une trace
/// de plus.
pub fn is_an_empty_object(document: &Document) -> bool {
    document.as_str().split_whitespace().collect::<String>() == "{}"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_span_that_covers_a_line_the_user_wrote_when_ash_tries_to_claim_it_then_it_refuses_to_call_it_its_own(
    ) {
        // Given — la garantie de la feature, prise à l'endroit exact où elle se casserait :
        // une fonction écrite plus tard qui viserait une plage quelconque du fichier. Elle
        // ne peut pas fabriquer son argument, et c'est le compilateur puis ce refus qui
        // l'arrêtent — pas la prudence de celui qui l'écrit
        let content = "{\n  \"model\": \"opus\"\n}\n";

        // When
        let claimed = Ours::covering(content, 4..20);

        // Then
        assert_eq!(claimed, None);
    }

    #[test]
    fn given_text_that_carries_no_ash_marker_when_it_is_offered_for_writing_then_it_is_refused() {
        // Given — l'autre moitié de la même garantie : ce qu'Ash ajoute doit se reconnaître
        // plus tard, sans quoi la désinstallation ne saurait pas quoi retirer
        let anonymous =
            "{\"hooks\": [{\"type\": \"command\", \"command\": \"le mien\"}]}".to_owned();

        // When
        let offered = AshText::new(anonymous);

        // Then
        assert!(offered.is_none());
    }

    #[test]
    fn given_several_edits_across_a_file_when_they_are_applied_then_each_one_lands_where_it_was_read(
    ) {
        // Given — la fusion touche plusieurs endroits d'un coup : un tableau existant, et
        // des clés à créer. Les appliquer du début vers la fin décalerait toutes les plages
        // suivantes, et Ash écrirait au milieu du texte de l'utilisateur
        let original = "{\"a\": [], \"b\": []}";
        let mine = |text: &str| {
            AshText::new(format!("{text} # ash:hook v1")).unwrap_or_else(|| panic!("marqué"))
        };
        let edits = vec![Edit::Add(7, mine("un")), Edit::Add(16, mine("deux"))];

        // When
        let composed = Document::edited(original, edits);

        // Then
        assert_eq!(
            composed.as_str(),
            "{\"a\": [un # ash:hook v1], \"b\": [deux # ash:hook v1]}"
        );
    }
}
