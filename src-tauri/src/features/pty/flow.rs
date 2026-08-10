use std::sync::{Condvar, Mutex};

/// Crédits d'émission d'un PTY vers la webview.
///
/// **Pourquoi ça existe.** Le spike du jalon J0 a montré qu'au-delà de 50 Mo de données
/// non consommées, `Terminal.write()` de xterm.js lève et **jette la sortie**. Un
/// terminal qui perd de la sortie est cassé. La boucle de lecture doit donc être
/// acquittée par le rappel de `write()` côté webview, pas par le retour de `read()`.
///
/// Le mécanisme est volontairement simple : un compteur de morceaux en vol. Le lecteur
/// prend un crédit avant d'émettre et se bloque quand il n'en reste plus ; le PTY se
/// remplit alors, et le programme qui écrit dedans se bloque à son tour. C'est
/// exactement la contre-pression qu'on veut — celle du système, pas une file en mémoire.
pub struct Credits {
    state: Mutex<State>,
    awoken: Condvar,
}

struct State {
    available: usize,
    closed: bool,
}

impl Credits {
    pub fn new(window: usize) -> Self {
        Self {
            state: Mutex::new(State {
                available: window,
                closed: false,
            }),
            awoken: Condvar::new(),
        }
    }

    /// Consomme un crédit, en attendant qu'il s'en libère un.
    ///
    /// Rend `false` quand les crédits ont été fermés : c'est le signal d'arrêt du
    /// lecteur. Un verrou empoisonné compte aussi comme une fermeture — le thread qui
    /// l'a empoisonné est mort, il n'y a plus personne pour acquitter.
    pub fn acquire(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };

        while state.available == 0 && !state.closed {
            let Ok(next) = self.awoken.wait(state) else {
                return false;
            };
            state = next;
        }

        if state.closed {
            return false;
        }

        state.available -= 1;
        true
    }

    /// Rend un crédit : la webview a fini d'écrire un morceau dans xterm.js.
    pub fn release(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.available += 1;
            self.awoken.notify_one();
        }
    }

    /// Ferme les crédits et réveille le lecteur, qu'il attende ou non.
    ///
    /// Sans ça, fermer un onglet pendant que son shell est silencieux laisserait un
    /// thread bloqué sur le `Condvar` jusqu'à la fin du processus.
    pub fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            self.awoken.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn given_a_window_of_two_when_two_chunks_are_in_flight_then_the_third_waits() {
        // Given
        let credits = Arc::new(Credits::new(2));
        assert!(credits.acquire());
        assert!(credits.acquire());

        // When
        let waiting = Arc::clone(&credits);
        let reader = std::thread::spawn(move || waiting.acquire());
        std::thread::sleep(Duration::from_millis(20));
        assert!(!reader.is_finished(), "le lecteur aurait dû être bloqué");

        // Then
        credits.release();
        assert!(reader.join().unwrap_or(false));
    }

    #[test]
    fn given_a_reader_waiting_for_a_credit_when_the_tab_closes_then_it_is_released_with_a_stop() {
        // Given
        let credits = Arc::new(Credits::new(1));
        assert!(credits.acquire());
        let waiting = Arc::clone(&credits);
        let reader = std::thread::spawn(move || waiting.acquire());
        std::thread::sleep(Duration::from_millis(20));
        assert!(!reader.is_finished());

        // When
        credits.close();

        // Then
        assert!(
            !reader.join().unwrap_or(true),
            "un lecteur réveillé par la fermeture doit s'arrêter, pas émettre"
        );
    }

    #[test]
    fn given_closed_credits_when_acquiring_then_it_refuses_immediately() {
        // Given
        let credits = Credits::new(4);
        credits.close();

        // When
        let granted = credits.acquire();

        // Then
        assert!(!granted);
    }
}
