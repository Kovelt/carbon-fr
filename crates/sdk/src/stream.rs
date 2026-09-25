//! Flux SSE `GET /v1/intensity/stream` (`stream`, feature Cargo optionnelle —
//! ADR-0031 décisions 7 et 9, et l'encadré « points tranchés » du
//! 2026-09-25).
//!
//! Type nommé [`IntensityStream`] (`futures_core::Stream<Item =
//! Result<IntensityEvent, CarbonFrError>>`), posé sur `eventsource-stream`
//! par-dessus `Response::bytes_stream()`. **Pas de timeout total** : un délai
//! de connexion ([`CarbonFrBuilder::connect_timeout`](crate::CarbonFrBuilder::connect_timeout))
//! puis un délai de lecture entre deux messages
//! ([`StreamConfig::idle_timeout`]) — le keep-alive serveur (`: …`, toutes
//! les ~15 s côté `crates/adapter-http`) doit rester sous ce délai, sinon une
//! connexion vivante serait faussement jugée morte. Au-delà de l'un ou
//! l'autre délai : reconnexion automatique **volontairement mince** (délai
//! fixe puis exponentiel borné, [`Reconnect`]), **sans** `Last-Event-ID` — le
//! serveur n'en émet aucun (`StreamEventBody`, `crates/adapter-http/src/dto.rs`)
//! et sa source (`tokio::broadcast` éphémère, ADR-0014) ne permettrait de
//! toute façon aucun rejeu : **des événements peuvent être perdus pendant une
//! coupure**.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use eventsource_stream::{Event as RawEvent, EventStreamError, Eventsource};
use futures_core::Stream;
use serde::Deserialize;

use crate::client::CarbonFr;
use crate::error::{CarbonFrError, ProblemDetails};
use crate::region::Region;

/// Délai par défaut de lecture entre deux messages du flux (aucun octet reçu,
/// keep-alive compris) avant de considérer la connexion morte — 3×
/// l'intervalle de keep-alive observé côté serveur (`Sse::keep_alive`,
/// défaut axum ~15 s, `crates/adapter-http/src/handlers.rs::intensity_stream`).
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// Politique de reconnexion automatique de [`IntensityStream`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconnect {
    /// Aucune reconnexion : une coupure (délai de lecture dépassé, connexion
    /// fermée par le serveur, erreur de transport) termine le flux — le
    /// dernier élément est `Some(Err(_))` (ou `None` si le serveur a fermé
    /// proprement), puis `None`.
    Disabled,
    /// Reconnexion automatique : `initial_backoff` avant la première
    /// tentative, puis backoff exponentiel (×2) plafonné à `max_backoff`.
    /// Réinitialisé à `initial_backoff` dès qu'une reconnexion réussit.
    /// Aucune limite de nombre de tentatives (délai **plafonné**, pas les
    /// essais) — une coupure réseau transitoire ne doit pas faire échouer le
    /// flux silencieusement au bout de N essais.
    Enabled {
        initial_backoff: Duration,
        max_backoff: Duration,
    },
}

impl Default for Reconnect {
    fn default() -> Self {
        Reconnect::Enabled {
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
        }
    }
}

/// Configuration du flux SSE — voir [`CarbonFrBuilder::stream_config`](crate::CarbonFrBuilder::stream_config).
#[derive(Debug, Clone, Copy)]
pub struct StreamConfig {
    /// Délai de lecture entre deux messages (défaut [`DEFAULT_IDLE_TIMEOUT`]).
    pub idle_timeout: Duration,
    /// Politique de reconnexion (défaut : activée, backoff 1 s → 30 s).
    pub reconnect: Reconnect,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            reconnect: Reconnect::default(),
        }
    }
}

