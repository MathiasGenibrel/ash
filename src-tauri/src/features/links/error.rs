//! Ce qui peut empêcher un lien de s'ouvrir — et rien d'autre.
//!
//! **Une seule variante, et aucun champ.** Ce n'est pas une économie : le frontend n'a
//! aucune décision à prendre — un lien refusé et un lien qu'`open` n'a pas su ouvrir se
//! ressemblent trait pour trait de l'autre côté, et rien ne se répare depuis l'écran.
//! Surtout, la valeur soumise vient d'une sortie de PTY : un message qui la recopierait
//! serait le chemin par lequel un mot choisi par un tiers finirait dans une trace, puis
//! dans un rapport de bug. `features/usage/error.rs` refuse les champs pour la même
//! famille de raison, et c'est le même réflexe qu'il faut avoir ici.

use std::fmt;

/// Le lien n'a pas été ouvert. Il n'y a rien à en dire de plus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkError {
    /// Le candidat n'est pas ouvrable — schéma hors de la liste blanche, chemin qui
    /// n'existe pas —, ou LaunchServices a refusé. Les deux cas sont **volontairement**
    /// indistinguables : la seule conduite possible est de ne rien faire.
    Unopenable,
}

impl fmt::Display for LinkError {
    /// Une phrase **fixe**, sans interpolation d'aucune sorte : voir l'en-tête du module.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("this link is not something ash opens")
    }
}

impl std::error::Error for LinkError {}

// Le frontend reçoit un message, pas une structure — comme pour `PtyError`.
impl serde::Serialize for LinkError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
