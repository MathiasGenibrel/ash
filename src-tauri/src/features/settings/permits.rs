//! Combien de commandes le test 4 lance en même temps, et qui le tient.
//!
//! `re-verify all` relance toute la liste **en parallèle** (maquette §3.2). Le test 4
//! lançant un processus par outil, sept outils déclarés voudraient dire sept programmes
//! démarrés dans la même seconde — et ces programmes-là sont des CLI d'agents, c'est-à-dire
//! souvent un runtime entier à charger. La maquette ne dit rien de la limite ; ne pas en
//! poser reviendrait à en choisir une (l'infini) sans le dire.
//!
//! **Quatre.** C'est la borne, et elle se justifie des deux côtés : au-dessous, une liste
//! de sept outils prendrait trois vagues au lieu de deux et l'utilisateur attendrait sans
//! raison ; au-dessus, on démarre plus de programmes simultanés que la machine n'a de
//! cœurs utiles pour un travail qui n'est pas le sien. Chaque lancement a par ailleurs son
//! propre délai de renoncement, donc la file avance quoi qu'il arrive.
//!
//! Le tout est un sémaphore de comptage écrit à la main : la bibliothèque standard n'en a
//! pas, et une dépendance pour vingt lignes de `Mutex` + `Condvar` ne se justifie pas.

use std::sync::{Condvar, Mutex};

/// Combien de commandes de vérification tournent au plus en même temps.
pub const MAX_CONCURRENT_PROBES: usize = 4;

/// Des jetons, dont la libération est portée par [`Permit`].
///
/// Rendre un garde plutôt qu'un `release()` à appeler est ce qui empêche la fuite : un
/// chemin de retour anticipé — et le test 4 en a plusieurs — oublierait la restitution, et
/// la limite se refermerait pour de bon sur l'application.
pub struct Permits {
    free: Mutex<usize>,
    released: Condvar,
}

impl Permits {
    pub fn new(count: usize) -> Self {
        Self {
            free: Mutex::new(count),
            released: Condvar::new(),
        }
    }

    /// Attend qu'un jeton se libère, et le prend.
    pub fn acquire(&self) -> Permit<'_> {
        let Ok(mut free) = self.free.lock() else {
            // Un fil de vérification a paniqué en tenant le verrou. Refuser de continuer
            // priverait l'écran de toute vérification ultérieure ; on renonce seulement à
            // la limite, pour cet appel.
            return Permit { permits: None };
        };
        while *free == 0 {
            match self.released.wait(free) {
                Ok(waited) => free = waited,
                Err(_) => return Permit { permits: None },
            }
        }
        *free -= 1;
        Permit {
            permits: Some(self),
        }
    }

    /// Prend un jeton s'il y en a un, sans attendre.
    ///
    /// `#[cfg(test)]` : la production attend toujours son tour — renoncer à vérifier une
    /// entrée parce que quatre autres sont en cours n'aurait aucun sens. C'est la façon de
    /// prouver la borne **sans faire dormir un test**.
    #[cfg(test)]
    pub fn try_acquire(&self) -> Option<Permit<'_>> {
        let mut free = self.free.lock().ok()?;
        if *free == 0 {
            return None;
        }
        *free -= 1;
        Some(Permit {
            permits: Some(self),
        })
    }
}

/// Un jeton pris. Le rendre est ce que fait sa destruction, et rien d'autre ne le rend.
pub struct Permit<'a> {
    permits: Option<&'a Permits>,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let Some(permits) = self.permits else {
            return;
        };
        if let Ok(mut free) = permits.free.lock() {
            *free += 1;
            permits.released.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_every_permit_taken_when_one_more_verification_asks_for_one_then_it_has_to_wait() {
        // Given — c'est la borne de `re-verify all` : sans elle, sept outils déclarés
        // démarrent sept programmes dans la même seconde
        let permits = Permits::new(MAX_CONCURRENT_PROBES);
        let held: Vec<_> = (0..MAX_CONCURRENT_PROBES)
            .map(|_| permits.try_acquire().expect("les jetons sont libres"))
            .collect();

        // When
        let extra = permits.try_acquire();

        // Then
        assert!(extra.is_none());
        drop(held);
    }

    #[test]
    fn given_a_verification_that_has_finished_when_its_permit_falls_out_of_scope_then_the_next_one_starts(
    ) {
        // Given — la restitution est portée par la destruction du garde : un chemin de
        // retour anticipé du test 4 refermerait sinon la limite pour de bon
        let permits = Permits::new(1);
        let taken = permits.try_acquire().expect("le jeton est libre");
        assert!(permits.try_acquire().is_none());

        // When
        drop(taken);

        // Then
        assert!(permits.try_acquire().is_some());
    }
}
