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
//!   no electrical meaning of its own. Lives in `robowire` (its E30-E32 power
//!   budget checks need the identical engine, and robosim already depends on
//!   robowire, never the reverse) — re-exported here so `crate::graph::...`
//!   call sites in `electrical`/`simulate` are unaffected by the move.
//! - `electrical` — real-component Ohm's-law math (resistor `ohms`, LED
//!   `forward_v`, motor winding resistance, fixed-power equivalent
//!   resistance) — current is always derived from an actual live voltage,
//!   never a fixed lookup number.
//! - `led` — the LED component's own behavior (lit/current-limited/burned/
//!   reason), factored out of `simulate`'s dispatch so it's not one more
//!   inline arm in a growing match; other kinds gain their own sibling
//!   module the same way as they need more than a couple of lines.
//! - `motor` — the motor component's own behavior (driver-channel
//!   resolution, powered/spin/current_a/reason).
//! - `sensor` — the tof/imu/light/env component's own behavior (fake
//!   reading, bus-address conflict, current draw).
//! - `fixed_power` — the shared behavior of every kind whose current draw
//!   is simply `equiv_load_current` against its own `power_in` net when
//!   powered (regulator/esc/mcu/radio/buzzer/servo) — genuinely identical
//!   across all six, unlike the other component modules' merely similar
//!   shapes.
//! - `battery` — the battery's own finalization pass (terminal current +
//!   sag), run once the whole graph is built rather than per-instance —
//!   a battery's own current isn't knowable until every net's Σ is.
//! - `simulate` — `run_state()`, the orchestrator that walks the graph and
//!   dispatches to each kind's own component logic.
//! - `types` — the public input/output shapes.

pub mod battery;
pub mod electrical;
pub mod fixed_power;
pub mod led;
pub mod motor;
pub mod sensor;
pub mod simulate;
pub mod types;

pub use robowire::graph;

pub use simulate::run_state;
pub use types::{InstanceRunState, NetRunState, PwmChannel, RunInputs, RunState};
