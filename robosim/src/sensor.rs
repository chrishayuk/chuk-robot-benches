//! Bus sensor (tof/imu) component behavior — the fake reading, bus-address
//! conflict detection, and current draw, factored out of `simulate.rs`'s
//! dispatch match for the same reason `led.rs`/`motor.rs` were.

use crate::electrical::fixed_power_draw;
use crate::types::{InstanceRunState, RunInputs};
use robowire::catalogue::{Elec, ElecPart};
use robowire::checks::bus_final_addresses;
use robowire::schema::Netlist;
use std::collections::{BTreeMap, BTreeSet};

#[allow(clippy::too_many_arguments)]
pub fn compute(
    nl: &Netlist,
    net_of: &BTreeMap<String, String>,
    hot: &BTreeSet<String>,
    resolved_volts: &BTreeMap<String, f64>,
    inputs: &RunInputs,
    inst: &str,
    part: &ElecPart,
    elec: &Elec,
    powered: bool,
) -> (InstanceRunState, Option<(String, f64)>) {
    let default_val = part.range_mm.unwrap_or(0.0);
    let value = inputs.sensor_values.get(inst).copied().unwrap_or(default_val);

    let mut bus_conflict = None;
    for bus in &nl.buses {
        if !bus.devices.iter().any(|d| &d.inst == inst) {
            continue;
        }
        let finals = bus_final_addresses(bus);
        if let Some(my_addr) = finals.get(inst) {
            bus_conflict = Some(finals.values().filter(|a| *a == my_addr).count() > 1);
        }
    }

    let (amps, sink) = fixed_power_draw(net_of, hot, resolved_volts, elec, part, inst, powered);

    let mut state = InstanceRunState::default();
    state.powered = Some(powered);
    state.value = Some(value);
    state.bus_conflict = bus_conflict;
    state.current_a = Some(amps);

    (state, sink)
}
