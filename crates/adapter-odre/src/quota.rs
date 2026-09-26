//! Observation du quota ODRÉ réel (ADR-0022 addendum 2026-09-26, plan I8 PROD-3).
//!
//! ODRÉ renvoie, sur **chaque** réponse (`records` comme `exports/json`), des
//! en-têtes de quota **par jeu de données et par client (IP)** :
//! `x-ratelimit-dataset-limit` / `-remaining` / `-reset` (50 000/mois par jeu).
//! Il existe aussi un quota API global (`x-ratelimit-limit`/`-remaining`/`-reset`,
//! 10 000 000, tous jeux confondus) — beaucoup moins intéressant : c'est le quota
//! *par jeu* qui sert de garde-fou au poller (ADR-0003).
//!
//! Jusqu'ici rien de ce quota réel n'était capté : `carbonfr_upstream_requests_total`
//! (`bin/server/src/metrics.rs`) ne compte que les appels *initiés* par le
//! processus (un **proxy**), pas ce qu'ODRÉ en pense (dérive possible avec
//! d'autres consommateurs de la même IP, en-têtes qui changeraient de sémantique…).
//!
//! [`QuotaTracker`] lit ces en-têtes de façon **purement opportuniste** : 0 appel
//! HTTP supplémentaire, juste une lecture des en-têtes déjà présents dans les
//! réponses de [`fetch`](crate::OdreClient) et [`fetch_export`](crate::OdreClient),
//! et garde la **dernière** observation connue par jeu de données. La composition
//! root (`bin/server`) en lit un instantané ([`QuotaTracker::snapshot`]) pour le
//! rendre en jauges Prometheus sur `/metrics`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use reqwest::header::HeaderMap;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Dernière observation connue du quota ODRÉ pour un jeu de données.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatasetQuota {
    /// Nom du jeu de données ODRÉ (ex. `eco2mix-national-tr`).
    pub dataset: String,
    /// Plafond mensuel du jeu (`x-ratelimit-dataset-limit`).
    pub limit: u64,
    /// Appels restants avant remise à zéro (`x-ratelimit-dataset-remaining`).
    pub remaining: u64,
    /// Horodatage Unix (s) de la prochaine remise à zéro
    /// (`x-ratelimit-dataset-reset`) ; `None` si l'en-tête est absent ou
    /// illisible — n'empêche pas d'enregistrer `limit`/`remaining`.
    pub reset_unix: Option<i64>,
    /// Horodatage Unix (s) auquel cette observation a été faite (horloge locale
    /// du processus, pas celle d'ODRÉ).
    pub observed_unix: i64,
}

/// Traceur partagé du quota ODRÉ, par jeu de données.
///
/// `std::sync::Mutex` (pas `tokio::sync::Mutex`) : section critique triviale
/// (copie de quelques champs dans une `BTreeMap`), jamais tenue à travers un
/// point d'attente async — pas besoin d'un mutex asynchrone. Un verrou
/// empoisonné (panique d'un appelant pendant qu'il le tenait) ne doit jamais
/// faire paniquer un simple lecteur de quota : `unwrap_or_else(|e| e.into_inner())`
/// récupère la map quand même, cohérente (la panique ne peut survenir
/// qu'entre deux opérations complètes dessus).
///
/// `Clone` bon marché (`Arc`) : chaque clone d'[`OdreClient`](crate::OdreClient)
/// partage le même traceur, c'est voulu (un seul traceur pour tous les usages
/// du client dans le processus serveur).
#[derive(Clone, Default)]
pub struct QuotaTracker(Arc<Mutex<BTreeMap<String, DatasetQuota>>>);

impl QuotaTracker {
    /// Traceur vide (aucune observation).
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe les en-têtes de quota d'une réponse ODRÉ pour `dataset`.
    ///
    /// Opportuniste : si `x-ratelimit-dataset-limit` ou `-remaining` est absent
    /// ou n'est pas un entier, **rien n'est enregistré** — retour silencieux,
    /// jamais d'erreur (une observation manquée ne doit jamais faire échouer
    /// l'appel ODRÉ qui la porte). `x-ratelimit-dataset-reset` est optionnel :
    /// son format ODRÉ (`AAAA-MM-JJ HH:MM:SS+00:00`) est rendu RFC 3339 en
    /// remplaçant l'espace séparateur par `T` puis parsé ; un échec de parsing
    /// laisse `reset_unix` à `None` sans empêcher d'enregistrer `limit`/`remaining`.
    pub fn observe(&self, dataset: &str, headers: &HeaderMap, now: OffsetDateTime) {
        let Some(limit) = header_u64(headers, "x-ratelimit-dataset-limit") else {
            return;
        };
        let Some(remaining) = header_u64(headers, "x-ratelimit-dataset-remaining") else {
            return;
        };
        let reset_unix = header_str(headers, "x-ratelimit-dataset-reset").and_then(parse_reset);

        let quota = DatasetQuota {
            dataset: dataset.to_string(),
            limit,
            remaining,
            reset_unix,
            observed_unix: now.unix_timestamp(),
        };
        let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert(quota.dataset.clone(), quota);
    }

