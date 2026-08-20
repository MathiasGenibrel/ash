//! Les deux quotas tels qu'ils traversent la frontière, et la lecture **défensive** de la
//! réponse dont ils sortent.
//!
//! ## Ce qui traverse est une date, jamais un décompte
//!
//! La maquette (vue 5b) montre `resets in 2h14`, et c'est un **fait d'affichage**. Ce qui
//! passe la frontière est [`Quota::resets_at`], une date absolue en millisecondes depuis
//! l'époque Unix — exactement comme `TabInfo.stateSince`, et pour la même raison : un
//! décompte transporté ferait repartir l'event chaque seconde pour animer un compteur,
//! alors que la valeur ne change qu'au rythme du quota.
//!
//! ## La durée de la fenêtre n'est écrite nulle part, et c'est un résultat
//!
//! `5 h` n'apparaît pas dans ce fichier, et il ne doit pas y apparaître. **Ce n'est pas un
//! oubli, et ce n'est pas non plus une valeur que l'API donnerait** : l'API n'expose aucune
//! durée de fenêtre, ni dans les seaux plats ni dans `limits[]`. La seule forme sous laquelle
//! elle décrit une fenêtre est le moment où celle-ci se remet à zéro (`resets_at`) — et c'est
//! exactement ce dont l'écran a besoin pour afficher `2h14`. Le critère « la durée vient de
//! l'API, et n'est pas écrite en dur » est donc tenu par la négative : Ash n'a jamais besoin
//! de cette durée, donc il n'a jamais l'occasion de la coder.
//!
//! La clé historique `five_hour` **nomme** une durée de cinq heures ; on ne s'en sert pas pour
//! en déduire quoi que ce soit, parce qu'une fenêtre qui passerait à quatre heures changerait
//! la valeur sans changer la clé.
//!
//! **Ce qui rouvrirait la question**, et qu'il faut savoir avant de l'ouvrir : dessiner une
//! barre de progression *de la fenêtre* — « on est à 40 % des cinq heures » — et non du quota
//! demanderait la durée, qui n'existe nulle part dans la réponse. C'est précisément pour cet
//! usage-là que `ccstatusline` code `FIVE_HOUR_BLOCK_MS` en dur dans `usage-windows.ts` : il
//! en dessine une. Ash n'en dessine pas, et tant qu'il n'en dessine pas, il n'a rien à
//! deviner.
//!
//! ## La forme de la réponse, et pourquoi elle est lue deux fois
//!
//! L'hôte rend deux formes selon l'ancienneté du compte, et la même réponse peut porter les
//! deux :
//!
//! ```json
//! { "five_hour": { "utilization": 63, "resets_at": "2026-08-20T18:14:00Z" },
//!   "seven_day": { "utilization": 28, "resets_at": "2026-08-23T09:00:00Z" } }
//! ```
//!
//! ```json
//! { "limits": [ { "kind": "session",    "percent": 63, "resets_at": "2026-08-20T18:14:00Z" },
//!               { "kind": "weekly_all", "percent": 28, "resets_at": "2026-08-23T09:00:00Z" } ] }
//! ```
//!
//! Les seaux plats passent en premier, `limits[]` sert de repli — l'ordre de `ccstatusline`,
//! qui a vu les deux en production. **Chaque champ tombe indépendamment** : un compte migré
//! peut porter le pourcentage d'un côté et la date de l'autre.
//!
//! Rien d'autre n'est lu. `weekly_scoped`, `extra_usage`, les seaux par modèle : la spec
//! §4.2 demande deux couples, et un champ qu'on ne montre pas n'a pas à entrer.

use crate::shared::time::UnixMillis;

use super::error::UsageError;

/// Un quota : où il en est, et quand il repart de zéro.
///
/// `resets_at` est facultatif parce que l'hôte le rend parfois nul — un compte dont la
/// fenêtre n'a pas commencé, une entreprise sans fenêtre de limitation. Le pourcentage
/// passe quand même : la maquette montre deux choses, et n'en avoir qu'une vaut mieux que
/// n'avoir rien.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    /// Entre `0` et `100`. L'hôte le rend parfois fractionnaire.
    pub percent: f64,
    /// Quand la fenêtre repart de zéro, en millisecondes depuis l'époque Unix.
    #[cfg_attr(test, ts(type = "number | null"))]
    pub resets_at: Option<UnixMillis>,
}