/// Paramètres de [`CarbonFr::intensity_stream`] — mêmes filtres que
/// `GET /v1/intensity/stream` côté serveur (`StreamQuery`,
/// `crates/adapter-http/src/handlers.rs`) : région et seuil.
#[derive(Debug, Clone, Default)]
pub struct IntensityStreamOptions {
    /// Filtre région (défaut : toutes les régions poussées).
    pub region: Option<Region>,
    /// Ne pousser que les mises à jour strictement sous ce seuil
    /// (gCO₂eq/kWh).
    pub below: Option<f64>,
}

/// Un événement `intensity` du flux (`StreamEventBody`,
/// `crates/adapter-http/src/dto.rs`).
///
/// Équivalent du type `StreamEvent` du SDK TypeScript.
#[derive(Debug, Clone, Deserialize)]
pub struct IntensityEvent {
    pub region: String,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: time::OffsetDateTime,
    pub intensity: f64,
    pub methodology: String,
    pub methodology_version: u32,
    pub unit: String,
}

/// Distingue, au niveau **octets bruts** (avant analyse SSE), une erreur de
/// transport d'un dépassement du délai de lecture — le délai est réarmé à
/// chaque paquet reçu, keep-alive compris, via [`with_idle_timeout`] : c'est
/// la seule façon de ne pas confondre un keep-alive (qui doit garder la
/// connexion « vivante ») avec une absence totale d'activité.
#[derive(Debug)]
enum IdleOrTransportError<E> {
    Idle,
    Transport(E),
}

impl<E: std::fmt::Display> std::fmt::Display for IdleOrTransportError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdleOrTransportError::Idle => f.write_str("délai de lecture dépassé"),
            IdleOrTransportError::Transport(e) => write!(f, "{e}"),
        }
    }
}

impl<E: std::fmt::Display + std::fmt::Debug> std::error::Error for IdleOrTransportError<E> {}

/// Réarme un délai à chaque élément reçu du flux `stream` (keep-alive compris
/// — la trame n'a pas encore été analysée comme événement SSE à ce niveau) ;
/// émet une [`IdleOrTransportError::Idle`] si `idle_timeout` s'écoule sans
/// aucun élément. Appliqué **avant** [`Eventsource::eventsource`] (décision 7,
/// ADR-0031) : après coup, les trames keep-alive (commentaires `: …`) sont
/// déjà absorbées par le parseur SSE sans jamais atteindre l'appelant — trop
/// tard pour réarmer un délai dessus.
fn with_idle_timeout<S, B, E>(
    stream: S,
    idle_timeout: Duration,
) -> impl Stream<Item = Result<B, IdleOrTransportError<E>>> + Send + 'static
where
    S: Stream<Item = Result<B, E>> + Send + 'static,
    B: Send + 'static,
    E: Send + 'static,
{
    let mut inner = Box::pin(stream);
    let mut deadline = Box::pin(tokio::time::sleep(idle_timeout));
    futures_util::stream::poll_fn(move |cx| match inner.as_mut().poll_next(cx) {
        Poll::Ready(Some(Ok(item))) => {
            deadline
                .as_mut()
                .reset(tokio::time::Instant::now() + idle_timeout);
            Poll::Ready(Some(Ok(item)))
        }
        Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(IdleOrTransportError::Transport(e)))),
        Poll::Ready(None) => Poll::Ready(None),
        Poll::Pending => match deadline.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(Some(Err(IdleOrTransportError::Idle))),
            Poll::Pending => Poll::Pending,
        },
    })
}

type StreamError = EventStreamError<IdleOrTransportError<reqwest::Error>>;
type BoxedEventStream = Pin<Box<dyn Stream<Item = Result<RawEvent, StreamError>> + Send>>;
type ConnectFuture = Pin<Box<dyn Future<Output = Result<BoxedEventStream, CarbonFrError>> + Send>>;

