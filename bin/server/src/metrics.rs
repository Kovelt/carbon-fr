//! Métriques d'exploitation, exposées au format **Prometheus** sur `/metrics`.
//!
//! Registre **fait maison, zéro dépendance** (compteurs/jauges atomiques). Les
//! métriques utiles ici sont opérationnelles — pas applicatives :
//!
//! - **fraîcheur du poller** : si `now − last_success` dépasse l'intervalle de
//!   poll, l'ingestion est en panne (donnée gelée) → alerte la plus importante ;
//! - **volume & erreurs** d'ingestion ;
//! - **appels amont** par source : `carbonfr_upstream_requests_total`, un
//!   **proxy** (appels initiés) pour ODRÉ et les autres API amont ;
//! - **quota ODRÉ réel** (ADR-0022 addendum 2026-09-26) : depuis
//!   `carbonfr-adapter-odre` (module `quota`), les en-têtes de quota renvoyés
//!   par ODRÉ lui-même (par jeu de données, 50 000/mois) — [`render_odre_quota`],
//!   séparée de [`Metrics`] pour garder ce module indépendant des types de
//!   l'adapter (la composition root fait la conversion). Le proxy ci-dessus
//!   reste la seule visibilité pour Open-Meteo/ENTSO-E, qui n'exposent pas ce
//!   genre d'en-tête.
//!
//! La latence HTTP est déjà tracée par `TraceLayer` ; on n'embarque donc aucun
//! histogramme (ni crate de métriques) — cohérent avec l'ethos zéro-dépendance
//! du projet (compteur de visiteurs, primitives de scheduling, déjà faits main).

use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Registre de métriques partagé (clone bon marché : `Arc`). Le poller écrit,
/// le handler `/metrics` lit.
#[derive(Clone)]
pub struct Metrics {
    inner: Arc<Inner>,
}

struct Inner {
    version: &'static str,
    ingest_written: AtomicU64,
    ingest_errors: AtomicU64,
    poll_cycles: AtomicU64,
    upstream_odre: AtomicU64,
    upstream_open_meteo: AtomicU64,
    upstream_entsoe: AtomicU64,
    /// Horodatage Unix (s) du dernier cycle ayant écrit ≥ 1 ligne (0 si jamais).
    last_success_unix: AtomicI64,
    /// Horodatage Unix (s) de la dernière mesure nationale connue (0 si jamais).
    last_measurement_unix: AtomicI64,
    /// Horodatage Unix (s) de la dernière ingestion de prix spot (0 si jamais).
    last_price_unix: AtomicI64,
    /// Horodatage Unix (s) de la dernière ingestion du contexte d'import
    /// transfrontalier (0 si jamais).
    last_flows_unix: AtomicI64,
}

impl Metrics {
    pub fn new(version: &'static str) -> Self {
        Self {
            inner: Arc::new(Inner {
                version,
                ingest_written: AtomicU64::new(0),
                ingest_errors: AtomicU64::new(0),
                poll_cycles: AtomicU64::new(0),
                upstream_odre: AtomicU64::new(0),
                upstream_open_meteo: AtomicU64::new(0),
                upstream_entsoe: AtomicU64::new(0),
                last_success_unix: AtomicI64::new(0),
                last_measurement_unix: AtomicI64::new(0),
                last_price_unix: AtomicI64::new(0),
                last_flows_unix: AtomicI64::new(0),
            }),
        }
    }

    pub fn add_written(&self, n: usize) {
        self.inner
            .ingest_written
            .fetch_add(n as u64, Ordering::Relaxed);
    }

