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
/// Base du *backoff* exponentiel entre tentatives (0,5 s, 1 s, 2 s…).
const BACKOFF_BASE: Duration = Duration::from_millis(500);

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

/// Installe le provider crypto `ring` de rustls comme provider par défaut du
/// **processus**, si aucun n'est déjà en place.
///
/// reqwest 0.13 (feature `rustls-no-provider`, cf. Cargo.toml racine) ne tire
/// plus `aws-lc-rs` : sans provider installé, `reqwest::Client::builder().build()`
/// **panique**, y compris pour un usage HTTP en clair (la pile TLS est montée
/// dès `.build()`). Un seul provider dans tout le workspace (ADR-0031 décision
/// 3 : pas de double provider) → `ring`, déjà celui de sqlx (`tls-rustls-ring`).
/// Appelé défensivement ici (pas seulement au bootstrap de `bin/server`, cf.
/// `main.rs`) pour que cette crate reste utilisable seule — les 8 tests de ce
/// crate construisent chacun un `HttpNotifier` (`new`/`new_for_test`), donc un
/// `reqwest::Client`, sans jamais passer par `main()`. `install_default()`
/// renvoie `Err` si un provider est déjà installé (par le bootstrap du
/// binaire, ou par un appel concurrent depuis un autre adapter) : sans
/// conséquence, on l'ignore — c'est forcément le même `ring`, seul provider
/// présent dans le graphe de dépendances du workspace. Le `Once` évite juste
/// de reconstruire un `CryptoProvider` (allocation) à chaque appel.
fn ensure_crypto_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Garde SSRF de **production** : identique à l'ancien corps de
/// `HttpNotifier::guard_ssrf` avant factorisation — schéma HTTPS, pas
/// d'userinfo, et — pour un hôte **littéral IP** — refus des plages non
/// publiques (le resolver ne s'applique qu'aux **noms** d'hôte ; reqwest
/// connecte un littéral IP sans résoudre). La défense DNS (noms) est portée
/// par [`PublicOnlyResolver`].
fn guard_prod(url: &str) -> Result<(), SourceError> {
    validate_webhook_url(url).map_err(|e| SourceError::Invalid(e.to_string()))
}

/// Garde SSRF de **test uniquement** (`#[cfg(test)]`) : n'autorise que
/// `http://127.0.0.1` (en clair, avec ou sans port) — juste de quoi viser le
/// serveur HTTP local des tests, jamais une URL SSRF arbitraire. N'existe pas
/// dans le binaire de prod ; le chemin de prod utilise [`guard_prod`], inchangé.
#[cfg(test)]
fn guard_allow_local_test_server(url: &str) -> Result<(), SourceError> {
    let is_local = url == "http://127.0.0.1" || url.starts_with("http://127.0.0.1:");
    if is_local {
        Ok(())
    } else {
        Err(SourceError::Invalid(
            "URL de test non autorisée (réservé à http://127.0.0.1)".to_string(),
        ))
    }
}

/// `Notifier` HTTP : POST signé vers l'URL de rappel, avec garde SSRF.
///
/// `guard` et `backoff_base` sont les deux seuls points de variation entre le
/// chemin de production ([`HttpNotifier::new`]) et le constructeur réservé aux
/// tests ([`HttpNotifier::new_for_test`], `#[cfg(test)]`) : la construction du
/// `reqwest::Client` (timeouts, refus des redirections, `no_proxy`, resolver
/// anti-SSRF) est **partagée** par [`HttpNotifier::build`], donc identique
/// dans les deux cas — seuls les délais et la garde d'URL diffèrent.
#[derive(Clone)]
pub struct HttpNotifier {
    client: reqwest::Client,
    /// Validation de l'URL avant émission (pointeur de fonction : pas d'état
    /// capturé, donc `Clone`/`Send`/`Sync` gratuits). En prod : [`guard_prod`].
    guard: fn(&str) -> Result<(), SourceError>,
    backoff_base: Duration,
}

