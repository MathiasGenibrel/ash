//! La cadence, le front de premier plan, et le seul portillon par lequel un appel sort.
//!
//! Les quatre conditions cumulatives d'[ADR-0016](../../../../docs/adr/0016-ash-sort-sur-le-reseau.md)
//! se tiennent ici pour trois d'entre elles — la quatrième, la destination nommée, vit dans
//! `api.rs`. Elles ne sont pas réparties : elles sont **toutes dans [`claim_turn`]**, une
//! fonction de dix lignes, et c'est délibéré. Une condition posée à deux endroits est une
//! condition qu'on perd le jour où l'un des deux change.
//!
//! ```text
//! run()  ──►  poll_once()  ──►  claim_turn()  ──►  fetch_and_publish()
//!   ▲                               │  arrêt ? premier plan ? interrupteur ? 60 s ?
//!   └── rest() ◄── réveil ──────────┘
//! ```
//!
//! ## Ce que la boucle ne fait pas
//!
//! Elle ne décide de rien. C'est un **battement** : elle appelle [`UsagePoller::poll_once`],
//! puis attend une minute ou qu'on la réveille. Tout ce qui ressemble à une règle est dans
//! le portillon, que le battement et le front empruntent tous les deux — donc, aussi, tous
//! les tests de ce fichier.
//!
//! ## Le premier plan est un front, pas seulement un niveau
//!
//! [`UsagePoller::on_window_focus`] ne fait que **réveiller** le fil de fond. Ça suffit à
//! tenir le corollaire de la condition 2 — « le retour au premier plan déclenche un appel,
//! sans attendre la fin du cycle » — parce que le portillon regarde la date du dernier
//! appel, et non la position de la boucle : après deux heures derrière une autre fenêtre,
//! le dernier appel a deux heures et le tour est dû immédiatement.
//!
//! **Et la limite d'un appel par minute tient quand même.** Un utilisateur qui bascule
//! trois fois entre Ash et son éditeur en dix secondes réveille trois fois la boucle, et le
//! portillon refuse deux fois. C'est ce qui concilie le corollaire de l'ADR avec le critère
//! d'acceptation de l'issue, sans que ni l'un ni l'autre ne cède.
//!
//! **`on_window_focus` n'appelle jamais l'hôte elle-même**, et c'est la condition 1 : elle
//! est invoquée depuis le fil de l'interface, au moment précis où l'utilisateur revient sur
//! la fenêtre. Y faire une lecture de trousseau — qui peut attendre un clic humain — et un
//! `GET` gèlerait la fenêtre à cet instant-là.
//!
//! ## Ce qui est émis, et quand
//!
//! Un [`AccountUsage`] n'est poussé que lorsqu'il **diffère** du dernier poussé. C'est ce
//! qui rend vrai « aucun décompte n'est transporté » : la valeur ne porte que des
//! pourcentages et des dates absolues, donc elle ne bouge pas tant que le quota n'a pas
//! bougé, donc l'event ne part pas. Le `2h14` de la maquette s'égrène à l'écran.
//!
//! ## Et quand ça échoue
//!
//! La valeur **disparaît** (condition 3, premier sens) : `session` et `weekly` repassent à
//! `None`. Pas de zéro, pas de tiret, et surtout pas la dernière valeur connue laissée en
//! place en la faisant passer pour fraîche. C'est aussi ce qui arrive quand l'utilisateur
//! coupe l'interrupteur : continuer d'afficher un chiffre qu'on ne rafraîchit plus serait
//! exactement le mensonge que l'ADR interdit.

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::shared::time::Clock;

use super::api::UsageApi;
use super::error::UsageError;
use super::preferences::UsagePreferences;
use super::quota::AccountUsage;
use super::token::{Credentials, Readability};

