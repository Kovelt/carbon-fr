//! Adapter sortant : **livraison de webhooks signés** (`Notifier`, ADR-0016).
//!
//! La **seule** frontière par laquelle `carbon-fr` émet une requête sortante. La
//! sécurité y est doublée :
//! - validation **anti-SSRF** de l'URL (schéma + deny-list, [`validate_webhook_url`]),
//! - **re-résolution DNS** de l'hôte à la livraison et refus si une IP résolue
//!   n'est pas publique (parade TOCTOU / DNS rebinding, ADR-0016 §3).
//!
//! Livraison **best-effort fiable** : timeout court + retries à *backoff*
//! exponentiel borné. La signature HMAC est calculée en amont (domaine) et
//! transmise dans l'en-tête `X-Carbonfr-Signature`.

use std::time::Duration;

use async_trait::async_trait;
use carbonfr_core::domain::{is_public_ip, validate_webhook_url, webhook_host};
use carbonfr_core::ports::{Notifier, SourceError, WebhookDelivery};

/// Nombre maximal de tentatives de livraison.
const MAX_ATTEMPTS: u32 = 3;
/// Délai d'une requête (connexion + réponse).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// `Notifier` HTTP : POST signé vers l'URL de rappel, avec garde SSRF.
#[derive(Clone)]
pub struct HttpNotifier {
    client: reqwest::Client,
}

impl HttpNotifier {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            // On gère la redirection nous-mêmes : la suivre rouvrirait une faille
            // SSRF (redirection vers une IP interne). On refuse donc les redirects.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Re-valide l'URL **et** résout l'hôte, en refusant toute IP non publique
    /// (parade TOCTOU). `Err` si l'URL est interdite ou la résolution suspecte.
    async fn guard_ssrf(&self, url: &str) -> Result<(), SourceError> {
        validate_webhook_url(url).map_err(|e| SourceError::Invalid(e.to_string()))?;
        let host =
            webhook_host(url).ok_or_else(|| SourceError::Invalid("hôte illisible".into()))?;

        // Hôte littéral IP : déjà couvert par `validate_webhook_url`. Hôte nom :
        // on résout et on vérifie chaque IP.
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Ok(());
        }
        let addrs = tokio::net::lookup_host((host.as_str(), 443u16))
            .await
            .map_err(|e| SourceError::Unavailable(format!("résolution DNS : {e}")))?;
        let mut any = false;
        for addr in addrs {
            any = true;
            if !is_public_ip(addr.ip()) {
                return Err(SourceError::Invalid(
                    "l'hôte résout vers une IP non publique (SSRF)".into(),
                ));
            }
        }
        if !any {
            return Err(SourceError::Unavailable("hôte non résolu".into()));
        }
        Ok(())
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
        self.guard_ssrf(&delivery.url).await?;

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