/// Ce qu'Ash sait de l'usage du **compte** — pas d'un onglet, pas d'un worktree.
///
/// Les quotas sont transverses : ils ne dépendent d'aucune sélection, et changer d'onglet
/// ne les touche pas. C'est pour cette raison qu'aucun identifiant d'onglet n'entre dans ce
/// type, ni dans la feature qui le produit.
///
/// **Les deux champs sont indépendants** : un compte migré, ou une réponse partielle, laisse
/// passer celui qui existe. Et les deux à `None` sont ce que « la valeur disparaît » veut
/// dire (condition 3 d'ADR-0016) : ni un zéro, ni un tiret, ni la dernière valeur connue.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    /// La fenêtre courte — cinq heures aujourd'hui, et Ash n'en sait rien.
    pub session: Option<Quota>,
    /// La fenêtre longue — sept jours aujourd'hui, et Ash n'en sait rien non plus.
    pub weekly: Option<Quota>,
}

impl AccountUsage {
    /// Ce qu'Ash affiche quand il ne sait rien : rien.
    #[must_use]
    pub fn unknown() -> Self {
        Self::default()
    }
}

/// Les deux quotas dans le corps de la réponse.
///
/// **Ne rend `Err` que si le corps n'est pas un document JSON.** Un document qu'on comprend
/// mais où aucun quota ne figure rend un [`AccountUsage::unknown`] : c'est la même chose à
/// l'écran, et ça évite d'appeler « panne » ce qui est peut-être un compte sans abonnement.
pub fn read_usage(body: &str) -> Result<AccountUsage, UsageError> {
    let parsed =
        serde_json::from_str::<serde_json::Value>(body).map_err(|_| UsageError::Unreadable)?;

    Ok(AccountUsage {
        session: quota(&parsed, "five_hour", "session"),
        weekly: quota(&parsed, "seven_day", "weekly_all"),
    })
}

/// Un quota, cherché dans le seau plat puis dans `limits[]`.
///
/// Les deux champs tombent **séparément** sur le repli : un compte migré peut porter le
/// pourcentage dans l'un et la date dans l'autre, et exiger que les deux viennent de la
/// même source ferait perdre les deux.
fn quota(parsed: &serde_json::Value, bucket_key: &str, kind: &str) -> Option<Quota> {
    let bucket = real(parsed.get(bucket_key), "utilization");
    let limit = real(
        parsed
            .get("limits")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .find(|entry| entry.get("kind").and_then(serde_json::Value::as_str) == Some(kind)),
        "percent",
    );

    let percent = bucket
        .and_then(|found| found.0)
        .or_else(|| limit.and_then(|found| found.0))?;
    let resets_at = bucket
        .and_then(|found| found.1)
        .or_else(|| limit.and_then(|found| found.1));

    Some(Quota { percent, resets_at })
}

/// Le pourcentage et la date d'une entrée, ou `None` si l'entrée ne dit rien de réel.
///
/// **Une entrée à `0 %` sans date de remise à zéro n'est pas une fenêtre**, c'est un
/// bouchon : les comptes d'entreprise n'ont pas de fenêtre de limitation, et l'hôte les
/// décrit ainsi. La laisser passer afficherait `0 %` là où il n'y a rien à afficher — un
/// chiffre inventé, ce que la condition 3 d'ADR-0016 interdit.
fn real(
    entry: Option<&serde_json::Value>,
    percent_key: &str,
) -> Option<(Option<f64>, Option<UnixMillis>)> {
    let entry = entry?;
    let percent = entry
        .get(percent_key)
        .and_then(serde_json::Value::as_f64)
        .filter(|found| found.is_finite())
        .map(|found| found.clamp(0.0, 100.0));
    // `resets_at` n'est lu que sous forme de texte RFC 3339 : c'est la seule que l'API
    // d'usage produise. La forme numérique existe ailleurs — la charge du hook `Status`
    // porte des secondes Unix —, et la reconnaître ici obligerait à deviner l'unité d'un
    // nombre. Un quota dont la date ne se lit pas garde son pourcentage.
    let resets_at = entry
        .get("resets_at")
        .and_then(serde_json::Value::as_str)
        .and_then(epoch_millis);

    if percent.unwrap_or(0.0) == 0.0 && resets_at.is_none() {
        return None;
    }
    Some((percent, resets_at))
}

