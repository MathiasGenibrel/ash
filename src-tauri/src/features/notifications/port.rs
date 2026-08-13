//! Ce qu'une bannière porte, ce que macOS en dit, et ce que le clic rend.
//!
//! Trois types et un trait, sans une ligne d'Objective-C : c'est ce qui permet à `agents`
//! et à `settings` de parler de notifications dans leurs tests sans qu'aucune bannière
//! n'apparaisse sur l'écran de qui lance `cargo test`.

use std::sync::Arc;

/// Ce qu'Ash demande à macOS d'afficher.
///
/// **`payload` est opaque pour cette feature**, et c'est ce qui la garde générale : elle
/// le confie à macOS avec la bannière, et le rend tel quel au clic. C'est le composition
/// root qui sait qu'il s'agit d'un identifiant d'onglet — ici, ce n'est qu'une chaîne qui
/// fait l'aller-retour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    pub payload: String,
    pub title: String,
    pub body: String,
}

/// Ce que macOS répond quand on lui demande si Ash a le droit d'interrompre.
///
/// Trois valeurs pour quatre `UNAuthorizationStatus`, et le regroupement est une décision :
/// voir [`super::macos::authorization_of`], qui la porte et qui est éprouvée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    Granted,
    /// Refusée. **C'est la valeur que la spec §8 veut voir affichée**, et celle qu'aucune
    /// pile du projet n'avait su produire jusqu'ici.
    Denied,
    /// macOS n'a pas répondu — ou n'a pas de réponse à donner. C'est le cas hors d'une
    /// application empaquetée, où le centre de notifications n'existe pas pour Ash.
    Undisclosed,
}

/// Ce que le système fait de l'aveu qu'un agent attend.
///
/// Un trait, pour la raison habituelle du dépôt : les effets système passent par un trait
/// que la feature possède. Celui-ci en abstrait deux, et ils vont ensemble — ce qui *pose*
/// la bannière et ce qui *sait* si elle a le droit d'arriver sont le même centre de
/// notifications, et les séparer laisserait la fenêtre de réglages décrire l'autorisation
/// d'un mécanisme dont elle ne serait pas sûre qu'il soit celui qui poste.
pub trait Banners: Send + Sync {
    /// Pose la bannière. Ne rend rien : une notification perdue ne change aucun état, et
    /// rien du produit ne doit pouvoir dépendre de sa réussite.
    fn post(&self, banner: Banner);

    /// Ce que macOS dit de l'autorisation, **maintenant**.
    ///
    /// Relu à chaque fois plutôt que retenu : l'utilisateur peut couper les notifications
    /// d'Ash dans les Réglages Système pendant qu'Ash est ouvert, et la fenêtre de réglages
    /// est précisément l'endroit où il ira vérifier.
    fn authorization(&self) -> Authorization;
}

/// Ce qu'on fait d'un clic sur une bannière : rejouer le `payload` qu'elle portait.
///
/// **Ce rappel arrive sur le fil principal**, celui de l'interface, et il n'appartient pas
/// à cette feature de décider ce que le `payload` veut dire. Il doit donc rendre la main
/// tout de suite : ce que le composition root en fait est une émission d'event, qui ne
/// bloque pas.
pub type Clicked = Arc<dyn Fn(&str) + Send + Sync>;