impl HttpNotifier {
    /// Échoue si le client HTTP ne peut pas être construit (ex. magasin de
    /// certificats système illisible) : **jamais** de repli sur un
    /// `reqwest::Client` par défaut, qui perdrait le resolver anti-SSRF, le
    /// refus des redirections et `no_proxy` (ADR-0016).
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::build(REQUEST_TIMEOUT, CONNECT_TIMEOUT, BACKOFF_BASE, guard_prod)
    }

    /// Construction partagée par [`HttpNotifier::new`] (prod) et
    /// [`HttpNotifier::new_for_test`] (tests) : mêmes garde-fous structurels
    /// (redirections refusées, pas de proxy d'environnement, resolver
    /// anti-SSRF) quels que soient les timeouts/la garde passés en paramètre.
    fn build(
        request_timeout: Duration,
        connect_timeout: Duration,
        backoff_base: Duration,
        guard: fn(&str) -> Result<(), SourceError>,
    ) -> Result<Self, reqwest::Error> {
        ensure_crypto_provider();
        let client = reqwest::Client::builder()
            .timeout(request_timeout)
            .connect_timeout(connect_timeout)
            // Redirections refusées : les suivre rouvrirait une faille SSRF
            // (redirection vers une IP interne).
            .redirect(reqwest::redirect::Policy::none())
            // Ignore HTTPS_PROXY/ALL_PROXY de l'environnement : un proxy interne
            // contournerait le filtre d'IP du resolver.
            .no_proxy()
            // Filtre d'IP publiques appliqué **dans** la pile de résolution reqwest.
            .dns_resolver(std::sync::Arc::new(PublicOnlyResolver))
            .build()?;
        Ok(Self {
            client,
            guard,
            backoff_base,
        })
    }

    /// Constructeur de **test uniquement** (`#[cfg(test)]` : jamais compilé
    /// dans le binaire de prod, ni même accessible hors de ce crate — privé).
    /// Autorise `http://127.0.0.1[:port]/…` (via [`guard_allow_local_test_server`])
    /// et des délais courts, pour cibler un serveur HTTP local dans les tests
    /// hermétiques. Le chemin de production ([`HttpNotifier::new`]) n'est pas
    /// touché : il continue d'appeler [`guard_prod`] avec les timeouts prod.
    #[cfg(test)]
    fn new_for_test(
        request_timeout: Duration,
        connect_timeout: Duration,
        backoff_base: Duration,
    ) -> Result<Self, reqwest::Error> {
        Self::build(
            request_timeout,
            connect_timeout,
            backoff_base,
            guard_allow_local_test_server,
        )
    }

    fn guard_ssrf(&self, url: &str) -> Result<(), SourceError> {
        (self.guard)(url)
    }
}

#[async_trait]
impl Notifier for HttpNotifier {
    async fn deliver(&self, delivery: &WebhookDelivery) -> Result<(), SourceError> {
        self.guard_ssrf(&delivery.url)?;

        let mut last_err = SourceError::Unavailable("aucune tentative".into());
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                // Backoff exponentiel borné à partir de `backoff_base` (0,5 s,
                // 1 s, 2 s… en prod ; raccourci par `new_for_test` en test).
                let backoff = self.backoff_base * (1u32 << (attempt - 1));
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    #[tokio::test]
    async fn rejects_forbidden_url_before_any_request() {
        let notifier = HttpNotifier::new().expect("client webhook de test");
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
        let notifier = HttpNotifier::new().expect("client webhook de test");
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

    /// Le chemin de **production** (`HttpNotifier::new`) refuse toujours de
    /// viser un serveur local en clair, même après la factorisation
    /// `build`/`guard` : la garde de schéma (`NotHttps`) tranche avant même
    /// l'examen de l'hôte (cas de l'hôte interdit en https :
    /// `rejects_forbidden_url_before_any_request`).
    #[tokio::test]
    async fn prod_rejects_plain_http_to_loopback() {
        let notifier = HttpNotifier::new().expect("client webhook de test");
        let delivery = WebhookDelivery {
            url: "http://127.0.0.1/hook".to_string(),
            body: "{}".to_string(),
            signature: "x".to_string(),
        };
        assert!(matches!(
            notifier.deliver(&delivery).await.unwrap_err(),
            SourceError::Invalid(_)
        ));
    }

    // --- Tests hermétiques avec un vrai serveur HTTP local -----------------
    //
    // Le constructeur `HttpNotifier::new_for_test` (privé, `#[cfg(test)]`)
    // autorise `http://127.0.0.1` et prend des délais courts, ce qui permet de
    // livrer réellement vers un serveur `axum` lié sur `127.0.0.1:0` (port
    // choisi par l'OS) sans jamais desserrer les garde-fous de production.

    /// Timeouts/backoff volontairement courts : garde la suite < 5 s.
    const TEST_REQUEST_TIMEOUT: Duration = Duration::from_millis(300);
    const TEST_CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
    const TEST_BACKOFF_BASE: Duration = Duration::from_millis(20);

    fn test_notifier() -> HttpNotifier {
        HttpNotifier::new_for_test(
            TEST_REQUEST_TIMEOUT,
            TEST_CONNECT_TIMEOUT,
            TEST_BACKOFF_BASE,
        )
        .expect("client webhook de test")
    }

    /// Démarre `app` sur `127.0.0.1:<port éphémère>` et rend l'URL de base
    /// (`http://127.0.0.1:<port>`) ainsi que le `JoinHandle` de la tâche
    /// serveur (abandonnée — donc annulée — à la fin de chaque test avec le
    /// runtime `#[tokio::test]`, pas besoin d'arrêt explicite).
    async fn spawn_server(app: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind du serveur de test");
        let addr = listener.local_addr().expect("adresse locale");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), handle)
    }