/// Une date RFC 3339 en millisecondes depuis l'époque, ou `None`.
///
/// Écrit à la main plutôt qu'avec `chrono` ou `time` : ADR-0016 vient d'ajouter une
/// dépendance à un arbre qui en compte peu, et ce qu'on a à lire est un format à longueur
/// fixe dont on ne veut ni le calendrier, ni les fuseaux nommés, ni le formatage.
///
/// Les trois formes observées, toutes acceptées :
/// `2026-08-20T18:14:00Z`, `2026-08-20T18:14:00.000Z`, `2026-08-20T18:14:00.885205+00:00`.
fn epoch_millis(text: &str) -> Option<UnixMillis> {
    let (date, rest) = text.split_once(['T', 't', ' '])?;
    let mut date = date.splitn(3, '-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;

    // Ce qui suit l'heure : `Z`, un décalage, ou rien.
    let (clock, offset) = match rest.find(['Z', 'z', '+']) {
        Some(at) => (&rest[..at], &rest[at..]),
        // Le `-` d'un décalage négatif, cherché après l'heure pour ne pas confondre avec
        // les tirets de la date, qui ont déjà été consommés.
        None => match rest.rfind('-') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, ""),
        },
    };

    let (clock, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut clock = clock.splitn(3, ':');
    let hours: i64 = clock.next()?.parse().ok()?;
    let minutes: i64 = clock.next()?.parse().ok()?;
    let seconds: i64 = clock.next().unwrap_or("0").parse().ok()?;

    // Les millisecondes sont la résolution du `Date` du TypeScript : les six chiffres que
    // l'hôte rend parfois sont tronqués, jamais arrondis.
    let millis: i64 = match fraction.len() {
        0 => 0,
        _ => {
            let mut padded = fraction.to_owned();
            padded.truncate(3);
            while padded.len() < 3 {
                padded.push('0');
            }
            padded.parse().ok()?
        }
    };

    let shift = match offset.as_bytes().first() {
        None | Some(b'Z') | Some(b'z') => 0,
        Some(sign) => {
            let signum = if *sign == b'-' { -1 } else { 1 };
            let digits: String = offset[1..].chars().filter(char::is_ascii_digit).collect();
            if digits.len() != 4 {
                return None;
            }
            let hours: i64 = digits[..2].parse().ok()?;
            let minutes: i64 = digits[2..].parse().ok()?;
            signum * (hours * 3600 + minutes * 60)
        }
    };

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let since_epoch =
        days_from_civil(year, month, day) * 86_400 + hours * 3600 + minutes * 60 + seconds - shift;

    // Une date antérieure à 1970 ne se dit pas en millisecondes non signées, et une remise
    // à zéro déjà passée est parfaitement légitime : ce qui est rejeté ici est un document
    // absurde, pas une date ancienne.
    UnixMillis::try_from(since_epoch.checked_mul(1000)?.checked_add(millis)?).ok()
}