fn map_stream_err(err: StreamError, idle_timeout: Duration) -> CarbonFrError {
    match err {
        EventStreamError::Transport(IdleOrTransportError::Idle) => {
            CarbonFrError::Timeout(idle_timeout)
        }
        EventStreamError::Transport(IdleOrTransportError::Transport(e)) => {
            CarbonFrError::Transport(e)
        }
        EventStreamError::Utf8(e) => CarbonFrError::Decode(e.to_string()),
        EventStreamError::Parser(e) => CarbonFrError::Decode(e.to_string()),
    }
}

/// Une (re)connexion : envoie la requête (délai `connect_timeout`), vérifie
/// le statut (une réponse en erreur devient un [`CarbonFrError::Api`] avant
/// même d'entamer le flux), puis branche [`with_idle_timeout`] +
/// `.eventsource()` sur le corps.
fn connect(
    http: reqwest::Client,
    url: url::Url,
    api_key: Option<String>,
    user_agent: String,
    connect_timeout: Duration,
    idle_timeout: Duration,
) -> ConnectFuture {
    Box::pin(async move {
        let mut builder = http
            .get(url)
            .header(reqwest::header::USER_AGENT, user_agent)
            .header(reqwest::header::ACCEPT, "text/event-stream");
        if let Some(api_key) = api_key {
            builder = builder.bearer_auth(api_key);
        }

        let response = tokio::time::timeout(connect_timeout, builder.send())
            .await
            .map_err(|_| CarbonFrError::Timeout(connect_timeout))?
            .map_err(CarbonFrError::from)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let bytes = response.bytes().await.unwrap_or_default();
            let problem = serde_json::from_slice::<ProblemDetails>(&bytes).unwrap_or_else(|_| {
                ProblemDetails::fallback(status, &String::from_utf8_lossy(&bytes))
            });
            return Err(CarbonFrError::Api { status, problem });
        }

        let raw = with_idle_timeout(response.bytes_stream(), idle_timeout);
        Ok(Box::pin(raw.eventsource()) as BoxedEventStream)
    })
}

enum State {
    Connecting(ConnectFuture),
    Streaming(BoxedEventStream),
    Waiting(Pin<Box<tokio::time::Sleep>>),
    Done,
}

/// Flux **live** des mises à jour d'intensité (`GET /v1/intensity/stream`,
/// Server-Sent Events, ADR-0014 §2) — construit par [`CarbonFr::intensity_stream`].
///
/// **Pas de timeout total** : un délai de connexion
/// ([`CarbonFrBuilder::connect_timeout`](crate::CarbonFrBuilder::connect_timeout))
/// puis un délai de lecture entre deux messages
/// ([`StreamConfig::idle_timeout`]) — le keep-alive serveur (`: …`, toutes
/// les ~15 s côté `crates/adapter-http`) doit rester sous ce délai, sinon une
/// connexion vivante serait faussement jugée morte. Au-delà de l'un ou
/// l'autre délai : reconnexion automatique **volontairement mince** (délai
/// fixe puis exponentiel borné, [`Reconnect`]), **sans** `Last-Event-ID` — le
/// serveur n'en émet aucun et sa source (`tokio::broadcast` éphémère,
/// ADR-0014) ne permettrait de toute façon aucun rejeu : **des événements
/// peuvent être perdus pendant une coupure**.
pub struct IntensityStream {
    http: reqwest::Client,
    url: url::Url,
    api_key: Option<String>,
    user_agent: String,
    connect_timeout: Duration,
    idle_timeout: Duration,
    reconnect: Reconnect,
    backoff: Duration,
    state: State,
}

impl IntensityStream {
    fn connect_future(&self) -> ConnectFuture {
        connect(
            self.http.clone(),
            self.url.clone(),
            self.api_key.clone(),
            self.user_agent.clone(),
            self.connect_timeout,
            self.idle_timeout,
        )
    }