/// Un appel au plus par minute, quel que soit le nombre d'onglets et de rafraîchissements.
///
/// C'est un plancher, pas une période : le battement s'en sert aussi comme durée d'attente,
/// mais c'est le portillon qui l'impose, et lui seul.
pub const MIN_INTERVAL: Duration = Duration::from_secs(60);

/// Où la valeur va quand elle change.
///
/// Un trait, pour que la cadence et le front se vérifient sans lancer d'application Tauri.
/// Le composition root le branche sur l'event `ash://account-usage`.
pub trait UsageSink: Send + Sync {
    fn publish(&self, usage: AccountUsage);
}

/// Ce que la boucle et le front se partagent.
struct Pulse {
    /// La fenêtre d'Ash est-elle devant ? Poussé par le composition root, jamais deviné :
    /// c'est le même signal que celui qui décide des bannières de la spec §8.
    focused: bool,
    /// Quand un appel est **parti**, réussi ou non. Un échec compte : réessayer sans
    /// attendre transformerait une panne d'hôte en martèlement.
    last_call: Option<Instant>,
    /// Le dernier [`AccountUsage`] poussé — ce que la fenêtre affiche en ce moment.
    published: AccountUsage,
    stopping: bool,
}

/// Le rythme des appels d'ADR-0016, et rien d'autre.
///
/// Il ne connaît **aucun onglet**, et c'est ce qui fait que les valeurs sont les mêmes quel
/// que soit celui qui est sélectionné : il n'y a qu'un compte, donc qu'un usage, et changer
/// d'onglet n'a aucun chemin jusqu'ici.
pub struct UsagePoller {
    api: Arc<dyn UsageApi>,
    credentials: Credentials,
    preferences: Arc<UsagePreferences>,
    clock: Arc<dyn Clock>,
    sink: Arc<dyn UsageSink>,
    pulse: Mutex<Pulse>,
    wake: Condvar,
}

impl UsagePoller {
    pub fn new(
        api: Arc<dyn UsageApi>,
        credentials: Credentials,
        preferences: Arc<UsagePreferences>,
        clock: Arc<dyn Clock>,
        sink: Arc<dyn UsageSink>,
    ) -> Self {
        Self {
            api,
            credentials,
            preferences,
            clock,
            sink,
            pulse: Mutex::new(Pulse {
                // Le composition root pousse le vrai niveau au premier `Focused` ; jusque-là
                // rien ne part, ce qui est le sens de la condition 2 pour une fenêtre qui
                // n'a pas encore paru.
                focused: false,
                last_call: None,
                published: AccountUsage::unknown(),
                stopping: false,
            }),
            wake: Condvar::new(),
        }
    }

    /// Ce que la fenêtre affiche en ce moment, pour une webview qui vient de s'ouvrir.
    ///
    /// **Une lecture, jamais un appel** : c'est la condition 1. Un rendu qui demanderait la
    /// valeur à l'hôte ferait dépendre l'affichage d'un réseau lent.
    pub fn snapshot(&self) -> AccountUsage {
        self.locked().published
    }

    /// Ash appelle-t-il l'hôte ?
    pub fn polling(&self) -> bool {
        self.preferences.polling()
    }

    /// L'issue de la dernière lecture du trousseau — **et rien n'est lu ici**.
    ///
    /// C'est ce que la section `usage` de la fenêtre de réglages affiche, et la condition 1
    /// d'ADR-0016 vaut aussi pour elle : ouvrir un panneau ne fait pas surgir un dialogue de
    /// trousseau. Voir `token.rs`.
    pub fn token_readability(&self) -> Readability {
        self.credentials.readability()
    }

    /// L'interrupteur d'ADR-0016, tel que la fenêtre de réglages le bascule.
    ///
    /// Éteint, la valeur **disparaît tout de suite** : voir l'en-tête. Allumé, la boucle est
    /// réveillée — le portillon décide ensuite s'il est trop tôt.
    pub fn set_polling(&self, enabled: bool) {
        if !self.preferences.set_polling(enabled) {
            return;
        }
        if enabled {
            self.wake.notify_all();
        } else {
            self.publish(AccountUsage::unknown());
        }
    }

