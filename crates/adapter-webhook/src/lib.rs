//! Adapter sortant : **livraison de webhooks signés** (`Notifier`, ADR-0016).
//!
//! La **seule** frontière par laquelle `carbon-fr` émet une requête sortante. La
//! sécurité y est triple :
//! - validation **anti-SSRF** de l'URL (schéma + deny-list, [`validate_webhook_url`])
//!   à l'inscription **et** avant chaque livraison (littéraux IP, userinfo, port) ;
//! - **resolver DNS custom** ([`PublicOnlyResolver`]) **interne à reqwest** : la
//!   résolution qui décide l'IP contactée est **la même** qui la valide → pas de
//!   fenêtre TOCTOU / DNS rebinding (contrairement à un check séparé suivi d'une
//!   re-résolution par le client) ;
//! - **aucune redirection** suivie (une redirection rouvrirait la faille SSRF).
//!
//! Livraison **best-effort fiable** : timeouts courts + retries à *backoff*
//! exponentiel borné. La signature HMAC est calculée en amont (domaine).

use std::net::SocketAddr;
use std::time::Duration;

use async_trait::async_trait;
use carbonfr_core::domain::{is_public_ip, validate_webhook_url};
use carbonfr_core::ports::{Notifier, SourceError, WebhookDelivery};

/// Nombre maximal de tentatives de livraison.
const MAX_ATTEMPTS: u32 = 3;
/// Délai d'une requête (connexion + réponse).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Délai d'établissement de connexion.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Resolver DNS qui **n'autorise que des IP publiquement routables**.
///
/// Branché dans reqwest via `dns_resolver`, il filtre **au moment où reqwest
/// résout réellement l'hôte** : l'IP que le client va contacter est exactement
/// celle qui a passé le filtre — il n'y a donc pas de fenêtre TOCTOU. Si toutes
/// les IP résolues sont privées/loopback/link-local/réservées, la résolution
/// échoue et aucune connexion n'est ouverte.
struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            // Résolution système (port factice 0 : seule l'IP nous intéresse).
            let addrs = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let public: Vec<SocketAddr> = addrs.filter(|a| is_public_ip(a.ip())).collect();
            if public.is_empty() {
                let err: Box<dyn std::error::Error + Send + Sync> =
                    "l'hôte ne résout vers aucune IP publique (anti-SSRF)".into();
                return Err(err);
            }
            let iter: Box<dyn Iterator<Item = SocketAddr> + Send> = Box::new(public.into_iter());
            Ok(iter)
        })
    }
}

/// `Notifier` HTTP : POST signé vers l'URL de rappel, avec garde SSRF.
#[derive(Clone)]
pub struct HttpNotifier {
    client: reqwest::Client,
}

impl HttpNotifier {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            // Redirections refusées : les suivre rouvrirait une faille SSRF
            // (redirection vers une IP interne).
            .redirect(reqwest::redirect::Policy::none())
            // Ignore HTTPS_PROXY/ALL_PROXY de l'environnement : un proxy interne
            // contournerait le filtre d'IP du resolver.
            .no_proxy()
            // Filtre d'IP publiques appliqué **dans** la pile de résolution reqwest.
            .dns_resolver(std::sync::Arc::new(PublicOnlyResolver))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Garde structurelle avant émission : schéma HTTPS, pas d'userinfo, et —
    /// pour un hôte **littéral IP** — refus des plages non publiques (le resolver
    /// ne s'applique qu'aux **noms** d'hôte ; reqwest connecte un littéral IP sans
    /// résoudre). La défense DNS (noms) est portée par [`PublicOnlyResolver`].
    fn guard_ssrf(&self, url: &str) -> Result<(), SourceError> {
        validate_webhook_url(url).map_err(|e| SourceError::Invalid(e.to_string()))
    }
}

impl Default for HttpNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Notifier for HttpNotifier {
    async fn deliver(&self, delivery: &WebhookDelivery) -> Result<(), SourceError> {
        self.guard_ssrf(&delivery.url)?;

        let mut last_err = SourceError::Unavailable("aucune tentative".into());
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                // Backoff exponentiel borné : 0,5 s, 1 s, 2 s…
                let backoff = Duration::from_millis(500 * (1u64 << (attempt - 1)));
                tokio::time::sleep(backoff).await;
            }
            let result = self
                .client
                .post(&delivery.url)
                .header("content-type", "application/json")
                .header(
                    "x-carbonfr-signature",
                    format!("sha256={}", delivery.signature),
                )
                .body(delivery.body.clone())
                .send()
                .await;
            match result {
                Ok(resp) if resp.status().is_success() => return Ok(()),
                Ok(resp) => {
                    last_err = SourceError::Unavailable(format!("statut {}", resp.status()))
                }
                Err(e) => last_err = SourceError::Unavailable(e.to_string()),
            }
        }
        Err(last_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_forbidden_url_before_any_request() {
        let notifier = HttpNotifier::new();
        let delivery = WebhookDelivery {
            url: "https://127.0.0.1/hook".to_string(),
            body: "{}".to_string(),
            signature: "x".to_string(),
        };
        let err = notifier.deliver(&delivery).await.unwrap_err();
        assert!(matches!(err, SourceError::Invalid(_)));
    }

    #[tokio::test]
    async fn rejects_non_https() {
        let notifier = HttpNotifier::new();
        let delivery = WebhookDelivery {
            url: "http://example.com/hook".to_string(),
            body: "{}".to_string(),
            signature: "x".to_string(),
        };
        assert!(matches!(
            notifier.deliver(&delivery).await.unwrap_err(),
            SourceError::Invalid(_)
        ));
    }
}