/// Le nombre de jours entre le 1er janvier 1970 et cette date du calendrier grégorien.
///
/// L'algorithme `days_from_civil` de Howard Hinnant, qui vaut sur tout l'intervalle utile
/// et n'a pas de table d'années bissextiles à tenir à jour.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une réponse d'hôte, dans l'une ou l'autre de ses deux formes.
    ///
    /// Le constructeur ne pose **rien** : chaque test dit exactement ce que sa réponse
    /// porte, parce que la question posée à ce module est précisément « qu'est-ce qui
    /// manque, et qu'est-ce qui passe quand même ».
    #[derive(Default)]
    struct ResponseBuilder {
        buckets: Vec<String>,
        limits: Vec<String>,
    }

    impl ResponseBuilder {
        fn new() -> Self {
            Self::default()
        }

        /// Un seau plat — la forme historique.
        fn bucket(mut self, key: &str, body: &str) -> Self {
            self.buckets.push(format!("\"{key}\":{body}"));
            self
        }

        /// Une entrée de `limits[]` — la forme des comptes migrés.
        fn limit(mut self, body: &str) -> Self {
            self.limits.push(body.to_owned());
            self
        }

        fn build(self) -> String {
            let mut fields = self.buckets;
            if !self.limits.is_empty() {
                fields.push(format!("\"limits\":[{}]", self.limits.join(",")));
            }
            format!("{{{}}}", fields.join(","))
        }
    }

    /// Le 20 août 2026 à 18h14 UTC, la date de la maquette.
    const MOCKUP_RESET: UnixMillis = 1_787_249_640_000;

    #[test]
    fn given_the_two_flat_buckets_when_the_response_is_read_then_each_quota_carries_a_percentage_and_an_absolute_reset_date(
    ) {
        // Given — la forme historique, et les deux couples de la maquette (vue 5b)
        let body = ResponseBuilder::new()
            .bucket(
                "five_hour",
                r#"{"utilization":63,"resets_at":"2026-08-20T18:14:00Z"}"#,
            )
            .bucket(
                "seven_day",
                r#"{"utilization":28,"resets_at":"2026-08-23T09:00:00Z"}"#,
            )
            .build();

        // When
        let usage = read_usage(&body);

        // Then — une date, jamais un décompte : `2h14` se calcule à l'écran
        assert_eq!(
            usage,
            Ok(AccountUsage {
                session: Some(Quota {
                    percent: 63.0,
                    resets_at: Some(MOCKUP_RESET),
                }),
                weekly: Some(Quota {
                    percent: 28.0,
                    resets_at: Some(1_787_475_600_000),
                }),
            })
        );
    }

    #[test]
    fn given_a_migrated_account_that_only_reports_limits_when_the_response_is_read_then_both_quotas_still_come_out(
    ) {
        // Given — la forme des comptes migrés (#503 de ccstatusline) : plus de seaux plats,
        // un tableau `limits[]` dont chaque entrée porte son `kind`
        let body = ResponseBuilder::new()
            .limit(r#"{"kind":"session","percent":63,"resets_at":"2026-08-20T18:14:00Z","scope":null}"#)
            .limit(r#"{"kind":"weekly_all","percent":28,"resets_at":"2026-08-23T09:00:00Z","scope":null}"#)
            .limit(r#"{"kind":"weekly_scoped","percent":3,"resets_at":"2026-08-23T09:00:00Z","scope":{"model":{"display_name":"Sonnet"}}}"#)
            .build();

        // When
        let usage = read_usage(&body);

        // Then — et le quota par modèle, que la spec §4.2 ne demande pas, n'est pas entré
        assert_eq!(
            usage,
            Ok(AccountUsage {
                session: Some(Quota {
                    percent: 63.0,
                    resets_at: Some(MOCKUP_RESET),
                }),
                weekly: Some(Quota {
                    percent: 28.0,
                    resets_at: Some(1_787_475_600_000),
                }),
            })
        );
    }

    #[test]
    fn given_a_response_that_carries_the_weekly_quota_but_not_the_session_when_it_is_read_then_the_weekly_passes_alone(
    ) {
        // Given — un compte migré à moitié, ou une réponse partielle. Perdre le quota
        // présent parce que l'autre manque serait punir l'utilisateur d'un changement de
        // format chez l'hôte
        let body = ResponseBuilder::new()
            .bucket(
                "seven_day",
                r#"{"utilization":28,"resets_at":"2026-08-23T09:00:00Z"}"#,
            )
            .build();

        // When
        let usage = read_usage(&body);

        // Then — et rien ne suggère que la session a échoué : elle n'existe pas
        assert_eq!(
            usage,
            Ok(AccountUsage {
                session: None,
                weekly: Some(Quota {
                    percent: 28.0,
                    resets_at: Some(1_787_475_600_000),
                }),
            })
        );
    }

    #[test]
    fn given_a_migrated_account_whose_flat_bucket_kept_only_the_date_when_it_is_read_then_the_percentage_comes_from_limits(
    ) {
        // Given — l'hôte a été vu rendre les deux formes dans la même réponse, chacune
        // incomplète. Exiger que le pourcentage et la date viennent de la même source
        // ferait perdre les deux
        let body = ResponseBuilder::new()
            .bucket("five_hour", r#"{"resets_at":"2026-08-20T18:14:00Z"}"#)
            .limit(r#"{"kind":"session","percent":63,"resets_at":null}"#)
            .build();

        // When
        let usage = read_usage(&body);

        // Then
        assert_eq!(
            usage.map(|found| found.session),
            Ok(Some(Quota {
                percent: 63.0,
                resets_at: Some(MOCKUP_RESET),
            }))
        );
    }

    #[test]
    fn given_an_account_that_has_no_usage_window_at_all_when_it_is_read_then_no_quota_is_invented()
    {
        // Given — les comptes d'entreprise n'ont pas de fenêtre de limitation, et l'hôte
        // les décrit par un bouchon : zéro pour cent, et aucune remise à zéro. L'afficher
        // dirait « 0 % consommé » là où il n'y a rien de mesuré
        let body = ResponseBuilder::new()
            .bucket("five_hour", r#"{"utilization":0,"resets_at":null}"#)
            .bucket("seven_day", "null")
            .limit(r#"{"kind":"weekly_all","percent":0,"resets_at":null}"#)
            .build();

        // When
        let usage = read_usage(&body);

        // Then
        assert_eq!(usage, Ok(AccountUsage::unknown()));
    }

    #[test]
    fn given_a_body_that_is_not_a_json_document_when_it_is_read_then_nothing_is_understood() {
        // Given — une page d'erreur d'un proxy d'entreprise, une réponse tronquée
        let bodies = ["", "<html>403 Forbidden</html>", "{\"limits\":"];

        // When
        let read: Vec<Result<AccountUsage, UsageError>> =
            bodies.iter().map(|body| read_usage(body)).collect();

        // Then
        assert_eq!(read, vec![Err(UsageError::Unreadable); bodies.len()]);
    }

    #[test]
    fn given_a_document_ash_understands_but_that_names_no_quota_when_it_is_read_then_it_is_not_a_failure(
    ) {
        // Given — un compte sans abonnement mesuré, ou une réponse dont tous les champs ont
        // changé de nom. À l'écran c'est la même chose qu'un échec — rien —, mais appeler
        // ça une panne ferait relire le jeton pour rien
        let body = r#"{"account":{"kind":"enterprise"}}"#;

        // When
        let usage = read_usage(body);

        // Then
        assert_eq!(usage, Ok(AccountUsage::unknown()));
    }

    #[test]
    fn given_the_shapes_of_reset_dates_the_host_produces_when_they_are_read_then_they_all_land_on_the_same_instant(
    ) {
        // Given — les trois formes observées, plus un décalage non nul. Une seconde de
        // décalage sur une remise à zéro se voit à l'écran : `2h14` deviendrait `1h14`
        let same_instant = [
            "2026-08-20T18:14:00Z",
            "2026-08-20T18:14:00.000Z",
            "2026-08-20T20:14:00+02:00",
            "2026-08-20T13:14:00-05:00",
        ];

        // When
        let read: Vec<Option<UnixMillis>> =
            same_instant.iter().map(|at| epoch_millis(at)).collect();
        // Les six chiffres que l'hôte rend parfois : la milliseconde est la résolution du
        // `Date` du TypeScript, et ce qui dépasse est tronqué plutôt qu'arrondi.
        let with_microseconds = epoch_millis("2026-08-20T18:14:00.885205+00:00");

        // Then
        assert_eq!(read, vec![Some(MOCKUP_RESET); same_instant.len()]);
        assert_eq!(with_microseconds, Some(MOCKUP_RESET + 885));
    }

    #[test]
    fn given_a_reset_date_in_a_leap_year_when_it_is_read_then_the_extra_day_is_counted() {
        // Given — le 29 février existe, et une fenêtre hebdomadaire le traverse une année
        // sur quatre. Une table d'années bissextiles oubliée décalerait la date d'un jour
        // entier, et `3d 09h` deviendrait `2d 09h`
        let leap_day = epoch_millis("2028-02-29T00:00:00Z");
        let day_after = epoch_millis("2028-03-01T00:00:00Z");

        // When
        let gap = day_after.zip(leap_day).map(|(after, at)| after - at);

        // Then
        assert_eq!(leap_day, Some(1_835_395_200_000));
        assert_eq!(gap, Some(86_400_000));
    }

    #[test]
    fn given_a_reset_date_that_says_nothing_readable_when_it_is_read_then_the_quota_keeps_its_percentage(
    ) {
        // Given — le format évoluera. Une date illisible ne doit pas emporter le
        // pourcentage, qui est l'essentiel de la pastille
        let body = ResponseBuilder::new()
            .bucket(
                "five_hour",
                r#"{"utilization":63,"resets_at":"le vingt août"}"#,
            )
            .bucket("seven_day", r#"{"utilization":28,"resets_at":1787480400}"#)
            .build();

        // When
        let usage = read_usage(&body);

        // Then — la forme numérique est celle d'une autre charge utile, pas de celle-ci :
        // en accepter une obligerait à deviner si elle compte des secondes ou des
        // millisecondes
        assert_eq!(
            usage,
            Ok(AccountUsage {
                session: Some(Quota {
                    percent: 63.0,
                    resets_at: None,
                }),
                weekly: Some(Quota {
                    percent: 28.0,
                    resets_at: None,
                }),
            })
        );
    }

    #[test]
    fn given_a_percentage_outside_what_a_percentage_can_be_when_it_is_read_then_it_is_brought_back_between_zero_and_a_hundred(
    ) {
        // Given — une barre de progression dessinée à partir de `-4` ou de `142` sort de
        // sa piste, et un `NaN` traverse `serde` sans se faire remarquer jusqu'au CSS
        let body = ResponseBuilder::new()
            .bucket(
                "five_hour",
                r#"{"utilization":142,"resets_at":"2026-08-20T18:14:00Z"}"#,
            )
            .bucket(
                "seven_day",
                r#"{"utilization":-4,"resets_at":"2026-08-23T09:00:00Z"}"#,
            )
            .build();

        // When
        let usage = read_usage(&body).unwrap_or_default();

        // Then
        assert_eq!(usage.session.map(|found| found.percent), Some(100.0));
        assert_eq!(usage.weekly.map(|found| found.percent), Some(0.0));
    }
}