    /// La fenêtre a pris ou perdu le premier plan. Voir l'en-tête pour le front.
    pub fn on_window_focus(&self, focused: bool) {
        let mut pulse = self.locked();
        if pulse.focused == focused {
            return;
        }
        pulse.focused = focused;
        drop(pulse);
        if focused {
            self.wake.notify_all();
        }
    }

    /// Met le battement sur un fil à lui, et rend la main tout de suite.
    ///
    /// **C'est la seule façon de faire partir un appel depuis l'extérieur de ce module**, et
    /// c'est ce qui rend la condition 1 d'ADR-0016 *structurelle* plutôt que disciplinaire :
    /// le poller est un état `manage`d par Tauri, donc toute méthode publique qui attendrait
    /// le réseau serait à une ligne de se retrouver dans une commande, c'est-à-dire sur un
    /// chemin de rendu. Aucune ne l'attend — [`Self::snapshot`], [`Self::polling`] et
    /// [`Self::token_readability`] lisent, [`Self::on_window_focus`] et [`Self::set_polling`]
    /// réveillent, et le reste est privé.
    ///
    /// Le fil est **détaché** : personne ne le joint, et [`Self::stop`] est ce qui le fait
    /// rendre la main.
    pub fn beat_in_background(self: &Arc<Self>) {
        let beating = Arc::clone(self);
        std::thread::spawn(move || beating.run());
    }

    /// Un tour, sans jamais bloquer l'appelant sur un portillon fermé.
    ///
    /// Rend `true` si un appel est parti. C'est ce que les tests pilotent : la boucle n'a
    /// rien de plus.
    fn poll_once(&self) -> bool {
        if !self.claim_turn() {
            return false;
        }
        self.fetch_and_publish();
        true
    }

    /// Le battement, sur le fil que [`Self::beat_in_background`] lui a donné.
    fn run(&self) {
        loop {
            if self.locked().stopping {
                return;
            }
            self.poll_once();
            self.rest();
        }
    }

    /// Le fil de fond n'a plus lieu d'être.
    pub fn stop(&self) {
        self.locked().stopping = true;
        self.wake.notify_all();
    }

    /// **Le portillon** : les trois conditions d'ADR-0016 qui vivent ici, et la cadence.
    ///
    /// Rend `true` en ayant **déjà compté** le tour : deux fils qui arrivent ensemble n'en
    /// obtiennent qu'un, parce que la date du dernier appel est posée sous le même verrou
    /// que la décision.
    fn claim_turn(&self) -> bool {
        let mut pulse = self.locked();
        if pulse.stopping || !pulse.focused || !self.preferences.polling() {
            return false;
        }
        let now = self.clock.now();
        if let Some(last) = pulse.last_call {
            if now.duration_since(last) < MIN_INTERVAL {
                return false;
            }
        }
        pulse.last_call = Some(now);
        true
    }

    /// Le jeton, l'appel, et ce qui en sort — **sans tenir le verrou**.
    ///
    /// Ni la lecture du trousseau, qui peut attendre un clic humain, ni le `GET`, qui peut
    /// attendre dix secondes, ne doivent retenir [`Self::on_window_focus`] : elle arrive du
    /// fil de l'interface.
    fn fetch_and_publish(&self) {
        let found = self
            .credentials
            .token()
            .and_then(|token| self.api.fetch(&token));

        if found == Err(UsageError::Unauthorized) {
            // L'hôte dit que ce jeton-là ne vaut plus : le prochain tour en relira un.
            self.credentials.forget();
        }

        self.publish(found.unwrap_or_else(|_| AccountUsage::unknown()));
    }

    /// Pousse la valeur, **si elle a changé**. Voir l'en-tête.
    fn publish(&self, usage: AccountUsage) {
        let mut pulse = self.locked();
        if pulse.published == usage {
            return;
        }
        pulse.published = usage;
        drop(pulse);
        self.sink.publish(usage);
    }

