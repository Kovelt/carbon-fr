//! # carbonfr-core
//!
//! Cœur métier de **carbon-fr** : domaine, cas d'usage et ports.
//!
//! **Aucune IO ici** (règle d'or, ADR-0002) : ce crate ne dépend ni de HTTP,
//! ni de SQL, ni d'un runtime. Les adapters (crates séparés) implémentent les
//! ports définis dans [`ports`], et la composition root (`bin/server`) les
//! assemble.

pub mod application;
pub mod domain;
pub mod ports;
