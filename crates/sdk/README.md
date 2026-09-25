# carbonfr-sdk

Async Rust client for [carbon-fr](https://github.com/Kovelt/carbon-fr), the open API for
the carbon intensity of French electricity (gCO₂eq/kWh, based on RTE/éCO2mix open data via
[ODRÉ](https://odre.opendatasoft.com/)).

[![crates.io](https://img.shields.io/crates/v/carbonfr-sdk.svg)](https://crates.io/crates/carbonfr-sdk)
[![docs.rs](https://docs.rs/carbonfr-sdk/badge.svg)](https://docs.rs/carbonfr-sdk)
[![MSRV 1.88](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0030-politique-publication-crates-io.md)

Design rationale lives in [ADR-0031](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0031-conception-sdk-rust.md)
(client/TLS construction, error model, the SSE stream, versioning) and
[ADR-0030](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0030-politique-publication-crates-io.md)
(publication policy shared with `carbonfr-core`/`carbonfr-eligibility`).

## Installation

```sh
cargo add carbonfr-sdk
# for the live SSE stream too:
cargo add carbonfr-sdk --features stream
```

MSRV: **1.88.0** (same floor as the rest of the workspace's publishable crates, driven by
`time`, not by the 2024 edition alone — ADR-0030 §5). SemVer: **0.x, pre-1.0** — any breaking
change (removed/renamed public item, a field added to a public-field struct…) bumps the
*minor* version, never a patch.

## What this crate covers

Full parity with the 26 `/v1` operations of the TypeScript SDK (`@carbon-fr/sdk`, `health`/
`health_ready` excluded — infra probes, outside the versioned contract):

- **Client**: `CarbonFr` / `CarbonFrBuilder` — base URL, `Bearer` API key, configurable/
  disableable per-request REST timeout, configurable connect timeout, injectable
  `reqwest::Client`.
- **Errors**: `CarbonFrError` (`#[non_exhaustive]`) — typed transport/API/decode/timeout/
  config variants, RFC 9457 `application/problem+json` decoding (`ProblemDetails`).
- **Intensity**: `intensity_now`, `intensity_date`, `intensity_stats`, `below`.
- **Forecast & scheduling**: `forecast`, `greenest_window`, `schedule`, `schedule_slots`.
- **Mix, exchanges, weather, renewable**: `mix`, `exchanges`, `exchanges_history`, `weather`,
  `weather_history`, `renewable`.
- **Reference data**: `methodologies`, `factors`, `price`, `price_history`, `cost_reference`,
  `eligibility_rulesets`.
- **Visit stats**: `visit_stats`, `record_visit`.
- **Webhooks** (require an API key): `list_webhooks`, `create_webhook`, `delete_webhook`.
- **Live stream** (Cargo feature `stream`): `intensity_stream`, a named `IntensityStream`
  type implementing `futures_core::Stream<Item = Result<IntensityEvent, CarbonFrError>>` over
  `GET /v1/intensity/stream` (Server-Sent Events).

Not yet in this crate: `examples/` beyond the four shipped here, and the crate's own first
crates.io publication (tracked separately, `docs/plan-iterations.md` §I6) — this README is
kept accurate rather than describing a state that isn't reached yet.

## Examples

Runnable end to end (`examples/`, against the default hosted instance unless you set a
`base_url`):

```sh
cargo run -p carbonfr-sdk --example intensity_now
cargo run -p carbonfr-sdk --example mix_region
cargo run -p carbonfr-sdk --example api_error
cargo run -p carbonfr-sdk --example stream --features stream
```

- `intensity_now` — the simplest call, default options.
- `mix_region` — a parameterized call (`?region=bretagne`) via an options struct.
- `api_error` — typed error handling: matching on `CarbonFrError`, reading the RFC 9457
  `code`/`detail` from `ProblemDetails`.
- `stream` — the live SSE feed, first three events then stop.

## Minimal example

```rust,no_run
# async fn go() -> Result<(), carbonfr_sdk::CarbonFrError> {
use carbonfr_sdk::{CarbonFr, IntensityNowOptions};

let client = CarbonFr::builder().build()?;
let now = client.intensity_now(IntensityNowOptions::default()).await?;
println!("{} — {:.1} {}", now.region, now.intensity.value, now.intensity.unit);
# Ok(())
# }
```

## Errors

Every fallible call returns `Result<T, CarbonFrError>`. `CarbonFrError` is
`#[non_exhaustive]` (always end an external `match` with a `_`/catch-all arm): `Transport`
(connection/TLS/DNS/timeout), `Api { status, problem }` (a non-2xx response, RFC 9457 body
decoded into `ProblemDetails`, with a `problem+json`-shaped fallback if the body isn't
one), `Decode` (malformed JSON or SSE frame), `Config` (invalid client configuration —
also used for the webhook methods' pre-flight "an API key is required" check, so a caller
never pays a round trip for a call that was always going to 401), and `Timeout` (connect
timeout, or the SSE stream's read-idle timeout). `ProblemDetails::code()` is the **stable**
machine anchor to match on — `detail()` is a human message, never contractual.

The webhook methods (`list_webhooks`, `create_webhook`, `delete_webhook`) require an API key
(`CarbonFrBuilder::api_key`); calling one without a key configured fails immediately with
`CarbonFrError::Config`, before any network request is attempted.

## Timeouts and reconnection

- **REST calls**: a 30 s default timeout per request (connect + response), configurable
  (`CarbonFrBuilder::timeout`) and disableable (`CarbonFrBuilder::no_timeout`) — a deliberate
  departure from the TypeScript SDK, which has none.
- **The SSE stream** (`intensity_stream`, feature `stream`) has **no total timeout** — it is
  meant to run for as long as you keep polling it. It does have a connect timeout
  (`CarbonFrBuilder::connect_timeout`) and a read-idle timeout between messages
  (`StreamConfig::idle_timeout`, keep-alive frames included, default 45 s — well above the
  server's ~15 s keep-alive cadence) below which a live connection is never mistaken for a
  dead one.
- **Reconnection** is on by default (`StreamConfig::reconnect`, fixed-then-exponential
  backoff, capped delay, unbounded attempts) and configurable/disableable. It never uses
  `Last-Event-ID`: the server doesn't emit one, and its source (an ephemeral in-process
  broadcast channel) couldn't replay missed updates anyway. **Events can be lost during an
  outage** — a reconnect resumes the live feed, it does not backfill what was missed while
  disconnected. Reconnection is silent: on success, `Stream::next()` simply yields the next
  event, with no intermediate `Err`.

## TLS: no reliance on a process-wide crypto provider default

`reqwest` 0.13 panics at client construction if no crypto provider is installed. Every
other crate in this workspace (`bin/server`, the HTTP/webhook adapters) installs `ring` as
the **process-wide** default via `CryptoProvider::install_default()` at bootstrap. This SDK
does the opposite on purpose: it builds its own `rustls::ClientConfig` with the `ring`
provider passed **explicitly**
(`ClientConfig::builder_with_provider` + `rustls_platform_verifier::BuilderVerifierExt::with_platform_verifier`),
fed to `reqwest` via `ClientBuilder::tls_backend_preconfigured`. It never reads nor installs
`CryptoProvider::get_default()` — safe to use as a dependency inside a host application
that installs a different provider, or none at all until its own first TLS client is built.

Need different TLS behavior (a corporate proxy, a different provider, mocking)?
`CarbonFrBuilder::http_client` accepts an already-built `reqwest::Client` — you then own its
TLS configuration entirely; the SDK only adds per-request headers (`User-Agent`,
`Authorization`) and the REST timeout on top.

## Cargo features

- default: REST client + typed errors only.
- `stream`: adds `CarbonFr::intensity_stream` (pulls in `eventsource-stream`,
  `reqwest/stream`, `futures-core`/`futures-util`, and `tokio`'s `time` feature for the
  read-idle timeout and reconnection backoff).

## No dependency on `carbonfr-core`

Request/response types are this crate's own (as the TypeScript SDK already does), and small
domain enums (`Region`, `Methodology`, `Estimator`, `EligibilityFramework`, `Interval`,
`FlowDirection`, `ThresholdDirection`, `WebhookStatus`, `EligibilitySignalVerdict`,
`EligibilitySignalProvenance`…) are duplicated with their own (de)serialization, following
the HTTP contract rather than the internal domain model
(`docs/adr/0031-conception-sdk-rust.md` decision 5). `Region` and
`EligibilitySignalProvenance` stay open (`#[non_exhaustive]` + an `Other`/catch-all variant)
since their catalog can grow server-side before the next SDK release; the rest are closed —
finite by construction of the methodology/model they describe.

Response types have **public fields and are never `#[non_exhaustive]`**: they are plain data,
constructible by literal in your own code and tests, consistent with how `carbonfr-core`/
`carbonfr-eligibility` treat their own public-field structs (ADR-0030 §3). Adding a public
field to one of them is a *minor* version bump, never a silent one.

## Third-party types on the public API

- **[`time`](https://docs.rs/time)** — `OffsetDateTime` for every genuinely RFC 3339 field
  (timestamps, `from`/`to`, `start`/`end`…), via the `serde-well-known` feature. A handful of
  fields that look like dates but aren't full RFC 3339 instants (a revision label, a
  regulatory vintage like `2026-H2`, a date-only field such as an eligibility ruleset's
  `hourly_switchover` or the visit stats' `since`) stay plain `String` on purpose — see the
  module docs on `carbonfr_sdk::HistoryPoint` and friends for exactly which.
- **[`reqwest`](https://docs.rs/reqwest)** / **[`rustls`](https://docs.rs/rustls)** — an
  already-built `reqwest::Client` can be injected via `CarbonFrBuilder::http_client`, in
  which case the caller owns its TLS configuration. `reqwest::Error` is also exposed as-is
  through `CarbonFrError::Transport`: a major `reqwest` upgrade is therefore a SemVer-relevant
  change for `carbonfr-sdk` (a *minor* bump while it is `0.x`).
- **[`futures-core`](https://docs.rs/futures-core)** (feature `stream`) — `IntensityStream`
  implements `futures_core::Stream`; consume it with `futures_util::StreamExt::next` or any
  equivalent combinator.

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/Kovelt/carbon-fr/blob/main/LICENSE-APACHE) or
[MIT license](https://github.com/Kovelt/carbon-fr/blob/main/LICENSE-MIT) at your option.