    /// Attend le prochain battement, ou qu'on la réveille.
    fn rest(&self) {
        let pulse = self.locked();
        if pulse.stopping {
            return;
        }
        let _ = self.wake.wait_timeout(pulse, MIN_INTERVAL);
    }

    /// Un verrou empoisonné veut dire qu'un fil a paniqué **ailleurs** en le tenant. Ce
    /// qu'il protège est intact, et propager la panique ferait tomber le fil de fond pour
    /// une jauge.
    fn locked(&self) -> MutexGuard<'_, Pulse> {
        self.pulse
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::shared::time::UnixMillis;

    use super::super::preferences::{UsageChoices, UsageStore};
    use super::super::quota::Quota;
    use super::super::token::{AccessToken, TokenSource};
    use super::*;

    /// Une horloge que le scénario avance lui-même — aucun test ne dort.
    struct TestClock {
        origin: Instant,
        elapsed: AtomicU64,
    }

    impl TestClock {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                origin: Instant::now(),
                elapsed: AtomicU64::new(0),
            })
        }

        fn tick(&self, seconds: u64) {
            self.elapsed.fetch_add(seconds, Ordering::SeqCst);
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> Instant {
            self.origin + Duration::from_secs(self.elapsed.load(Ordering::SeqCst))
        }

        fn wall(&self) -> UnixMillis {
            0
        }
    }

    /// Un hôte qui compte ce qu'on lui demande, et rend ce qu'on lui a dit de rendre.
    #[derive(Default)]
    struct FakeHost {
        answers: Mutex<Vec<Result<AccountUsage, UsageError>>>,
        called: AtomicU64,
    }

    impl FakeHost {
        /// Ce que l'hôte rendra, dans l'ordre. La dernière réponse est répétée.
        fn answering(answers: Vec<Result<AccountUsage, UsageError>>) -> Arc<Self> {
            Arc::new(Self {
                answers: Mutex::new(answers),
                called: AtomicU64::new(0),
            })
        }

        fn called(&self) -> u64 {
            self.called.load(Ordering::SeqCst)
        }
    }

    impl UsageApi for FakeHost {
        fn fetch(&self, _token: &AccessToken) -> Result<AccountUsage, UsageError> {
            self.called.fetch_add(1, Ordering::SeqCst);
            let mut answers = self.answers.lock().unwrap();
            if answers.len() > 1 {
                answers.remove(0)
            } else {
                answers
                    .first()
                    .copied()
                    .unwrap_or(Ok(AccountUsage::unknown()))
            }
        }
    }

    /// Un trousseau qui rend toujours la même chose, et compte les questions.
    struct FakeKeychain {
        answer: Result<AccessToken, UsageError>,
        asked: AtomicU64,
    }

    impl FakeKeychain {
        fn holding(answer: Result<AccessToken, UsageError>) -> Arc<Self> {
            Arc::new(Self {
                answer,
                asked: AtomicU64::new(0),
            })
        }
    }

    impl TokenSource for FakeKeychain {
        fn read(&self) -> Result<AccessToken, UsageError> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            self.answer.clone()
        }
    }

    /// Ce que la fenêtre a reçu, dans l'ordre.
    #[derive(Default)]
    struct FakeScreen {
        seen: Mutex<Vec<AccountUsage>>,
    }

    impl UsageSink for FakeScreen {
        fn publish(&self, usage: AccountUsage) {
            self.seen.lock().unwrap().push(usage);
        }
    }

    impl FakeScreen {
        fn seen(&self) -> Vec<AccountUsage> {
            self.seen.lock().unwrap().clone()
        }
    }

    /// Un magasin de préférence en mémoire.
    #[derive(Default)]
    struct FakeStore(Mutex<Option<UsageChoices>>);

    impl UsageStore for FakeStore {
        fn load(&self) -> Option<UsageChoices> {
            *self.0.lock().unwrap()
        }

        fn save(&self, choices: UsageChoices) -> Result<(), std::io::Error> {
            *self.0.lock().unwrap() = Some(choices);
            Ok(())
        }
    }

    /// Un poller assemblé, avec ses doublures sous la main.
    ///
    /// Défauts **déterministes** : une fenêtre au premier plan, un jeton lisible, un hôte
    /// qui rend les deux quotas de la maquette, et l'interrupteur dans sa position par
    /// défaut. Chaque test ne surcharge que ce qu'il met en cause.
    struct PollerBuilder {
        answers: Vec<Result<AccountUsage, UsageError>>,
        keychain: Result<AccessToken, UsageError>,
        focused: bool,
        polling: bool,
    }

    impl PollerBuilder {
        fn new() -> Self {
            Self {
                answers: vec![Ok(mockup())],
                keychain: AccessToken::new("secret-abc").ok_or(UsageError::NoToken),
                focused: true,
                polling: true,
            }
        }

        fn answering(mut self, answers: Vec<Result<AccountUsage, UsageError>>) -> Self {
            self.answers = answers;
            self
        }

        fn with_keychain(mut self, keychain: Result<AccessToken, UsageError>) -> Self {
            self.keychain = keychain;
            self
        }

        fn in_the_background(mut self) -> Self {
            self.focused = false;
            self
        }

        fn with_calls_cut(mut self) -> Self {
            self.polling = false;
            self
        }

        fn build(self) -> Assembled {
            let host = FakeHost::answering(self.answers);
            let keychain = FakeKeychain::holding(self.keychain);
            let screen = Arc::new(FakeScreen::default());
            let clock = TestClock::new();
            let preferences = Arc::new(UsagePreferences::restore(Arc::new(FakeStore::default())));
            preferences.set_polling(self.polling);

            let poller = UsagePoller::new(
                Arc::clone(&host) as Arc<dyn UsageApi>,
                Credentials::from(Arc::clone(&keychain) as Arc<dyn TokenSource>),
                preferences,
                Arc::clone(&clock) as Arc<dyn Clock>,
                Arc::clone(&screen) as Arc<dyn UsageSink>,
            );
            poller.on_window_focus(self.focused);

            Assembled {
                poller,
                host,
                keychain,
                screen,
                clock,
            }
        }
    }

    struct Assembled {
        poller: UsagePoller,
        host: Arc<FakeHost>,
        keychain: Arc<FakeKeychain>,
        screen: Arc<FakeScreen>,
        clock: Arc<TestClock>,
    }

    /// Les deux couples de la maquette (vue 5b).
    fn mockup() -> AccountUsage {
        AccountUsage {
            session: Some(Quota {
                percent: 63.0,
                resets_at: Some(1_787_249_640_000),
            }),
            weekly: Some(Quota {
                percent: 28.0,
                resets_at: Some(1_787_475_600_000),
            }),
        }
    }

    #[test]
    fn given_a_minute_in_which_the_screen_asks_ten_times_when_the_quotas_are_read_then_a_single_call_went_out_and_the_value_never_moved(
    ) {
        // Given — le critère d'acceptation de l'issue : un appel au plus toutes les 60 s,
        // quel que soit le nombre d'onglets ouverts et de rafraîchissements d'écran. C'est
        // aussi la réponse au scénario « l'utilisateur passe d'un onglet à un autre » : rien
        // du poller ne connaît d'onglet, donc rien de ce qu'il rend ne dépend de celui qui
        // est sélectionné
        let assembled = PollerBuilder::new().build();

        // When
        let calls_claimed = (0..10).filter(|_| assembled.poller.poll_once()).count();
        let read: Vec<AccountUsage> = (0..10).map(|_| assembled.poller.snapshot()).collect();

        // Then
        assert_eq!(calls_claimed, 1);
        assert_eq!(assembled.host.called(), 1);
        assert_eq!(read, vec![mockup(); 10]);
        // Une seule émission : la valeur n'a pas bougé, donc l'event n'est pas reparti
        assert_eq!(assembled.screen.seen(), vec![mockup()]);
    }

    #[test]
    fn given_a_minute_gone_by_when_the_loop_beats_again_then_a_second_call_goes_out() {
        // Given — le plancher est un plancher, pas un verrou : la valeur doit vivre
        let assembled = PollerBuilder::new().build();
        assembled.poller.poll_once();

        // When
        assembled.clock.tick(59);
        let too_early = assembled.poller.poll_once();
        assembled.clock.tick(1);
        let due = assembled.poller.poll_once();

        // Then
        assert!(!too_early);
        assert!(due);
        assert_eq!(assembled.host.called(), 2);
    }

    #[test]
    fn given_a_window_that_nobody_is_looking_at_when_the_loop_beats_then_nothing_goes_out() {
        // Given — la condition 2 d'ADR-0016. Ash tourne toute la journée derrière un
        // éditeur : un appel par minute pendant huit heures d'inattention est du réseau et
        // de la batterie dépensés pour un chiffre que personne ne lira
        let assembled = PollerBuilder::new().in_the_background().build();

        // When — une heure de battements
        for _ in 0..60 {
            assembled.poller.poll_once();
            assembled.clock.tick(60);
        }

        // Then
        assert_eq!(assembled.host.called(), 0);
        assert_eq!(assembled.screen.seen(), Vec::new());
    }

    #[test]
    fn given_two_hours_spent_behind_another_window_when_ash_comes_back_to_the_front_then_a_call_goes_out_at_once(
    ) {
        // Given — le corollaire de la condition 2. Sans lui, l'utilisateur qui revient sur
        // Ash regarderait pendant une minute une valeur vieille de deux heures, présentée
        // comme fraîche — ce que la condition 3 interdit
        let assembled = PollerBuilder::new().in_the_background().build();
        assembled.clock.tick(2 * 60 * 60);

        // When
        assembled.poller.on_window_focus(true);
        let at_once = assembled.poller.poll_once();

        // Then
        assert!(at_once);
        assert_eq!(assembled.screen.seen(), vec![mockup()]);
    }

    #[test]
    fn given_a_user_alt_tabbing_three_times_in_ten_seconds_when_each_return_asks_for_a_call_then_the_minute_still_holds(
    ) {
        // Given — le front et le plancher pourraient se contredire, et c'est le portillon
        // qui les concilie : le retour au premier plan *demande* un tour, il ne le
        // *donne* pas
        let assembled = PollerBuilder::new().build();
        assembled.poller.poll_once();

        // When
        for _ in 0..3 {
            assembled.poller.on_window_focus(false);
            assembled.clock.tick(3);
            assembled.poller.on_window_focus(true);
            assembled.poller.poll_once();
        }

        // Then
        assert_eq!(assembled.host.called(), 1);
    }

    #[test]
    fn given_no_readable_token_when_ash_builds_what_it_knows_about_usage_then_both_quotas_stop_existing(
    ) {
        // Given — le scénario de l'issue, et la condition 3 d'ADR-0016 : ce qu'on n'a pas
        // disparaît. Pas de zéro, pas de tiret, et rien qui signale une panne
        let assembled = PollerBuilder::new()
            .with_keychain(Err(UsageError::NoToken))
            .build();

        // When
        assembled.poller.poll_once();

        // Then — l'hôte n'a même pas été appelé : sans jeton, il n'y a rien à demander
        assert_eq!(assembled.poller.snapshot(), AccountUsage::unknown());
        assert_eq!(assembled.host.called(), 0);
        assert_eq!(assembled.screen.seen(), Vec::new());
    }

    #[test]
    fn given_a_value_on_screen_when_the_next_call_fails_when_it_is_published_then_the_stale_one_disappears(
    ) {
        // Given — la moitié la plus facile à perdre de la condition 3 : garder le dernier
        // chiffre connu quand l'appel échoue le fait passer pour frais. C'est le mensonge
        // que l'ADR nomme
        let assembled = PollerBuilder::new()
            .answering(vec![Ok(mockup()), Err(UsageError::Unreachable)])
            .build();

        // When
        assembled.poller.poll_once();
        assembled.clock.tick(60);
        assembled.poller.poll_once();

        // Then
        assert_eq!(assembled.poller.snapshot(), AccountUsage::unknown());
        assert_eq!(
            assembled.screen.seen(),
            vec![mockup(), AccountUsage::unknown()]
        );
    }

    #[test]
    fn given_a_host_that_refuses_the_token_when_the_next_call_comes_round_then_the_keychain_is_read_again(
    ) {
        // Given — un jeton OAuth expire toutes les heures, et l'outil en écrit un neuf dans
        // le même item. Sans cette relecture, les quotas s'éteindraient au bout d'une heure
        // et ne reviendraient qu'au redémarrage d'une application qui tourne toute la
        // journée
        let assembled = PollerBuilder::new()
            .answering(vec![Err(UsageError::Unauthorized), Ok(mockup())])
            .build();

        // When
        assembled.poller.poll_once();
        assembled.clock.tick(60);
        assembled.poller.poll_once();

        // Then
        assert_eq!(assembled.keychain.asked.load(Ordering::SeqCst), 2);
        assert_eq!(assembled.poller.snapshot(), mockup());
    }

    #[test]
    fn given_the_settings_switch_turned_off_when_the_loop_beats_then_nothing_goes_out_and_the_gauge_empties(
    ) {
        // Given — la condition 3 d'ADR-0016, second sens : l'utilisateur doit pouvoir
        // couper. Et couper doit aussi retirer le chiffre : le laisser sans le rafraîchir
        // serait la même valeur périmée présentée comme fraîche
        let assembled = PollerBuilder::new().build();
        assembled.poller.poll_once();

        // When
        assembled.poller.set_polling(false);
        assembled.clock.tick(600);
        let after_cutting = assembled.poller.poll_once();

        // Then
        assert!(!after_cutting);
        assert_eq!(assembled.host.called(), 1);
        assert_eq!(assembled.poller.snapshot(), AccountUsage::unknown());
        assert_eq!(
            assembled.screen.seen(),
            vec![mockup(), AccountUsage::unknown()]
        );
    }

    #[test]
    fn given_a_session_started_with_the_calls_cut_when_the_user_turns_them_back_on_then_they_resume(
    ) {
        // Given — l'interrupteur survit au redémarrage (voir `preferences.rs`) : Ash peut
        // donc démarrer coupé, et le rallumer ne doit pas demander de redémarrer
        let assembled = PollerBuilder::new().with_calls_cut().build();

        // When
        let while_cut = assembled.poller.poll_once();
        assembled.poller.set_polling(true);
        let after = assembled.poller.poll_once();

        // Then
        assert!(!while_cut);
        assert!(after);
        assert_eq!(assembled.poller.snapshot(), mockup());
    }

    #[test]
    fn given_a_response_that_carries_only_the_weekly_quota_when_it_reaches_the_screen_then_nothing_suggests_the_session_failed(
    ) {
        // Given — le troisième scénario de l'issue. La valeur qui traverse porte deux
        // champs indépendants, et c'est ce qui laisse passer celui qui existe
        let weekly_only = AccountUsage {
            session: None,
            ..mockup()
        };
        let assembled = PollerBuilder::new()
            .answering(vec![Ok(weekly_only)])
            .build();

        // When
        assembled.poller.poll_once();

        // Then
        assert_eq!(assembled.poller.snapshot(), weekly_only);
        assert_eq!(assembled.screen.seen(), vec![weekly_only]);
    }
}
