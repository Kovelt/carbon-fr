# carbonfr-eligibility

RFNBO / low-carbon hydrogen electrolyzer eligibility for
[carbon-fr](https://github.com/Kovelt/carbon-fr) ("Layer A", ADR-0025,
methodology ADR-0026): grid-level eligibility signals, evaluated per time
slot, under two explicitly labeled and neutral frameworks — `rfnbo`
(renewable) and `low-carbon` (nuclear/CCS-inclusive low-carbon).

[![crates.io](https://img.shields.io/crates/v/carbonfr-eligibility.svg)](https://crates.io/crates/carbonfr-eligibility)
[![docs.rs](https://docs.rs/carbonfr-eligibility/badge.svg)](https://docs.rs/carbonfr-eligibility)
[![MSRV 1.88](https://img.shields.io/badge/MSRV-1.88-blue.svg)](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0030-politique-publication-crates-io.md)

## What this crate is

`carbonfr-eligibility` evaluates, per time slot, whether the electricity
consumed by an electrolyzer could plausibly qualify as RFNBO or low-carbon
hydrogen feedstock, under one of several versioned, served rulesets
(`EligibilityRuleset`). It is a **pure domain crate**: no I/O (no
`serde`/`sqlx`/`axum`/`reqwest`), `time` rather than `chrono`, and its only
internal dependency is [`carbonfr-core`](https://docs.rs/carbonfr-core) for
`CarbonIntensity` and related domain types. Evaluation is **total**: it never
returns a `Result` — indetermination is a first-class outcome, carried by
`EligibilitySignal::Indeterminate`.

**Neutrality (ADR-0025, cardinal rule)**: this crate reports eligibility
*with respect to each framework*; it does not adjudicate the
renewable-vs-nuclear debate or declare a "color" for hydrogen. The underlying
data is open and verifiable; conclusions belong to the caller.

**Scope**: decision support from grid-level signals — **not** a
certification. Site-level data (actual gCO₂eq/kgH₂, contractual
additionality via PPA) is out of reach and out of scope (see below).

## Minimal example

```rust
use carbonfr_core::domain::CarbonIntensity;
use carbonfr_eligibility::{
    EligibilityFramework, FR_BIDDING_ZONE, IndeterminateReason, SlotInput, evaluate_slot,
    resolve_ruleset,
};
use time::OffsetDateTime;

let ruleset = resolve_ruleset(EligibilityFramework::Rfnbo, None)
    .expect("the RFNBO framework has a served ruleset");
let slot = SlotInput {
    at: OffsetDateTime::now_utc(),
    intensity: CarbonIntensity::new(40.0).expect("finite, non-negative value"),
    intensity_lower: CarbonIntensity::new(35.0).expect("finite, non-negative value"),
    intensity_upper: CarbonIntensity::new(45.0).expect("finite, non-negative value"),
    renewable_share: None,
    renewable_share_gap: IndeterminateReason::MissingData,
    spot_price_eur_mwh: None,
};
let verdict = evaluate_slot(&slot, &ruleset, FR_BIDDING_ZONE);
println!("eligible: {}", verdict.eligible);
```

This snippet compiles against this crate as-is (manually verified against the
crate; not wired into `cargo test` doctests as this README isn't included via
`#[doc = include_str!(...)]`).

## Third-party types on the public API

- **[`time`](https://docs.rs/time)** — `OffsetDateTime` for slot timestamps
  (`SlotInput::at`, `EligibilityVerdict::timestamp`), `Date` for ruleset dates
  (`EligibilityRuleset::hourly_switchover`) and `Duration` for forecast steps
  and horizons (`ShareClimatology::build`, `ShareHorizonError::horizon`,
  `ShareMeteoHorizonError::horizon`).

This crate does **not** use `async-trait`: it defines no ports of its own
(all traits live in `carbonfr-core`).

## Out of scope

- **I/O of any kind** — no HTTP, SQL, filesystem, or network. Adapters that
  feed this crate real spot prices, mixes, or forecasts are separate,
  unpublished crates of the `carbon-fr` workspace.
- **Site-level data** — actual gCO₂eq/kgH₂, certification, contractual
  additionality (PPA). This crate only reasons about **grid-level** signals.
- **SQL migrations**, **`/hydrogene` map data** (ADR-0029), **adapters and
  the composition root** (`bin/server`) — same as `carbonfr-core`, none of
  it lives here or is referenced from here.

## MSRV

**Rust 1.88.0**, inherited from the workspace and driven by the same direct
dependency (`time`) as `carbonfr-core`. Raising the MSRV is a *minor* version
bump, never a patch, and only when a direct dependency requires it.

## SemVer policy (pre-1.0)

This crate is `0.x`: per [ADR-0030](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0030-politique-publication-crates-io.md) §3,
a breaking change (removing/renaming a public item, adding a variant to an
exhaustive enum, adding a field to a struct with public fields) is a **minor**
bump (`0.x → 0.(x+1)`), never a patch.

Public enums are classified one by one:

- **`#[non_exhaustive]`**: `IndeterminateReason`, `Pillar`,
  `EligibilityFramework`, `RulesetStatus`, `EligibilitySignal` — catalogs
  expected to grow as new regulatory signals or ruleset statuses are added.
- **Exhaustive**: `TemporalGranularity` (the hourly/monthly split is fixed by
  ADR-0026), `ShareSource` (`Observed`/`Forecast`, binary by construction) —
  extending either would itself be a methodology change requiring a new ADR.

Structs with public fields (`SlotInput`, `EligibilityVerdict`,
`EligibilityRuleset`…) keep their fields public and are never retrofitted
with `#[non_exhaustive]`: they're plain data, meant to be constructed by
literal. Adding a public field to one of them is a minor bump too.

No optional features exist yet (no `serde` feature before 1.0 — ADR-0030 §6).

## License

Dual-licensed under [MIT](https://github.com/Kovelt/carbon-fr/blob/main/LICENSE-MIT)
or [Apache-2.0](https://github.com/Kovelt/carbon-fr/blob/main/LICENSE-APACHE),
at your option.

## Links

- Repository: <https://github.com/Kovelt/carbon-fr>
- Publication policy: [ADR-0030](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0030-politique-publication-crates-io.md)
- Eligibility methodology: [ADR-0025](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0025-extension-hydrogene-carbon-aware.md), [ADR-0026](https://github.com/Kovelt/carbon-fr/blob/main/docs/adr/0026-methodologie-overlays-eligibilite.md)