    #[derive(Debug, Clone, Default)]
    struct CapturedRequest {
        content_type: Option<String>,
        signature: Option<String>,
        body: Vec<u8>,
    }

    /// (1) 200 → `Ok`, et le serveur reçoit le corps + l'en-tête de signature
    /// HMAC + le content-type attendus.
    #[tokio::test]
    async fn delivers_successfully_and_server_receives_expected_request() {
        let captured: Arc<Mutex<Option<CapturedRequest>>> = Arc::new(Mutex::new(None));
        let state = captured.clone();
        let app = axum::Router::new().route(
            "/hook",
            axum::routing::post(
                move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                    let state = state.clone();
                    async move {
                        *state.lock().expect("mutex non empoisonné") = Some(CapturedRequest {
                            content_type: headers
                                .get("content-type")
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_string),
                            signature: headers
                                .get("x-carbonfr-signature")
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_string),
                            body: body.to_vec(),
                        });
                        axum::http::StatusCode::OK
                    }
                },
            ),
        );
        let (base_url, _server) = spawn_server(app).await;

        let notifier = test_notifier();
        let delivery = WebhookDelivery {
            url: format!("{base_url}/hook"),
            body: "{\"intensity\":42}".to_string(),
            signature: "deadbeef".to_string(),
        };
        notifier
            .deliver(&delivery)
            .await
            .expect("la livraison doit réussir (200)");

        let got = captured
            .lock()
            .expect("mutex non empoisonné")
            .clone()
            .expect("requête reçue");
        assert_eq!(got.content_type.as_deref(), Some("application/json"));
        assert_eq!(got.signature.as_deref(), Some("sha256=deadbeef"));
        assert_eq!(got.body, delivery.body.as_bytes());
    }

    /// (2) 500 puis 200 → `Ok` en exactement 2 tentatives.
    #[tokio::test]
    async fn retries_once_then_succeeds() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let state = attempts.clone();
        let app = axum::Router::new().route(
            "/hook",
            axum::routing::post(move || {
                let state = state.clone();
                async move {
                    let n = state.fetch_add(1, Ordering::SeqCst) + 1;
                    if n < 2 {
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR
                    } else {
                        axum::http::StatusCode::OK
                    }
                }
            }),
        );
        let (base_url, _server) = spawn_server(app).await;

        let notifier = test_notifier();
        let delivery = WebhookDelivery {
            url: format!("{base_url}/hook"),
            body: "{}".to_string(),
            signature: "x".to_string(),
        };
        notifier
            .deliver(&delivery)
            .await
            .expect("doit réussir à la 2e tentative");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    /// (3) 500 permanent → `Err` après exactement `MAX_ATTEMPTS` tentatives.
    #[tokio::test]
    async fn gives_up_after_max_attempts_on_permanent_failure() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let state = attempts.clone();
        let app = axum::Router::new().route(
            "/hook",
            axum::routing::post(move || {
                let state = state.clone();
                async move {
                    state.fetch_add(1, Ordering::SeqCst);
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                }
            }),
        );
        let (base_url, _server) = spawn_server(app).await;

        let notifier = test_notifier();
        let delivery = WebhookDelivery {
            url: format!("{base_url}/hook"),
            body: "{}".to_string(),
            signature: "x".to_string(),
        };
        let err = notifier.deliver(&delivery).await.unwrap_err();
        assert!(matches!(err, SourceError::Unavailable(_)));
        assert_eq!(attempts.load(Ordering::SeqCst), MAX_ATTEMPTS as usize);
    }

    /// (4) Le serveur ne répond pas dans le délai → `Err` (timeout), sans
    /// jamais attendre les 10 s de la config de prod (`REQUEST_TIMEOUT`).
    #[tokio::test]
    async fn times_out_without_waiting_prod_timeout() {
        let app = axum::Router::new().route(
            "/hook",
            axum::routing::post(|| async {
                // Bien plus long que `TEST_REQUEST_TIMEOUT` (300 ms) : le
                // client doit abandonner avant, jamais attendre cette fin.
                tokio::time::sleep(Duration::from_secs(60)).await;
                axum::http::StatusCode::OK
            }),
        );
        let (base_url, _server) = spawn_server(app).await;

        let notifier = test_notifier();
        let delivery = WebhookDelivery {
            url: format!("{base_url}/hook"),
            body: "{}".to_string(),
            signature: "x".to_string(),
        };
        let started = Instant::now();
        let err = notifier.deliver(&delivery).await.unwrap_err();
        assert!(matches!(err, SourceError::Unavailable(_)));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "le timeout de test n'a pas raccourci l'attente : {:?}",
            started.elapsed()
        );
    }

    /// (5) Redirection 302 : **non suivie** (`Policy::none()`). La cible de la
    /// `Location` est **joignable et observable** (même serveur, répond 200) :
    /// si la redirection était suivie, la livraison réussirait dès la 1re
    /// tentative et la cible serait contactée. Comportement constaté : le 302
    /// n'est pas un 2xx, `deliver` le traite comme un échec ordinaire — il
    /// retente `MAX_ATTEMPTS` fois puis renvoie `Err`, sans jamais contacter la
    /// cible.
    #[tokio::test]
    async fn does_not_follow_redirect() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let followed = Arc::new(AtomicUsize::new(0));
        let (hook_state, target_state) = (attempts.clone(), followed.clone());
        let target = move || {
            let target_state = target_state.clone();
            async move {
                target_state.fetch_add(1, Ordering::SeqCst);
                axum::http::StatusCode::OK
            }
        };
        let app = axum::Router::new()
            .route(
                "/hook",
                axum::routing::post(move || {
                    let hook_state = hook_state.clone();
                    async move {
                        hook_state.fetch_add(1, Ordering::SeqCst);
                        axum::response::Response::builder()
                            .status(axum::http::StatusCode::FOUND)
                            .header("location", "/elsewhere")
                            .body(axum::body::Body::empty())
                            .expect("réponse 302 valide")
                    }
                }),
            )
            .route(
                "/elsewhere",
                axum::routing::get(target.clone()).post(target),
            );
        let (base_url, _server) = spawn_server(app).await;

        let notifier = test_notifier();
        let delivery = WebhookDelivery {
            url: format!("{base_url}/hook"),
            body: "{}".to_string(),
            signature: "x".to_string(),
        };
        let err = notifier.deliver(&delivery).await.unwrap_err();
        assert!(matches!(err, SourceError::Unavailable(_)));
        // Chaque tentative a reçu le 302…
        assert_eq!(attempts.load(Ordering::SeqCst), MAX_ATTEMPTS as usize);
        // …et la cible de la redirection, pourtant joignable, n'a JAMAIS été
        // contactée : c'est ce qui prouve que la redirection n'est pas suivie.
        assert_eq!(followed.load(Ordering::SeqCst), 0);
    }
}
