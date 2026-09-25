//! Client `CarbonFr` + `CarbonFrBuilder` (ADR-0031 décision 3).
//!
//! Construction TLS : `rustls::ClientConfig` avec le provider crypto `ring`
//! passé **explicitement** (`ClientConfig::builder_with_provider`) et le
//! vérificateur du magasin système (`rustls_platform_verifier`), donnée à
//! `reqwest` via `ClientBuilder::tls_backend_preconfigured`. Ce chemin ne
//! touche **jamais** `rustls::crypto::CryptoProvider::get_default()` — ni en
//! lecture implicite (comme le ferait `ClientConfig::builder()`/
//! `reqwest::ClientBuilder::use_rustls_tls()`), ni en écriture
//! (`install_default()`, utilisé par `bin/server`/les adapters mais jamais
//! ici) — cf. `tests::crypto_provider_default_stays_uninstalled` plus bas, et
//! `reqwest::async_impl::client::TlsBackend::BuiltRustls` (le seul bras du
//! `match` emprunté par ce chemin, distinct de `TlsBackend::Rustls` qui
//! panique sans provider installé).

use std::sync::Arc;
use std::time::Duration;

use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;
use serde::de::DeserializeOwned;

use crate::error::{CarbonFrError, ProblemDetails};

#[cfg(feature = "stream")]
use crate::stream::StreamConfig;

/// URL de base de l'instance hébergée par défaut (ADR-0031).
pub const DEFAULT_BASE_URL: &str = "https://carbon-fr-api.kovelt.fr";

/// Délai par défaut d'une requête REST (connexion + réponse) — écart assumé à
/// la parité stricte avec le SDK TS, qui n'en a aucun (point tranché par
/// Morgan le 2026-09-25, ADR-0031). Configurable
/// ([`CarbonFrBuilder::timeout`]) et désactivable
/// ([`CarbonFrBuilder::no_timeout`]).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Délai par défaut d'établissement de connexion (cf.
/// [`CarbonFrBuilder::connect_timeout`]) : pour le REST, TCP + TLS seulement
/// (le reste de la requête relève de [`DEFAULT_TIMEOUT`]) ; pour le flux SSE,
/// TCP + TLS + envoi de la requête + réception des en-têtes, le flux n'ayant
/// ensuite **aucun** timeout total (ADR-0031, points tranchés 2026-09-25).
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Construit le `rustls::ClientConfig` du SDK : provider `ring` explicite +
/// vérificateur du magasin système de certificats, sans jamais lire ni
/// installer `CryptoProvider::get_default()` (voir le commentaire de module).
fn build_tls_config() -> Result<ClientConfig, CarbonFrError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| CarbonFrError::Config(format!("versions TLS invalides : {e}")))?
        .with_platform_verifier()
        .map_err(|e| {
            CarbonFrError::Config(format!(
                "échec de configuration du vérificateur de certificats : {e}"
            ))
        })
        .map(|builder| builder.with_no_client_auth())
}

/// Construit un [`CarbonFr`].
///
/// ```no_run
/// # async fn go() -> Result<(), carbonfr_sdk::CarbonFrError> {
/// let client = carbonfr_sdk::CarbonFr::builder()
///     .api_key("clé-api")
///     .build()?;
/// # let _ = client;
/// # Ok(())
/// # }
/// ```
pub struct CarbonFrBuilder {
    base_url: String,
    api_key: Option<String>,
    rest_timeout: Option<Duration>,
    connect_timeout: Duration,
    http_client: Option<reqwest::Client>,
    #[cfg(feature = "stream")]
    stream_config: StreamConfig,
}

impl Default for CarbonFrBuilder {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key: None,
            rest_timeout: Some(DEFAULT_TIMEOUT),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            http_client: None,
            #[cfg(feature = "stream")]
            stream_config: StreamConfig::default(),
        }
    }
}

