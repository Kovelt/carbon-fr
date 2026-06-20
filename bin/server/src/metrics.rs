//! Métriques d'exploitation, exposées au format **Prometheus** sur `/metrics`.
//!
//! Registre **fait maison, zéro dépendance** (compteurs/jauges atomiques). Les
//! métriques utiles ici sont opérationnelles — pas applicatives :
//!
//! - **fraîcheur du poller** : si `now − last_success` dépasse l'intervalle de
//!   poll, l'ingestion est en panne (donnée gelée) → alerte la plus importante ;
//! - **volume & erreurs** d'ingestion ;
//! - **appels amont** par source : proxy du **quota ODRÉ** (50 000/mois) et des
//!   autres API, sans rien stocker de plus.
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
        ] {
            let _ = writeln!(
                out,
                "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}"
            );
        }

        out
    }
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
        // Chaque métrique a son TYPE.
        assert_eq!(out.matches("# TYPE ").count(), 8);
    }
}
