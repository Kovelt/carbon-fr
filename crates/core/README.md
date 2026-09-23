# carbonfr-core

Domain, use cases and ports for [carbon-fr](https://github.com/Kovelt/carbon-fr), the
open API for the carbon intensity of French electricity (gCO₂eq/kWh, based on
RTE/éCO2mix open data via [ODRÉ](https://odre.opendatasoft.com/)).

> Not yet published to crates.io. This README documents the crate as prepared
> for publication (see [ADR-0030](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0030-politique-publication-crates-io.md)).
> No `crates.io`/`docs.rs` badge until the first version actually ships.

## What this crate is

`carbonfr-core` is the hexagonal-architecture core of carbon-fr: domain types
(carbon intensity, measurements, generation mix, forecasts, cost/price
references, cross-border flows, webhooks…), use cases, and the ports
(`async_trait` traits) that adapters implement. It is **pure**: no I/O, no
HTTP client, no SQL, no async runtime — only computation over plain Rust
types. Adapters (HTTP, PostgreSQL, ODRÉ, ENTSO-E…) and the composition root
live in separate, unpublished crates of the same workspace.

## Minimal example

```rust
use carbonfr_core::domain::{CarbonIntensity, Measurement, Methodology, Region, Vintage};
use time::OffsetDateTime;

let intensity = CarbonIntensity::new(42.0).expect("finite, non-negative value");
let measurement = Measurement {
    at: OffsetDateTime::now_utc(),
    region: Region::National,
    intensity,
    methodology: Methodology::rte_direct(),
    vintage: Vintage::Tr,
    mix: None,
};
assert_eq!(measurement.region, Region::National);
```

This snippet compiles against this crate as-is (manually verified against the
crate; not wired into `cargo test` doctests as this README isn't included via
`#[doc = include_str!(...)]`).

## Third-party types on the public API

Kept and documented rather than hidden behind newtypes (ADR-0030 §3):

- **[`time`](https://docs.rs/time)** — `OffsetDateTime` for every timestamp
  in the domain (`Measurement::at`, forecast points, webhook scheduling…),
  `Duration` for spans and freshness bounds (`MAX_FLOW_CONTEXT_AGE`,
  `MAX_SPOT_STALENESS`, `ClimatologyParams::{step, tau}`, `HorizonBands`,
  `BackfillHistory::new`) and `Date` for day-level values (`VisitStats::since`).
  A consumer of this crate depends on `time` for these types.
- **[`async-trait`](https://docs.rs/async-trait)** — every port in
  [`ports`](https://docs.rs/carbonfr-core/latest/carbonfr_core/ports/) is
  `#[async_trait]`. Implementing a port from outside this crate (a custom
  adapter) requires the same macro, or hand-desugaring to
  `Pin<Box<dyn Future>>`.

## Out of scope

This crate never touches, and never will:

- **I/O of any kind** — HTTP, SQL, filesystem, network. That's the adapters'
  job (separate, unpublished crates of the `carbon-fr` workspace).
- **SQL migrations** — live in `crates/adapter-postgres/migrations/` in the
  main repository, not here.
- **`/hydrogene` map data** (ADR-0029) — a static dataset served by
  `bin/server`, unrelated to this crate's domain.
- **Adapters and the composition root** (`bin/server`) — never published;
  they're coupled to `axum`/`sqlx`/`reqwest` and the deployment shape.

## MSRV

**Rust 1.88.0**, set by a direct dependency (`time`), not by the 2024 edition
alone. Raising the MSRV is a *minor* version bump (see below), never a patch,
and only when a direct dependency requires it.

## SemVer policy (pre-1.0)

This crate is `0.x`: per [ADR-0030](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0030-politique-publication-crates-io.md) §3,
a breaking change (removing/renaming a public item, adding a variant to an
exhaustive enum, adding a field to a struct with public fields) is a **minor**
bump (`0.x → 0.(x+1)`), never a patch.

Public enums are classified one by one, not with a blanket rule:

- **`#[non_exhaustive]`** on enums that are open-ended catalogs expected to
  grow with the domain, the data, or the surrounding infrastructure (the five
  error enums, `ApiTier`, `Perimeter`, `CostSource`, `CostTechnology`,
  `CostBasis`, `PriceComponentKind`, `Filiere`, `Neighbor`).
- **Exhaustive** where the set is finite by construction of the domain
  (`Vintage`, `ThresholdDirection`, `Region`, `Granularity`,
  `WindowEstimator`) — extending one of these would itself be a methodology
  change requiring a new ADR, so there's nothing to protect against under
  SemVer.

Structs with public fields (the vast majority of the domain's data types:
`Measurement`, `GenerationMix`, `ForecastPoint`…) keep their fields public and
are never retrofitted with `#[non_exhaustive]`: they're plain data, meant to
be constructed by literal by downstream consumers (including the future Rust
SDK, ADR-0031). Adding a public field to one of them is a minor bump too.

No optional features exist yet (in particular, no `serde` feature before
1.0 — see ADR-0030 §6): adding one later is additive under SemVer, so there's
no reason to reserve it in advance.

## License

Dual-licensed under [MIT](https://github.com/Kovelt/carbon-fr/blob/main/LICENSE-MIT)
or [Apache-2.0](https://github.com/Kovelt/carbon-fr/blob/main/LICENSE-APACHE),
at your option.

## Links

- Repository: <https://github.com/Kovelt/carbon-fr>
- Publication policy: [ADR-0030](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0030-politique-publication-crates-io.md)
- Architecture: [`docs/ARCHITECTURE.md`](https://github.com/Kovelt/carbon-fr/blob/main/docs/ARCHITECTURE.md)