impl CarbonFrBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// URL de base de l'API (défaut : [`DEFAULT_BASE_URL`], l'instance
    /// hébergée Kovelt). Doit être une URL absolue `http`/`https`.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Clé API `Bearer` (ADR-0015) — requise seulement pour les webhooks et
    /// le quota du tier hébergé. Envoyée en en-tête `Authorization`
    /// **uniquement** (jamais dans l'URL/la query string).
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Délai par requête REST (défaut [`DEFAULT_TIMEOUT`], 30 s). Sans effet
    /// sur le flux SSE (`IntensityStream`), qui n'a pas de timeout total —
    /// voir `CarbonFrBuilder::stream_config` (feature `stream`).
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.rest_timeout = Some(timeout);
        self
    }

    /// Désactive le délai par requête REST (aucun timeout total — écart
    /// assumé à la parité stricte avec le SDK TS).
    pub fn no_timeout(mut self) -> Self {
        self.rest_timeout = None;
        self
    }

    /// Délai d'établissement de connexion (défaut [`DEFAULT_CONNECT_TIMEOUT`],
    /// 10 s). REST : TCP + TLS uniquement (le reste de la requête est borné par
    /// [`CarbonFrBuilder::timeout`]). Flux SSE : TCP + TLS + envoi de la
    /// requête + réception des en-têtes, à chaque (re)connexion.
    pub fn connect_timeout(mut self, connect_timeout: Duration) -> Self {
        self.connect_timeout = connect_timeout;
        self
    }

    /// Configuration du flux SSE (délai de lecture entre messages,
    /// reconnexion) — voir [`crate::stream::StreamConfig`].
    #[cfg(feature = "stream")]
    pub fn stream_config(mut self, stream_config: StreamConfig) -> Self {
        self.stream_config = stream_config;
        self
    }

    /// Injecte un `reqwest::Client` déjà construit — **à l'appelant** de le
    /// configurer (TLS, proxy…) ; [`CarbonFrBuilder::connect_timeout`] est
    /// alors ignoré pour ce client (déjà figé à la construction), mais
    /// [`CarbonFrBuilder::timeout`]/[`CarbonFrBuilder::api_key`] restent
    /// appliqués par requête (voir `CarbonFr::request`).
    pub fn http_client(mut self, http_client: reqwest::Client) -> Self {
        self.http_client = Some(http_client);
        self
    }

    /// Construit le client. Échoue toujours par `Result` — jamais de
    /// panique, jamais de repli silencieux sur un client par défaut (même en
    /// l'absence de provider crypto installé au niveau du processus, cf. le
    /// commentaire de module).
    pub fn build(self) -> Result<CarbonFr, CarbonFrError> {
        let base_url = url::Url::parse(&self.base_url).map_err(|e| {
            CarbonFrError::Config(format!("base_url invalide ({e}) : {}", self.base_url))
        })?;
        if base_url.scheme() != "http" && base_url.scheme() != "https" {
            return Err(CarbonFrError::Config(format!(
                "base_url doit être http(s) : {}",
                self.base_url
            )));
        }
        if base_url.cannot_be_a_base() {
            return Err(CarbonFrError::Config(format!(
                "base_url sans chemin exploitable : {}",
                self.base_url
            )));
        }

        let http = match self.http_client {
            Some(client) => client,
            None => {
                let tls = build_tls_config()?;
                reqwest::Client::builder()
                    .connect_timeout(self.connect_timeout)
                    .tls_backend_preconfigured(tls)
                    .build()
                    .map_err(CarbonFrError::Transport)?
            }
        };

        Ok(CarbonFr {
            http,
            base_url,
            api_key: self.api_key,
            user_agent: format!("carbonfr-sdk/{}", env!("CARGO_PKG_VERSION")),
            rest_timeout: self.rest_timeout,
            #[cfg(feature = "stream")]
            connect_timeout: self.connect_timeout,
            #[cfg(feature = "stream")]
            stream_config: self.stream_config,
        })
    }
}

/// Client de l'API carbon-fr (intensité carbone de l'électricité française).
///
/// Se construit via [`CarbonFr::builder`] — jamais directement (aucun champ
/// public), pour garder la main sur la construction TLS.
#[derive(Clone)]
pub struct CarbonFr {
    pub(crate) http: reqwest::Client,
    pub(crate) base_url: url::Url,
    pub(crate) api_key: Option<String>,
    pub(crate) user_agent: String,
    /// Délai par défaut d'une requête REST — lu par `CarbonFr::send_checked`
    /// (`request`/`send_checked`/`request_json` ci-dessous, utilisés par les
    /// 25 méthodes REST de `crate::methods`, ADR-0031 décision 8) et par le
    /// flux SSE (délai de connexion, `crate::stream`, feature `stream`).
    pub(crate) rest_timeout: Option<Duration>,
    #[cfg(feature = "stream")]
    pub(crate) connect_timeout: Duration,
    #[cfg(feature = "stream")]
    pub(crate) stream_config: StreamConfig,
}

