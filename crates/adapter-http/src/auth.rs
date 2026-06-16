//! Middleware de **bord** : authentification par clé API + application de quota
//! (ADR-0015).
//!
//! Préoccupation d'**infrastructure entrante**, jamais du domaine : le `core` ne
//! prend aucun principal, les cas d'usage sont inchangés. Le middleware résout un
//! **principal** (anonyme par IP, ou clé authentifiée via [`ApiKeyRepository`]),
//! applique un quota par fenêtre, et laisse passer la requête telle quelle.
//!
//! **Opt-in** : il n'est appliqué que si la composition root le câble. Par défaut
//! (self-hosting), l'API reste **anonyme et sans limite** — parité OSS (§6).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Response};
use carbonfr_core::ports::{ApiKeyRepository, ApiTier};
use sha2::{Digest, Sha256};

use crate::error::ErrorBody;

/// Limites de débit par niveau (requêtes par minute), ADR-0015 §5.
#[derive(Debug, Clone, Copy)]
pub struct AuthConfig {
    /// Limite des appelants **anonymes** (par IP).
    pub anonymous_per_min: u32,
    /// Limite des appelants à **clé gratuite**.
    pub free_per_min: u32,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            anonymous_per_min: 60,
            free_per_min: 600,
        }
    }
}

/// Appelant résolu. L'**anonyme** n'est pas un tier : c'est l'absence de clé.
enum Principal {
    Anonymous { ip: String },
    Keyed { id: String, tier: ApiTier },
}

/// État du middleware : registre de clés + compteur de quota en mémoire.
#[derive(Clone)]
pub struct AuthState {
    keys: Arc<dyn ApiKeyRepository>,
    limiter: Arc<Mutex<RateLimiter>>,
    config: AuthConfig,
}

impl AuthState {
    pub fn new(keys: Arc<dyn ApiKeyRepository>, config: AuthConfig) -> Self {
        Self {
            keys,
            limiter: Arc::new(Mutex::new(RateLimiter::default())),
            config,
        }
    }
}

/// Compteur de quota **fenêtre fixe par minute**, en mémoire (suffisant pour une
/// instance unique, ADR-0007 ; réversible derrière un futur `UsageMeter`).
#[derive(Default)]
struct RateLimiter {
    windows: HashMap<String, (i64, u32)>,
}

impl RateLimiter {
    /// Incrémente le compteur de `id` pour `minute`. `Some(restant)` si sous la
    /// limite, `None` si dépassée.
    fn check(&mut self, id: &str, limit: u32, minute: i64) -> Option<u32> {
        // Purge légère : si la carte grossit, on ne garde que la minute courante.
        if self.windows.len() > 10_000 {
            self.windows.retain(|_, (m, _)| *m == minute);
        }
        let entry = self.windows.entry(id.to_string()).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 0);
        }
        if entry.1 >= limit {
            return None;
        }
        entry.1 += 1;
        Some(limit - entry.1)
    }
}

/// Empreinte SHA-256 hex d'une clé (jamais stockée/loguée en clair).
fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Jeton `Authorization: Bearer …`, le cas échéant.
fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// IP client (en-têtes du reverse proxy), `unknown` à défaut.
fn client_ip(headers: &HeaderMap) -> String {
    for name in ["x-forwarded-for", "x-real-ip"] {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok()) {
            let first = value.split(',').next().unwrap_or("").trim();
            if !first.is_empty() {
                return first.to_string();
            }
        }
    }
    "unknown".to_string()
}

fn limit_for(tier: ApiTier, config: &AuthConfig) -> u32 {
    match tier {
        ApiTier::Free => config.free_per_min,
    }
}

fn current_minute() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp() / 60
}

fn error_response(status: StatusCode, code: &'static str, message: &str) -> Response {
    (status, Json(ErrorBody::new(code, message.to_string()))).into_response()
}

/// Middleware d'authentification + quota. À appliquer via
/// `axum::middleware::from_fn_with_state` au routeur protégé.
pub async fn enforce(State(state): State<AuthState>, request: Request, next: Next) -> Response {
    let headers = request.headers();

    // 1. Résolution du principal.
    let principal = match bearer_token(headers) {
        Some(token) => {
            let hash = hash_key(&token);
            match state.keys.resolve(&hash).await {
                Ok(Some(record)) => Principal::Keyed {
                    id: hash,
                    tier: record.tier,
                },
                Ok(None) => {
                    return error_response(
                        StatusCode::UNAUTHORIZED,
                        "unauthorized",
                        "clé API inconnue",
                    );
                }
                Err(_) => {
                    return error_response(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "unavailable",
                        "vérification de la clé impossible",
                    );
                }
            }
        }
        None => Principal::Anonymous {
            ip: client_ip(headers),
        },
    };

    // 2. Application du quota (fenêtre minute).
    let (limit, id) = match &principal {
        Principal::Anonymous { ip } => (state.config.anonymous_per_min, format!("ip:{ip}")),
        Principal::Keyed { id, tier } => (limit_for(*tier, &state.config), format!("key:{id}")),
    };
    let minute = current_minute();
    let remaining = state
        .limiter
        .lock()
        .expect("limiter")
        .check(&id, limit, minute);

    match remaining {
        Some(remaining) => {
            let mut response = next.run(request).await;
            let h = response.headers_mut();
            h.insert("ratelimit-limit", limit.into());
            h.insert("ratelimit-remaining", remaining.into());
            response
        }
        None => {
            let mut response = error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "quota dépassé",
            );
            let h = response.headers_mut();
            h.insert("ratelimit-limit", limit.into());
            h.insert("ratelimit-remaining", 0.into());
            // Réessai au début de la minute suivante.
            let retry = 60 - (time::OffsetDateTime::now_utc().unix_timestamp() % 60);
            if let Ok(v) = retry.to_string().parse() {
                h.insert(header::RETRY_AFTER, v);
            }
            response
        }
    }
}

/// Empreinte d'une clé en clair, pour la délivrance (`mint-key`).
pub fn key_fingerprint(key: &str) -> String {
    hash_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_blocks_after_limit_then_resets_next_minute() {
        let mut rl = RateLimiter::default();
        assert_eq!(rl.check("a", 2, 100), Some(1));
        assert_eq!(rl.check("a", 2, 100), Some(0));
        assert_eq!(rl.check("a", 2, 100), None); // dépassé
        // Minute suivante : remis à zéro.
        assert_eq!(rl.check("a", 2, 101), Some(1));
        // Autre principal : compteur indépendant.
        assert_eq!(rl.check("b", 2, 101), Some(1));
    }

    #[test]
    fn bearer_token_parsing() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, "Bearer abc123".parse().unwrap());
        assert_eq!(bearer_token(&h), Some("abc123".to_string()));
        let mut empty = HeaderMap::new();
        empty.insert(header::AUTHORIZATION, "Basic xyz".parse().unwrap());
        assert_eq!(bearer_token(&empty), None);
    }
}