    /// Instantané de toutes les observations connues, trié par nom de jeu
    /// (garanti par la `BTreeMap` sous-jacente).
    pub fn snapshot(&self) -> Vec<DatasetQuota> {
        let guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.values().cloned().collect()
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    header_str(headers, name)?.trim().parse().ok()
}

/// `AAAA-MM-JJ HH:MM:SS+00:00` (format ODRÉ) → Unix (s). `None` si absent ou
/// illisible.
fn parse_reset(raw: &str) -> Option<i64> {
    let rfc3339 = raw.replacen(' ', "T", 1);
    OffsetDateTime::parse(&rfc3339, &Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderName, HeaderValue};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn observes_full_headers() {
        let tracker = QuotaTracker::new();
        let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        tracker.observe(
            "eco2mix-national-tr",
            &headers(&[
                ("x-ratelimit-dataset-limit", "50000"),
                ("x-ratelimit-dataset-remaining", "49977"),
                ("x-ratelimit-dataset-reset", "2026-10-01 00:00:00+00:00"),
            ]),
            now,
        );

        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].dataset, "eco2mix-national-tr");
        assert_eq!(snapshot[0].limit, 50_000);
        assert_eq!(snapshot[0].remaining, 49_977);
        let expected_reset = OffsetDateTime::parse("2026-10-01T00:00:00+00:00", &Rfc3339)
            .unwrap()
            .unix_timestamp();
        assert_eq!(snapshot[0].reset_unix, Some(expected_reset));
        assert_eq!(snapshot[0].observed_unix, 1_800_000_000);
    }

    #[test]
    fn observes_without_reset_header() {
        let tracker = QuotaTracker::new();
        tracker.observe(
            "eco2mix-regional-tr",
            &headers(&[
                ("x-ratelimit-dataset-limit", "50000"),
                ("x-ratelimit-dataset-remaining", "22422"),
            ]),
            OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap(),
        );

        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].reset_unix, None);
        assert_eq!(snapshot[0].remaining, 22_422);
    }

    #[test]
    fn unreadable_reset_still_records_limit_and_remaining() {
        let tracker = QuotaTracker::new();
        tracker.observe(
            "eco2mix-national-tr",
            &headers(&[
                ("x-ratelimit-dataset-limit", "50000"),
                ("x-ratelimit-dataset-remaining", "1"),
                ("x-ratelimit-dataset-reset", "pas une date"),
            ]),
            OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap(),
        );

        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].reset_unix, None);
        assert_eq!(snapshot[0].limit, 50_000);
        assert_eq!(snapshot[0].remaining, 1);
    }

    #[test]
    fn missing_limit_records_nothing() {
        let tracker = QuotaTracker::new();
        tracker.observe(
            "eco2mix-national-tr",
            &headers(&[("x-ratelimit-dataset-remaining", "49977")]),
            OffsetDateTime::now_utc(),
        );

        assert!(tracker.snapshot().is_empty());
    }

    #[test]
    fn non_numeric_value_records_nothing() {
        let tracker = QuotaTracker::new();
        tracker.observe(
            "eco2mix-national-tr",
            &headers(&[
                ("x-ratelimit-dataset-limit", "cinquante-mille"),
                ("x-ratelimit-dataset-remaining", "49977"),
            ]),
            OffsetDateTime::now_utc(),
        );

        assert!(tracker.snapshot().is_empty());
    }

    #[test]
    fn second_observation_replaces_the_first() {
        let tracker = QuotaTracker::new();
        let dataset = "eco2mix-national-tr";
        tracker.observe(
            dataset,
            &headers(&[
                ("x-ratelimit-dataset-limit", "50000"),
                ("x-ratelimit-dataset-remaining", "49977"),
            ]),
            OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap(),
        );
        tracker.observe(
            dataset,
            &headers(&[
                ("x-ratelimit-dataset-limit", "50000"),
                ("x-ratelimit-dataset-remaining", "49976"),
            ]),
            OffsetDateTime::from_unix_timestamp(1_800_000_100).unwrap(),
        );

        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].remaining, 49_976);
        assert_eq!(snapshot[0].observed_unix, 1_800_000_100);
    }

    #[test]
    fn snapshot_is_sorted_by_dataset_name() {
        let tracker = QuotaTracker::new();
        let now = OffsetDateTime::now_utc();
        tracker.observe(
            "eco2mix-regional-tr",
            &headers(&[
                ("x-ratelimit-dataset-limit", "50000"),
                ("x-ratelimit-dataset-remaining", "22422"),
            ]),
            now,
        );
        tracker.observe(
            "eco2mix-national-tr",
            &headers(&[
                ("x-ratelimit-dataset-limit", "50000"),
                ("x-ratelimit-dataset-remaining", "49977"),
            ]),
            now,
        );

        let datasets: Vec<String> = tracker.snapshot().into_iter().map(|q| q.dataset).collect();
        assert_eq!(datasets, vec!["eco2mix-national-tr", "eco2mix-regional-tr"]);
    }
}