    /// `None` : reconnexion désactivée ou déjà à l'arrêt — le flux se
    /// termine. `Some(delay)` : délai avant la prochaine tentative, et le
    /// backoff interne est avancé (doublé, plafonné) pour la suivante.
    fn next_backoff(&mut self) -> Option<Duration> {
        match self.reconnect {
            Reconnect::Disabled => None,
            Reconnect::Enabled { max_backoff, .. } => {
                let delay = self.backoff;
                self.backoff = self.backoff.saturating_mul(2).min(max_backoff);
                Some(delay)
            }
        }
    }

    fn reset_backoff(&mut self) {
        if let Reconnect::Enabled {
            initial_backoff, ..
        } = self.reconnect
        {
            self.backoff = initial_backoff;
        }
    }

    fn poll_next_inner(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<IntensityEvent, CarbonFrError>>> {
        loop {
            match &mut self.state {
                State::Connecting(fut) => match fut.as_mut().poll(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(stream)) => {
                        self.reset_backoff();
                        self.state = State::Streaming(stream);
                    }
                    Poll::Ready(Err(err)) => match self.next_backoff() {
                        Some(delay) => {
                            self.state = State::Waiting(Box::pin(tokio::time::sleep(delay)));
                        }
                        None => {
                            self.state = State::Done;
                            return Poll::Ready(Some(Err(err)));
                        }
                    },
                },
                State::Streaming(stream) => match stream.as_mut().poll_next(cx) {
                    Poll::Pending => return Poll::Pending,
                    // Seuls les événements nommés `intensity` sont émis par le
                    // serveur (`Event::default().event("intensity")...`,
                    // `crates/adapter-http/src/handlers.rs`) ; tout autre nom
                    // est ignoré défensivement plutôt que de faire échouer le
                    // flux. Les commentaires keep-alive (`: …`) ne sont même
                    // pas dispatchés jusqu'ici : `eventsource-stream` ne
                    // produit un `Event` que si son tampon `data` est non
                    // vide (RawEventLine::Comment est un no-op côté parseur).
                    Poll::Ready(Some(Ok(event))) if event.event != "intensity" => continue,
                    Poll::Ready(Some(Ok(event))) => {
                        match serde_json::from_str::<IntensityEvent>(&event.data) {
                            Ok(parsed) => return Poll::Ready(Some(Ok(parsed))),
                            Err(e) => {
                                return Poll::Ready(Some(Err(CarbonFrError::Decode(
                                    e.to_string(),
                                ))));
                            }
                        }
                    }
                    Poll::Ready(Some(Err(err))) => {
                        let mapped = map_stream_err(err, self.idle_timeout);
                        match self.next_backoff() {
                            Some(delay) => {
                                self.state = State::Waiting(Box::pin(tokio::time::sleep(delay)));
                            }
                            None => {
                                self.state = State::Done;
                                return Poll::Ready(Some(Err(mapped)));
                            }
                        }
                    }
                    // Le serveur a fermé la connexion proprement : un flux SSE
                    // est censé être long-lived, donc une fin inattendue est
                    // traitée comme une coupure (reconnexion), pas comme une
                    // fin normale du flux.
                    Poll::Ready(None) => match self.next_backoff() {
                        Some(delay) => {
                            self.state = State::Waiting(Box::pin(tokio::time::sleep(delay)));
                        }
                        None => {
                            self.state = State::Done;
                            return Poll::Ready(None);
                        }
                    },
                },
                State::Waiting(sleep) => match sleep.as_mut().poll(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(()) => {
                        self.state = State::Connecting(self.connect_future());
                    }
                },
                State::Done => return Poll::Ready(None),
            }
        }
    }
}

// Tous les champs de `State`/`IntensityStream` sont `Unpin` (les futures et
// flux internes sont déjà `Pin<Box<…>>`, et `Pin<Box<T>>` est toujours
// `Unpin` quel que soit `T`) : `poll_next` peut donc utiliser `get_mut`.
impl Stream for IntensityStream {
    type Item = Result<IntensityEvent, CarbonFrError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().poll_next_inner(cx)
    }
}