    pub fn inc_error(&self) {
        self.inner.ingest_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_cycle(&self) {
        self.inner.poll_cycles.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_upstream_odre(&self, n: u64) {
        self.inner.upstream_odre.fetch_add(n, Ordering::Relaxed);
    }

    pub fn inc_upstream_open_meteo(&self) {
        self.inner
            .upstream_open_meteo
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_upstream_entsoe(&self) {
        self.inner.upstream_entsoe.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_last_success(&self, unix_secs: i64) {
        self.inner
            .last_success_unix
            .store(unix_secs, Ordering::Relaxed);
    }

    pub fn set_last_measurement(&self, unix_secs: i64) {
        self.inner
            .last_measurement_unix
            .store(unix_secs, Ordering::Relaxed);
    }

    pub fn set_last_price(&self, unix_secs: i64) {
        self.inner
            .last_price_unix
            .store(unix_secs, Ordering::Relaxed);
    }

    pub fn set_last_flows(&self, unix_secs: i64) {
        self.inner
            .last_flows_unix
            .store(unix_secs, Ordering::Relaxed);
    }

    /// Rend l'exposition Prometheus (text format 0.0.4).
    pub fn render(&self) -> String {
        let i = &self.inner;
        let mut out = String::with_capacity(1024);

        // build_info : la version vit dans un label, la valeur est toujours 1
        // (convention Prometheus pour exposer des métadonnées).
        let _ = writeln!(
            out,
            "# HELP carbonfr_build_info Version du binaire (valeur constante 1).\n\
             # TYPE carbonfr_build_info gauge\n\
             carbonfr_build_info{{version=\"{}\"}} 1",
            i.version
        );

        for (name, help, value) in [
            (
                "carbonfr_poller_cycles_total",
                "Cycles de poll terminés.",
                i.poll_cycles.load(Ordering::Relaxed),
            ),
            (
                "carbonfr_poller_ingest_written_total",
                "Lignes de mesure écrites par le poller.",
                i.ingest_written.load(Ordering::Relaxed),
            ),
            (
                "carbonfr_poller_ingest_errors_total",
                "Échecs d'ingestion (comptés par région).",
                i.ingest_errors.load(Ordering::Relaxed),
            ),
        ] {
            let _ = writeln!(
                out,
                "# HELP {name} {help}\n# TYPE {name} counter\n{name} {value}"
            );
        }

        // Appels amont : une seule métrique, discriminée par le label `source`.
        let _ = writeln!(
            out,
            "# HELP carbonfr_upstream_requests_total Appels initiés vers une source amont (proxy de quota).\n\
             # TYPE carbonfr_upstream_requests_total counter\n\
             carbonfr_upstream_requests_total{{source=\"odre\"}} {}\n\
             carbonfr_upstream_requests_total{{source=\"open-meteo\"}} {}\n\
             carbonfr_upstream_requests_total{{source=\"entsoe\"}} {}",
            i.upstream_odre.load(Ordering::Relaxed),
            i.upstream_open_meteo.load(Ordering::Relaxed),
            i.upstream_entsoe.load(Ordering::Relaxed),
        );

        for (name, help, value) in [
            (
                "carbonfr_poller_last_success_timestamp_seconds",
                "Horodatage Unix du dernier cycle ayant écrit ≥ 1 ligne (0 si jamais).",
                i.last_success_unix.load(Ordering::Relaxed),
            ),
            (
                "carbonfr_poller_last_measurement_timestamp_seconds",
                "Horodatage Unix de la dernière mesure nationale connue (0 si jamais).",
                i.last_measurement_unix.load(Ordering::Relaxed),
            ),
            (
                "carbonfr_poller_last_price_timestamp_seconds",
                "Horodatage Unix de la dernière ingestion de prix spot (0 si jamais).",
                i.last_price_unix.load(Ordering::Relaxed),
            ),
            (
                "carbonfr_poller_last_flows_timestamp_seconds",
                "Horodatage Unix de la dernière ingestion du contexte d'import transfrontalier (0 si jamais).",
                i.last_flows_unix.load(Ordering::Relaxed),
            ),
        ] {
            let _ = writeln!(
                out,
                "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}"
            );
        }

        out
    }
}

/// Une observation de quota ODRÉ à rendre en jauges (miroir découplé de
/// `carbonfr_adapter_odre::quota::DatasetQuota` — ce module ne dépend d'aucun
/// adapter, cf. doc de tête).
pub struct QuotaGauge<'a> {
    pub dataset: &'a str,
    pub limit: u64,
    pub remaining: u64,
    pub reset_unix: Option<i64>,
    pub observed_unix: i64,
}

/// Rend les jauges du **quota ODRÉ réel** (ADR-0022 addendum 2026-09-26), une
/// par jeu de données observé. Chaînes vide si `entries` est vide (rien à
/// exposer avant la première observation). Chaque métrique a son
/// `# HELP`/`# TYPE` une seule fois, puis une ligne par jeu ; `reset_unix`
/// absent → la ligne `..._reset_timestamp_seconds` de ce jeu est omise (pas de
/// `0` trompeur : `0` est un horodatage Unix valide, contrairement aux jauges
/// de fraîcheur de [`Metrics`] où `0` signifie « jamais »).
pub fn render_odre_quota(entries: &[QuotaGauge<'_>]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(256 * entries.len());

    let _ = writeln!(
        out,
        "# HELP carbonfr_odre_quota_limit Plafond mensuel du quota ODRÉ par jeu de données (x-ratelimit-dataset-limit).\n\
         # TYPE carbonfr_odre_quota_limit gauge"
    );
    for e in entries {
        let _ = writeln!(
            out,
            "carbonfr_odre_quota_limit{{dataset=\"{}\"}} {}",
            escape_label(e.dataset),
            e.limit
        );
    }

    let _ = writeln!(
        out,
        "# HELP carbonfr_odre_quota_remaining Appels restants avant remise à zéro du quota ODRÉ par jeu de données (x-ratelimit-dataset-remaining).\n\
         # TYPE carbonfr_odre_quota_remaining gauge"
    );
    for e in entries {
        let _ = writeln!(
            out,
            "carbonfr_odre_quota_remaining{{dataset=\"{}\"}} {}",
            escape_label(e.dataset),
            e.remaining
        );
    }

    let with_reset: Vec<(&str, i64)> = entries
        .iter()
        .filter_map(|e| e.reset_unix.map(|reset| (e.dataset, reset)))
        .collect();
    if !with_reset.is_empty() {
        let _ = writeln!(
            out,
            "# HELP carbonfr_odre_quota_reset_timestamp_seconds Horodatage Unix de la prochaine remise à zéro du quota ODRÉ par jeu de données (x-ratelimit-dataset-reset).\n\
             # TYPE carbonfr_odre_quota_reset_timestamp_seconds gauge"
        );
        for (dataset, reset) in with_reset {
            let _ = writeln!(
                out,
                "carbonfr_odre_quota_reset_timestamp_seconds{{dataset=\"{}\"}} {reset}",
                escape_label(dataset),
            );
        }
    }

    let _ = writeln!(
        out,
        "# HELP carbonfr_odre_quota_observed_timestamp_seconds Horodatage Unix de la dernière observation du quota ODRÉ par jeu de données.\n\
         # TYPE carbonfr_odre_quota_observed_timestamp_seconds gauge"
    );
    for e in entries {
        let _ = writeln!(
            out,
            "carbonfr_odre_quota_observed_timestamp_seconds{{dataset=\"{}\"}} {}",
            escape_label(e.dataset),
            e.observed_unix
        );
    }

    out
}

/// Échappe `\`, `"` et le saut de ligne dans une valeur de label Prometheus,
/// comme l'exige le format texte 0.0.4. En pratique, `dataset` est toujours une
/// constante littérale choisie par notre propre code (`NATIONAL_DATASET`,
/// `REGIONAL_DATASET`… dans `carbonfr-adapter-odre`), jamais une valeur lue
/// dans la réponse HTTP d'ODRÉ — mais échapper reste bon marché et évite une
/// dépendance implicite à cette invariante si l'origine change un jour.
fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_reflects_counters_and_gauges() {
        let m = Metrics::new("9.9.9");
        m.add_written(42);
        m.add_written(8);
        m.inc_error();
        m.inc_cycle();
        m.add_upstream_odre(13);
        m.inc_upstream_open_meteo();
        m.set_last_success(1_718_000_000);
        m.set_last_measurement(1_718_000_900);
        m.set_last_price(1_718_001_800);
        m.set_last_flows(1_718_002_700);

        let out = m.render();

        // build_info : version en label, valeur 1.
        assert!(out.contains("carbonfr_build_info{version=\"9.9.9\"} 1"));
        // Compteurs cumulés.
        assert!(out.contains("carbonfr_poller_ingest_written_total 50"));
        assert!(out.contains("carbonfr_poller_ingest_errors_total 1"));
        assert!(out.contains("carbonfr_poller_cycles_total 1"));
        // Appels amont par label.
        assert!(out.contains("carbonfr_upstream_requests_total{source=\"odre\"} 13"));
        assert!(out.contains("carbonfr_upstream_requests_total{source=\"open-meteo\"} 1"));
        assert!(out.contains("carbonfr_upstream_requests_total{source=\"entsoe\"} 0"));
        // Jauges de fraîcheur.
        assert!(out.contains("carbonfr_poller_last_success_timestamp_seconds 1718000000"));
        assert!(out.contains("carbonfr_poller_last_measurement_timestamp_seconds 1718000900"));
        assert!(out.contains("carbonfr_poller_last_price_timestamp_seconds 1718001800"));
        assert!(out.contains("carbonfr_poller_last_flows_timestamp_seconds 1718002700"));
        // Chaque métrique a son TYPE.
        assert_eq!(out.matches("# TYPE ").count(), 9);
    }

    #[test]
    fn render_odre_quota_empty_is_empty_string() {
        assert_eq!(render_odre_quota(&[]), "");
    }

    #[test]
    fn render_odre_quota_renders_one_line_per_dataset() {
        let entries = [
            QuotaGauge {
                dataset: "eco2mix-national-tr",
                limit: 50_000,
                remaining: 49_977,
                reset_unix: Some(1_790_000_000),
                observed_unix: 1_789_000_000,
            },
            QuotaGauge {
                dataset: "eco2mix-regional-tr",
                limit: 50_000,
                remaining: 22_422,
                reset_unix: None,
                observed_unix: 1_789_000_100,
            },
        ];

        let out = render_odre_quota(&entries);

        assert!(out.contains("carbonfr_odre_quota_limit{dataset=\"eco2mix-national-tr\"} 50000"));
        assert!(out.contains("carbonfr_odre_quota_limit{dataset=\"eco2mix-regional-tr\"} 50000"));
        assert!(
            out.contains("carbonfr_odre_quota_remaining{dataset=\"eco2mix-national-tr\"} 49977")
        );
        assert!(
            out.contains("carbonfr_odre_quota_remaining{dataset=\"eco2mix-regional-tr\"} 22422")
        );
        // Reset : seul le jeu qui en a un est rendu (pas de ligne pour l'autre).
        assert!(out.contains(
            "carbonfr_odre_quota_reset_timestamp_seconds{dataset=\"eco2mix-national-tr\"} 1790000000"
        ));
        assert!(!out.contains(
            "carbonfr_odre_quota_reset_timestamp_seconds{dataset=\"eco2mix-regional-tr\"}"
        ));
        assert!(out.contains(
            "carbonfr_odre_quota_observed_timestamp_seconds{dataset=\"eco2mix-national-tr\"} 1789000000"
        ));
        assert!(out.contains(
            "carbonfr_odre_quota_observed_timestamp_seconds{dataset=\"eco2mix-regional-tr\"} 1789000100"
        ));
        // 4 métriques déclarées (limit, remaining, reset, observed).
        assert_eq!(out.matches("# TYPE ").count(), 4);
    }

    #[test]
    fn render_odre_quota_escapes_label_value() {
        let entries = [QuotaGauge {
            dataset: "weird\"dataset\\name",
            limit: 1,
            remaining: 1,
            reset_unix: None,
            observed_unix: 0,
        }];

        let out = render_odre_quota(&entries);

        assert!(out.contains("dataset=\"weird\\\"dataset\\\\name\""));
    }
}
