//! robosim — the live component simulator: given a robowire netlist +
//! catalogue and the current switch/button/throttle/sensor inputs, what is
//! every net and every component actually doing right now (specs/robowire.md
//! §3a)? Deliberately separate from `robowire` (which owns netlist authoring
//! and static legality checks): this crate owns none of that, only the
//! live/interactive projection, so it can gain other consumers later
//! (arena-plant, a future real firmware emulator) without dragging in the
//! designer or the E-check toolchain.
//!
//! - `graph` — generic net-reachability (BFS over undirected/directed edges),
//!   no electrical meaning of its own.
//! - `electrical` — real-component Ohm's-law math (resistor `ohms`, LED
//!   `forward_v`, motor winding resistance, fixed-power equivalent
//!   resistance) — current is always derived from an actual live voltage,
//!   never a fixed lookup number.
//! - `simulate` — `run_state()`, the orchestrator that walks the graph and
//!   produces a `RunState`.
//! - `types` — the public input/output shapes.

pub mod electrical;
pub mod graph;
pub mod simulate;
pub mod types;

pub use simulate::run_state;
pub use types::{InstanceRunState, NetRunState, RunInputs, RunState};
