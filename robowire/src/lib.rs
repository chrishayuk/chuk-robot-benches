//! robowire — the electrical truth (specs/robowire.md, M0 scope).
//!
//! Modular: `schema` (netlist types), `catalogue` (the electrical view of
//! the shared parts/ files), `checks` (E-codes, individually callable).
//! The netlist's content hash is RobotSpec's `elec` source hash.
//!
//! Interactive run mode (§3a) — "click the switch, the LED lights" — is NOT
//! owned here: it lives in the standalone `robosim` crate (netlist +
//! catalogue in, live per-net/per-instance state out), kept separate so it
//! can gain consumers beyond robowire's designer without coupling them to
//! netlist authoring or the E-check toolchain.

pub mod catalogue;
pub mod checks;
pub mod graph;
pub mod power;
pub mod power_graph;
pub mod prose;
pub mod render;
pub mod schema;
pub mod signal;
pub mod teach;
pub mod view;
pub mod wire;

pub const ROBOWIRE_VERSION: &str = "0.1.0-m0";

pub use catalogue::{ElecCatalogue, ElecPart};
pub use checks::{run_checks, CheckResult, Tier};
pub use schema::Netlist;