impl CarbonFr {
    /// Flux **live** des mises à jour d'intensité (`GET /v1/intensity/stream`,
    /// Server-Sent Events, ADR-0014 §2) — voir [`IntensityStream`] pour le
    /// détail du contrat (pas de timeout total, délais, reconnexion).
    pub fn intensity_stream(
        &self,
        options: IntensityStreamOptions,
    ) -> Result<IntensityStream, CarbonFrError> {
        let mut url = self
            .base_url
            .join("v1/intensity/stream")
            .map_err(|e| CarbonFrError::Config(format!("URL du flux SSE invalide : {e}")))?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(region) = &options.region {
                query.append_pair("region", region.as_str());
            }
            if let Some(below) = options.below {
                query.append_pair("below", &below.to_string());
            }
        }

        let reconnect = self.stream_config.reconnect;
        let backoff = match reconnect {
            Reconnect::Enabled {
                initial_backoff, ..
            } => initial_backoff,
            Reconnect::Disabled => Duration::ZERO,
        };
        let http = self.http.clone();
        let api_key = self.api_key.clone();
        let user_agent = self.user_agent.clone();
        let connect_timeout = self.connect_timeout;
        let idle_timeout = self.stream_config.idle_timeout;

        let state = State::Connecting(connect(
            http.clone(),
            url.clone(),
            api_key.clone(),
            user_agent.clone(),
            connect_timeout,
            idle_timeout,
        ));

