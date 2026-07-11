//! robowire — the electrical truth (specs/robowire.md, M0 scope).
//!
//! Modular: `schema` (netlist types), `catalogue` (the electrical view of
//! the shared parts/ files), `checks` (E-codes, individually callable).
//! The netlist's content hash is RobotSpec's `elec` source hash.

pub mod catalogue;
pub mod checks;
pub mod prose;
pub mod render;
pub mod schema;
pub mod view;

pub const ROBOWIRE_VERSION: &str = "0.1.0-m0";

pub use catalogue::{ElecCatalogue, ElecPart};
pub use checks::{run_checks, CheckResult};
pub use schema::Netlist;