/// `Debug` écrit à la main : la clé API n'est **jamais** affichée (`{:?}`,
/// `dbg!`, `tracing::debug!(?client)`…), seulement sa présence.
impl std::fmt::Debug for CarbonFr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CarbonFr")
            .field("base_url", &self.base_url.as_str())
            .field("api_key", &self.api_key.as_ref().map(|_| "<masquée>"))
            .field("user_agent", &self.user_agent)
            .field("rest_timeout", &self.rest_timeout)
            .finish_non_exhaustive()
    }
}

impl CarbonFr {
    /// Point d'entrée de construction — voir [`CarbonFrBuilder`].
    pub fn builder() -> CarbonFrBuilder {
        CarbonFrBuilder::new()
    }

    /// Construit une requête vers `path` (relatif à l'URL de base) : en-tête
    /// `User-Agent` toujours posé, `Authorization: Bearer …` **seulement**
    /// si une clé API est configurée. Appliqué de façon identique que le
    /// `reqwest::Client` sous-jacent ait été construit par le SDK ou injecté
    /// par l'appelant ([`CarbonFrBuilder::http_client`]) : les en-têtes
    /// par défaut d'un `Client` déjà construit ne peuvent plus être modifiés,
    /// donc ces deux en-têtes sont posés **par requête**, jamais au niveau du
    /// client.
    ///
    /// `request`/`request_for_url`/`send_checked`/`request_json` sont le
    /// socle **testé** (voir `tests` ci-dessous) des 25 méthodes REST
    /// publiques de `crate::methods` (ADR-0031 décision 8).
    pub(crate) fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, CarbonFrError> {
        Ok(self.request_for_url(method, self.url(path)?))
    }

    /// Résout `path` (relatif) en une URL absolue à partir de l'URL de base —
    /// factorisé pour `request` et pour les méthodes REST qui doivent ajouter
    /// des paramètres de requête avant d'appeler `request_for_url`
    /// (`crate::query`).
    pub(crate) fn url(&self, path: &str) -> Result<url::Url, CarbonFrError> {
        self.base_url.join(path).map_err(|e| {
            CarbonFrError::Config(format!("chemin de requête invalide « {path} » : {e}"))
        })
    }

    /// Variante de [`CarbonFr::request`] pour une URL déjà résolue (flux SSE :
    /// une seule résolution d'URL, réutilisée à chaque tentative de
    /// reconnexion — voir `crate::stream`).
    pub(crate) fn request_for_url(
        &self,
        method: reqwest::Method,
        url: url::Url,
    ) -> reqwest::RequestBuilder {
        let mut builder = self
            .http
            .request(method, url)
            .header(reqwest::header::USER_AGENT, self.user_agent.as_str());
        if let Some(api_key) = &self.api_key {
            builder = builder.bearer_auth(api_key);
        }
        builder
    }