        Ok(IntensityStream {
            http,
            url,
            api_key,
            user_agent,
            connect_timeout,
            idle_timeout,
            reconnect,
            backoff,
            state,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::response::sse::Event as AxumSseEvent;
    use axum::response::sse::Sse;
    use axum::routing::get;
    use futures_util::StreamExt;

    use super::*;
    use crate::client::CarbonFr;

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

    fn intensity_frame(region: &str, intensity: f64) -> AxumSseEvent {
        let body = serde_json::json!({
            "region": region,
            "timestamp": "2026-09-25T12:00:00Z",
            "intensity": intensity,
            "methodology": "rte-direct",
            "methodology_version": 1,
            "unit": "gCO2eq/kWh",
        });
        AxumSseEvent::default()
            .event("intensity")
            .data(body.to_string())
    }

    // --- Backoff (pur, sans I/O) ------------------------------------------

    /// `IntensityStream` minimal (jamais connecté, `state: State::Done`) pour
    /// tester `next_backoff`/`reset_backoff` isolément. Le `reqwest::Client`
    /// vient de `CarbonFr::builder()` (jamais `reqwest::Client::new()`, qui
    /// paniquerait ici sans provider crypto par défaut installé, cf.
    /// `client::tests::crypto_provider_default_stays_uninstalled`).
    fn dummy_stream(reconnect: Reconnect) -> IntensityStream {
        let http = CarbonFr::builder().build().expect("client de test").http;
        let backoff = match reconnect {
            Reconnect::Enabled {
                initial_backoff, ..
            } => initial_backoff,
            Reconnect::Disabled => Duration::ZERO,
        };
        IntensityStream {
            http,
            url: url::Url::parse("http://127.0.0.1:1/v1/intensity/stream").expect("URL de test"),
            api_key: None,
            user_agent: "test".to_string(),
            connect_timeout: Duration::from_secs(1),
            idle_timeout: Duration::from_secs(1),
            reconnect,
            backoff,
            state: State::Done,
        }
    }

    #[test]
    fn backoff_disabled_never_retries() {
        let mut stream = dummy_stream(Reconnect::Disabled);
        assert_eq!(stream.next_backoff(), None);
    }

    #[test]
    fn backoff_doubles_and_caps_then_resets() {
        let reconnect = Reconnect::Enabled {
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(45),
        };
        let mut stream = dummy_stream(reconnect);
        let delays: Vec<Duration> = (0..5)
            .map(|_| stream.next_backoff().expect("reconnexion activée"))
            .collect();
        assert_eq!(
            delays,
            vec![10, 20, 40, 45, 45]
                .into_iter()
                .map(Duration::from_millis)
                .collect::<Vec<_>>()
        );
        stream.reset_backoff();
        assert_eq!(stream.next_backoff(), Some(Duration::from_millis(10)));
    }

    // --- Réception + keep-alive --------------------------------------------

    #[tokio::test]
    async fn receives_events_and_ignores_keep_alives() {
        async fn handler() -> Sse<impl Stream<Item = Result<AxumSseEvent, Infallible>>> {
            let frames = vec![
                Ok(AxumSseEvent::default().comment("hb")),
                Ok(AxumSseEvent::default().comment("hb")),
                Ok(intensity_frame("national", 42.0)),
                Ok(AxumSseEvent::default().comment("hb")),
                Ok(intensity_frame("bretagne", 12.5)),
            ];
            // Reste ouvert après les 5 trames (au lieu de fermer la
            // connexion), pour ne pas déclencher de reconnexion pendant que
            // le test lit les deux événements attendus.
            let stream = futures_util::stream::iter(frames).chain(futures_util::stream::pending());
            Sse::new(stream)
        }
        let app = Router::new().route("/v1/intensity/stream", get(handler));
        let (base_url, _server) = spawn_server(app).await;

        let client = CarbonFr::builder()
            .base_url(&base_url)
            .connect_timeout(Duration::from_secs(2))
            .build()
            .expect("client de test");
        let mut stream = client
            .intensity_stream(IntensityStreamOptions::default())
            .expect("construction du flux");

        let first = stream.next().await.expect("1er événement").expect("ok");
        assert_eq!(first.region, "national");
        assert!((first.intensity - 42.0).abs() < f64::EPSILON);

        let second = stream.next().await.expect("2e événement").expect("ok");
        assert_eq!(second.region, "bretagne");
    }

    // --- Reconnexion après coupure serveur ----------------------------------

    #[tokio::test]
    async fn reconnects_after_server_drops_connection() {
        let attempt = Arc::new(AtomicUsize::new(0));

        async fn handler(
            axum::extract::State(attempt): axum::extract::State<Arc<AtomicUsize>>,
        ) -> Sse<Pin<Box<dyn Stream<Item = Result<AxumSseEvent, Infallible>> + Send>>> {
            let n = attempt.fetch_add(1, Ordering::SeqCst);
            let stream: Pin<Box<dyn Stream<Item = Result<AxumSseEvent, Infallible>> + Send>> =
                if n == 0 {
                    // 1re connexion : un événement, puis fin de flux (le serveur
                    // ferme la connexion — cas « coupure »).
                    Box::pin(futures_util::stream::iter(vec![Ok(intensity_frame(
                        "national", 1.0,
                    ))]))
                } else {
                    // 2e connexion (après reconnexion) : un événement différent,
                    // connexion maintenue ensuite.
                    Box::pin(
                        futures_util::stream::iter(vec![Ok(intensity_frame("national", 2.0))])
                            .chain(futures_util::stream::pending()),
                    )
                };
            Sse::new(stream)
        }

        let app = Router::new()
            .route("/v1/intensity/stream", get(handler))
            .with_state(attempt);
        let (base_url, _server) = spawn_server(app).await;

        let client = CarbonFr::builder()
            .base_url(&base_url)
            .connect_timeout(Duration::from_secs(2))
            .stream_config(StreamConfig {
                idle_timeout: Duration::from_secs(5),
                reconnect: Reconnect::Enabled {
                    initial_backoff: Duration::from_millis(10),
                    max_backoff: Duration::from_millis(50),
                },
            })
            .build()
            .expect("client de test");
        let mut stream = client
            .intensity_stream(IntensityStreamOptions::default())
            .expect("construction du flux");

        let first = stream.next().await.expect("1er événement").expect("ok");
        assert!((first.intensity - 1.0).abs() < f64::EPSILON);

        // La reconnexion est silencieuse : pas d'`Err` intermédiaire, on
        // obtient directement le prochain événement, servi par la 2e
        // connexion.
        let second = stream
            .next()
            .await
            .expect("2e événement (après reconnexion)")
            .expect("ok");
        assert!((second.intensity - 2.0).abs() < f64::EPSILON);
    }

    // --- Reconnexion sur délai de lecture dépassé ---------------------------

    #[tokio::test]
    async fn reconnects_after_idle_timeout_elapsed() {
        let attempt = Arc::new(AtomicUsize::new(0));

        async fn handler(
            axum::extract::State(attempt): axum::extract::State<Arc<AtomicUsize>>,
        ) -> Sse<Pin<Box<dyn Stream<Item = Result<AxumSseEvent, Infallible>> + Send>>> {
            let n = attempt.fetch_add(1, Ordering::SeqCst);
            let stream: Pin<Box<dyn Stream<Item = Result<AxumSseEvent, Infallible>> + Send>> =
                if n == 0 {
                    // 1re connexion : la réponse démarre (200, en-têtes SSE) mais
                    // aucune trame n'est jamais écrite ⇒ dépassement du délai de
                    // lecture côté client.
                    Box::pin(futures_util::stream::pending())
                } else {
                    Box::pin(futures_util::stream::iter(vec![Ok(intensity_frame(
                        "national", 7.0,
                    ))]))
                };
            Sse::new(stream)
        }

        let app = Router::new()
            .route("/v1/intensity/stream", get(handler))
            .with_state(attempt);
        let (base_url, _server) = spawn_server(app).await;

        let client = CarbonFr::builder()
            .base_url(&base_url)
            .connect_timeout(Duration::from_secs(2))
            .stream_config(StreamConfig {
                idle_timeout: Duration::from_millis(150),
                reconnect: Reconnect::Enabled {
                    initial_backoff: Duration::from_millis(10),
                    max_backoff: Duration::from_millis(50),
                },
            })
            .build()
            .expect("client de test");
        let mut stream = client
            .intensity_stream(IntensityStreamOptions::default())
            .expect("construction du flux");

        let event = tokio::time::timeout(Duration::from_secs(3), stream.next())
            .await
            .expect("le flux doit produire un événement après reconnexion")
            .expect("flux non terminé")
            .expect("ok");
        assert!((event.intensity - 7.0).abs() < f64::EPSILON);
    }

    // --- Reconnexion désactivée : la coupure termine le flux ---------------

    #[tokio::test]
    async fn disabled_reconnect_ends_stream_on_drop() {
        async fn handler() -> Sse<impl Stream<Item = Result<AxumSseEvent, Infallible>>> {
            Sse::new(futures_util::stream::iter(vec![Ok(intensity_frame(
                "national", 3.0,
            ))]))
        }
        let app = Router::new().route("/v1/intensity/stream", get(handler));
        let (base_url, _server) = spawn_server(app).await;

        let client = CarbonFr::builder()
            .base_url(&base_url)
            .connect_timeout(Duration::from_secs(2))
            .stream_config(StreamConfig {
                idle_timeout: Duration::from_secs(5),
                reconnect: Reconnect::Disabled,
            })
            .build()
            .expect("client de test");
        let mut stream = client
            .intensity_stream(IntensityStreamOptions::default())
            .expect("construction du flux");

        let first = stream.next().await.expect("1er événement").expect("ok");
        assert!((first.intensity - 3.0).abs() < f64::EPSILON);

        // Le serveur ferme après le seul événement envoyé ; sans reconnexion,
        // le flux se termine (`None`), il ne relance jamais de requête.
        assert!(stream.next().await.is_none());
    }
}
