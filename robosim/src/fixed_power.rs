//! Fixed-power device behavior (regulator/esc/mcu/radio/buzzer/servo) — the
//! one kind-shape shared identically across six catalogue kinds: `powered`
//! is already resolved by the generic `instance_powered` check the
//! orchestrator runs first, so all that's left is the current draw.
//! Factored out of `simulate.rs`'s dispatch match for the same reason
//! `led.rs`/`motor.rs`/`sensor.rs` were, even though — unlike those — every
//! kind here is genuinely identical, not just similarly-shaped.

use crate::electrical::fixed_power_draw;
use crate::types::InstanceRunState;
use robowire::catalogue::{Elec, ElecPart};
use std::collections::{BTreeMap, BTreeSet};

pub fn compute(
    net_of: &BTreeMap<String, String>,
    hot: &BTreeSet<String>,
    resolved_volts: &BTreeMap<String, f64>,
    elec: &Elec,
    part: &ElecPart,
    inst: &str,
    powered: bool,
) -> (InstanceRunState, Option<(String, f64)>) {
    let (amps, sink) = fixed_power_draw(net_of, hot, resolved_volts, elec, part, inst, powered);
    let mut state = InstanceRunState::default();
    state.powered = Some(powered);
    state.current_a = Some(amps);
    (state, sink)
}