    /// Envoie `builder` en appliquant le délai REST par défaut (si activé),
    /// et convertit une réponse non 2xx en [`CarbonFrError::Api`] (corps
    /// `application/problem+json` attendu, RFC 9457 — repli
    /// [`ProblemDetails::fallback`] si le corps n'en est pas un).
    pub(crate) async fn send_checked(
        &self,
        mut builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, CarbonFrError> {
        if let Some(timeout) = self.rest_timeout {
            builder = builder.timeout(timeout);
        }
        let response = builder.send().await?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let bytes = response.bytes().await.unwrap_or_default();
        let problem = serde_json::from_slice::<ProblemDetails>(&bytes)
            .unwrap_or_else(|_| ProblemDetails::fallback(status, &String::from_utf8_lossy(&bytes)));
        Err(CarbonFrError::Api { status, problem })
    }

    /// [`CarbonFr::send_checked`] puis désérialisation JSON du corps en `T`.
    pub(crate) async fn request_json<T: DeserializeOwned>(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<T, CarbonFrError> {
        let response = self.send_checked(builder).await?;
        let bytes = response.bytes().await?;
        serde_json::from_slice(&bytes).map_err(|e| CarbonFrError::Decode(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    /// Le `Debug` du client ne doit **jamais** révéler la clé API.
    #[test]
    fn debug_never_prints_api_key() {
        let client = CarbonFr::builder()
            .api_key("SECRET-cle-api-ne-pas-afficher")
            .build()
            .expect("client");
        let dbg = format!("{client:?}");
        assert!(
            !dbg.contains("SECRET-cle-api-ne-pas-afficher"),
            "clé visible : {dbg}"
        );
        assert!(dbg.contains("<masquée>"));
    }

    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::get;

    use super::*;

    /// Démarre `app` sur `127.0.0.1:<port éphémère>` (patron `adapter-webhook`,
    /// `crates/adapter-webhook/src/lib.rs`) : la tâche serveur est abandonnée
    /// (donc annulée) à la fin de chaque `#[tokio::test]`.
    async fn spawn_server(app: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind du serveur de test");
        let addr = listener.local_addr().expect("adresse locale");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), handle)
    }

    fn test_client(base_url: &str) -> CarbonFr {
        CarbonFr::builder()
            .base_url(base_url)
            .timeout(Duration::from_secs(5))
            .build()
            .expect("client de test")
    }

    // --- Construction --------------------------------------------------

    #[test]
    fn builds_with_defaults() {
        let client = CarbonFr::builder().build().expect("client par défaut");
        // `url::Url::parse` normalise en ajoutant un `/` final.
        assert_eq!(
            client.base_url,
            url::Url::parse(DEFAULT_BASE_URL).expect("URL par défaut valide")
        );
        assert!(client.api_key.is_none());
        assert_eq!(client.rest_timeout, Some(DEFAULT_TIMEOUT));
    }

    #[test]
    fn rejects_non_http_base_url() {
        let err = CarbonFr::builder()
            .base_url("ftp://example.com")
            .build()
            .unwrap_err();
        assert!(matches!(err, CarbonFrError::Config(_)));
    }

    #[test]
    fn accepts_injected_http_client() {
        // Un client injecté doit gérer son propre TLS (doc de
        // `CarbonFrBuilder::http_client`) : on reproduit ici la construction
        // qu'un appelant ferait (jamais `reqwest::Client::new()`, qui
        // paniquerait sans provider par défaut installé, cf.
        // `crypto_provider_default_stays_uninstalled`), pour prouver que
        // `build()` prend bien ce chemin (pas d'appel à
        // `build_tls_config` en interne) plutôt que de construire son propre
        // client.
        let tls = build_tls_config().expect("config TLS de test");
        let injected = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .build()
            .expect("client injecté");
        let client = CarbonFr::builder()
            .http_client(injected)
            .build()
            .expect("client avec http_client injecté");
        assert!(client.api_key.is_none());
    }

    /// Preuve exigée par ADR-0031 décision 3 : construire un `CarbonFr` ne
    /// lit ni n'installe le provider crypto par défaut du **processus**
    /// (`rustls::crypto::CryptoProvider::get_default()`), à l'inverse du
    /// bootstrap de `bin/server`/`adapter-webhook::ensure_crypto_provider`.
    /// Ce test tourne dans le même binaire que tous les autres tests de ce
    /// module : la seule façon qu'il échoue est qu'*une* ligne de ce crate
    /// appelle un jour `install_default()` — ce qu'il garde impossible.
    #[test]
    fn crypto_provider_default_stays_uninstalled() {
        assert!(
            rustls::crypto::CryptoProvider::get_default().is_none(),
            "aucun autre test ne doit installer de provider par défaut"
        );
        let _client = CarbonFr::builder().build().expect("client de test");
        assert!(
            rustls::crypto::CryptoProvider::get_default().is_none(),
            "CarbonFrBuilder::build ne doit jamais installer de provider par défaut du processus"
        );
    }

    // --- En-têtes --------------------------------------------------------

    #[derive(Default, Clone)]
    struct Captured(Arc<Mutex<Option<HeaderMap>>>);

    async fn capture(State(state): State<Captured>, headers: HeaderMap) -> &'static str {
        *state.0.lock().expect("verrou capture") = Some(headers);
        "{}"
    }

    #[tokio::test]
    async fn sends_user_agent_and_no_authorization_without_api_key() {
        let captured = Captured::default();
        let app = Router::new()
            .route("/probe", get(capture))
            .with_state(captured.clone());
        let (base_url, _server) = spawn_server(app).await;

        let client = test_client(&base_url);
        let builder = client.request(reqwest::Method::GET, "probe").unwrap();
        client.send_checked(builder).await.expect("réponse 200");

        let headers = captured.0.lock().unwrap().take().expect("requête reçue");
        assert_eq!(
            headers.get(reqwest::header::USER_AGENT).unwrap(),
            &format!("carbonfr-sdk/{}", env!("CARGO_PKG_VERSION"))
        );
        assert!(headers.get(reqwest::header::AUTHORIZATION).is_none());
    }

    #[tokio::test]
    async fn sends_bearer_authorization_when_api_key_set() {
        let captured = Captured::default();
        let app = Router::new()
            .route("/probe", get(capture))
            .with_state(captured.clone());
        let (base_url, _server) = spawn_server(app).await;

        let client = CarbonFr::builder()
            .base_url(&base_url)
            .api_key("secret-key-123")
            .timeout(Duration::from_secs(5))
            .build()
            .expect("client de test");
        let builder = client.request(reqwest::Method::GET, "probe").unwrap();
        client.send_checked(builder).await.expect("réponse 200");

        let headers = captured.0.lock().unwrap().take().expect("requête reçue");
        assert_eq!(
            headers.get(reqwest::header::AUTHORIZATION).unwrap(),
            "Bearer secret-key-123"
        );
    }

    // --- Erreurs API (RFC 9457) ------------------------------------------

    #[tokio::test]
    async fn api_error_decodes_problem_details_with_code() {
        async fn not_found() -> axum::response::Response {
            axum::response::Response::builder()
                .status(404)
                .header("content-type", "application/problem+json")
                .body(axum::body::Body::from(
                    r#"{"type":"about:blank","title":"No data","status":404,"detail":"no measurement for this period","code":"no_data"}"#,
                ))
                .unwrap()
        }
        let app = Router::new().route("/v1/intensity/now", get(not_found));
        let (base_url, _server) = spawn_server(app).await;
        let client = test_client(&base_url);

        let builder = client
            .request(reqwest::Method::GET, "v1/intensity/now")
            .unwrap();
        let err = client
            .request_json::<serde_json::Value>(builder)
            .await
            .unwrap_err();

        match &err {
            CarbonFrError::Api { status, problem } => {
                assert_eq!(*status, 404);
                assert_eq!(problem.code(), Some("no_data"));
                assert_eq!(problem.detail(), Some("no measurement for this period"));
            }
            other => panic!("attendu CarbonFrError::Api, obtenu {other:?}"),
        }
        assert_eq!(err.code(), Some("no_data"));
        assert_eq!(err.status(), Some(404));
    }

    #[tokio::test]
    async fn api_error_falls_back_when_body_is_not_problem_json() {
        async fn bad_gateway() -> axum::response::Response {
            axum::response::Response::builder()
                .status(502)
                .body(axum::body::Body::from("upstream error"))
                .unwrap()
        }
        let app = Router::new().route("/v1/intensity/now", get(bad_gateway));
        let (base_url, _server) = spawn_server(app).await;
        let client = test_client(&base_url);

        let builder = client
            .request(reqwest::Method::GET, "v1/intensity/now")
            .unwrap();
        let err = client
            .request_json::<serde_json::Value>(builder)
            .await
            .unwrap_err();

        match err {
            CarbonFrError::Api { status, problem } => {
                assert_eq!(status, 502);
                assert_eq!(problem.detail(), Some("upstream error"));
            }
            other => panic!("attendu CarbonFrError::Api, obtenu {other:?}"),
        }
    }

    // --- Timeout REST ------------------------------------------------------

    #[tokio::test]
    async fn rest_request_times_out() {
        async fn slow() -> &'static str {
            tokio::time::sleep(Duration::from_millis(300)).await;
            "{}"
        }
        let app = Router::new().route("/v1/slow", get(slow));
        let (base_url, _server) = spawn_server(app).await;

        let client = CarbonFr::builder()
            .base_url(&base_url)
            .timeout(Duration::from_millis(30))
            .build()
            .expect("client de test");
        let builder = client.request(reqwest::Method::GET, "v1/slow").unwrap();
        let err = client
            .request_json::<serde_json::Value>(builder)
            .await
            .unwrap_err();

        match err {
            CarbonFrError::Transport(e) => {
                assert!(e.is_timeout(), "erreur attendue : timeout, obtenu {e:?}")
            }
            other => panic!("attendu CarbonFrError::Transport(timeout), obtenu {other:?}"),
        }
    }

    #[tokio::test]
    async fn rest_timeout_can_be_disabled() {
        async fn slow() -> &'static str {
            tokio::time::sleep(Duration::from_millis(80)).await;
            "{}"
        }
        let app = Router::new().route("/v1/slow", get(slow));
        let (base_url, _server) = spawn_server(app).await;

        let client = CarbonFr::builder()
            .base_url(&base_url)
            .no_timeout()
            .build()
            .expect("client de test");
        assert_eq!(client.rest_timeout, None);
        let builder = client.request(reqwest::Method::GET, "v1/slow").unwrap();
        client
            .request_json::<serde_json::Value>(builder)
            .await
            .expect("pas de timeout : la requête doit aboutir");
    }
}
