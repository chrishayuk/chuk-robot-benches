//! robotspec — the robot, as data (specs/robotspec.md, M0 scope).
//!
//! Modular by design: `schema` (authored types), `catalogue` (content-hashed
//! parts), `geom` (planar primitives), `derive` (the pipeline, stage by
//! stage), `checks` (D/X codes, individually callable), `identity` (body ⊂
//! robot hashing). Consumers compose what they need — the viewer's ledger,
//! arena-plant's mass properties, and the MCP servers all link these same
//! modules (one derivation codebase, N consumers; viewer spec M1).

pub mod catalogue;
pub mod checks;
pub mod derive;
pub mod geom;
pub mod identity;
pub mod power;
pub mod schema;
pub mod view;

pub const ROBOTSPEC_VERSION: &str = "0.1.0-m0";
pub const SCHEMA_VERSION: &str = "robotspec-v0.1";
pub const DERIVATION_PIPELINE_VERSION: &str = "derive-v0.1.0-parametric";

pub(crate) const GRAVITY: f64 = 9.80665;
/// AWS antweight: 4in cube, 150g.
pub const CUBE_MM: f64 = 101.6;
pub const WEIGHT_LIMIT_G: f64 = 150.0;

pub use catalogue::{Catalogue, Part};
pub use checks::CheckResult;
pub use derive::{derive, DerivedRecord};
pub use power::{PowerChain, PowerGraph, PowerRail, WireSegment};
pub use schema::*;
