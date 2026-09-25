//! Erreurs typées du SDK (`CarbonFrError`) et corps **Problem Details**
//! (RFC 9457, ADR-0021) renvoyé par l'API sur toute réponse en erreur.
//!
//! `CarbonFrError` et [`ProblemDetails`] sont `#[non_exhaustive]` — premier
//! usage de cet attribut dans tout le dépôt (ADR-0031 décision 6) : un ajout
//! de variante/champ dans une version mineure ultérieure ne doit jamais casser
//! un `match` externe.

use std::time::Duration;

use serde::Deserialize;

/// Corps **Problem Details** (RFC 9457) porté par [`CarbonFrError::Api`].
///
/// Champs privés + accesseurs (plutôt que publics comme les DTO de `core`,
/// ADR-0030 §3) : `#[non_exhaustive]` sur une struct à champs publics
/// interdirait la construction par littéral qu'il protège, donc les deux
/// doctrines ne se combinent pas — ici on choisit l'ouverture par accesseurs.
/// `code` est l'ancrage machine **stable** à matcher en priorité (`detail` est
/// un message humain, jamais stable) — voir [`CarbonFrError::code`].
#[derive(Debug, Clone, Default, Deserialize)]
#[non_exhaustive]
pub struct ProblemDetails {
    #[serde(rename = "type", default)]
    problem_type: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

impl ProblemDetails {
    /// URI du type de problème (RFC 9457 §4.2.1) — `about:blank` aujourd'hui
    /// sur toute l'API (`status` + `code` suffisent).
    pub fn problem_type(&self) -> Option<&str> {
        self.problem_type.as_deref()
    }

    /// Résumé court, stable par [`code`](Self::code).
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Code de statut HTTP, répété dans le corps (RFC 9457 §3.1.2).
    pub fn status(&self) -> Option<u16> {
        self.status
    }

    /// Message lisible spécifique à l'occurrence — **jamais** un ancrage
    /// machine (utiliser [`code`](Self::code) pour ça).
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Extension carbon-fr : ancrage machine **stable** (`no_data`,
    /// `bad_request`, `unauthorized`, `unavailable`, `internal`,
    /// `rate_limited`…), catalogue extensible côté serveur.
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    /// Construit un [`ProblemDetails`] de repli quand le corps de la réponse
    /// n'est pas un JSON `application/problem+json` exploitable (panne en
    /// amont de l'API, proxy, corps vide…) : `detail` porte alors le corps
    /// brut (tronqué), pour ne rien perdre de diagnostiquable.
    pub(crate) fn fallback(status: u16, raw_body: &str) -> Self {
        const MAX_LEN: usize = 500;
        let detail = if raw_body.is_empty() {
            None
        } else {
            Some(raw_body.chars().take(MAX_LEN).collect())
        };
        Self {
            problem_type: None,
            title: None,
            status: Some(status),
            detail,
            code: None,
        }
    }
}

impl std::fmt::Display for ProblemDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = self
            .detail
            .as_deref()
            .or(self.title.as_deref())
            .unwrap_or("erreur sans détail");
        f.write_str(message)
    }
}

/// Erreur du SDK carbon-fr.
///
/// `#[non_exhaustive]` (ADR-0031 décision 6) : toujours terminer un `match`
/// externe par un bras `_` (ou `..` sur les variantes à champs).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CarbonFrError {
    /// Échec de transport (connexion, TLS, DNS, timeout REST — cf.
    /// [`CarbonFrBuilder::timeout`](crate::CarbonFrBuilder::timeout)…). La clé
    /// API circule **uniquement** en en-tête `Authorization` (jamais dans
    /// l'URL/la query string, cf. `CarbonFr::request`) : le message affiché
    /// par `reqwest::Error` (qui peut inclure l'URL de la requête) ne
    /// contient donc jamais de secret.
    #[error("erreur de transport : {0}")]
    Transport(#[from] reqwest::Error),

    /// Réponse en erreur de l'API (RFC 9457, ADR-0021). `status` répète le
    /// code HTTP ; `problem.code()` est l'ancrage machine à privilégier
    /// (raccourci : [`CarbonFrError::code`]).
    #[error("erreur API carbon-fr ({status}) : {problem}")]
    #[non_exhaustive]
    Api {
        status: u16,
        problem: ProblemDetails,
    },

    /// Échec de désérialisation d'un corps de réponse JSON, ou d'un flux SSE
    /// mal formé (trame hors grammaire WHATWG). Message auto-contenu
    /// (`String`) plutôt qu'un type tiers (`serde_json::Error` /
    /// `eventsource_stream::EventStreamError`) : évite d'exposer un type hors
    /// de notre contrôle dans une variante publique `#[non_exhaustive]`.
    #[error("échec de décodage de la réponse : {0}")]
    Decode(String),

    /// Configuration du client invalide (URL de base, provider TLS…) —
    /// [`CarbonFrBuilder::build`](crate::CarbonFrBuilder::build) échoue
    /// toujours par `Result`, jamais par panique ni repli silencieux sur un
    /// client par défaut.
    #[error("configuration du client invalide : {0}")]
    Config(String),

    /// Le délai de connexion (`CarbonFrBuilder::connect_timeout`) ou, pour le
    /// flux SSE, le délai de lecture entre deux messages
    /// (`StreamConfig::idle_timeout`) a été dépassé. Sur le flux, avec
    /// reconnexion activée (par défaut), cette erreur ne sort jamais du
    /// flux : elle déclenche une reconnexion silencieuse — voir
    /// `IntensityStream`.
    #[error("délai dépassé ({0:?})")]
    Timeout(Duration),
}

impl CarbonFrError {
    /// Ancrage machine stable (RFC 9457 `code`) si cette erreur est une
    /// [`CarbonFrError::Api`] — `None` pour toute autre variante. À matcher
    /// en priorité sur `detail`/le message d'affichage (ADR-0031 décision 6).
    pub fn code(&self) -> Option<&str> {
        match self {
            CarbonFrError::Api { problem, .. } => problem.code(),
            _ => None,
        }
    }

    /// Code de statut HTTP si cette erreur est une [`CarbonFrError::Api`].
    pub fn status(&self) -> Option<u16> {
        match self {
            CarbonFrError::Api { status, .. } => Some(*status),
            _ => None,
        }
    }
}
