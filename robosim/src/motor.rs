//! Motor component behavior — resolving which ESC channel drives a motor,
//! whether that channel is actually powered, and the resulting spin/current
//! — factored out of `simulate.rs`'s dispatch match for the same reason
//! `led.rs` was: one place per component's own rules, not a growing inline
//! arm shared with every other kind.
//!
//! `powered` is new here (motors weren't in `simulate.rs`'s old generic
//! "powered" kind list at all — a motor has no `power_in`/`gnd` pin pair of
//! its own, only `motor_in` terminals, so the generic `instance_powered`
//! check doesn't apply to it directly). It reports whether the DRIVING ESC
//! channel is live, independent of throttle/spin — a motor sitting at zero
//! throttle on a powered rail is a different, distinguishable state from
//! one with no power reaching it at all, the same distinction switch/LED
//! already draw (`closed`, `lit`) that a bare spin-tick (which only shows
//! while actually spinning) doesn't.

use crate::electrical::{instance_powered, motor_winding_ohms, net_volts_live, pin_net_by_role};
use crate::types::{InstanceRunState, RunInputs};
use robowire::catalogue::{Elec, ElecCatalogue, ElecPart};
use robowire::checks::motor_output_pin;
use robowire::schema::{split_pin, Netlist};
use robowire::signal::motor_signal_source_pin;
use std::collections::{BTreeMap, BTreeSet};

pub fn compute(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    hot: &BTreeSet<String>,
    grounded: &BTreeSet<String>,
    resolved_volts: &BTreeMap<String, f64>,
    inputs: &RunInputs,
    inst: &str,
    part: &ElecPart,
    elec: &Elec,
) -> Result<(InstanceRunState, Option<Vec<(String, f64)>>), String> {
    let motor_pins: Vec<String> = elec.pins.iter().filter(|(_, d)| d.role == "motor_in").map(|(p, _)| p.clone()).collect();

    let mut powered = None;
    let mut spin = 0.0;
    let mut amps = 0.0;
    let mut reason = None;
    let mut sinks: Option<Vec<(String, f64)>> = None;

    if let Some(pin) = motor_pins.first() {
        let terminal = format!("{inst}.{pin}");
        match motor_output_pin(nl, cat, &terminal)? {
            Some(driver_pin) => {
                let (esc_inst, _) = split_pin(&driver_pin)?;
                let esc_powered = instance_powered(nl, cat, net_of, hot, grounded, esc_inst)?;
                powered = Some(esc_powered);
                if esc_powered {
                    // Throttle comes from whichever MCU pin actually reaches
                    // this channel's signal-in pin (harness/lessons/03-motor-driver.json
                    // has none yet, and correctly never spins as a result) —
                    // not pinned directly to the motor instance, so a wiring
                    // mistake between the brain and the ESC shows up here too.
                    let source_pin = motor_signal_source_pin(nl, cat, &terminal)?;
                    if source_pin.is_none() {
                        reason = Some("no signal source wired to this channel".to_string());
                    }
                    // Real RC/ESC convention: 1000µs = full reverse, 1500µs =
                    // neutral, 2000µs = full forward — the actual quantity a
                    // bench signal generator would be set to, not an
                    // abstract fraction. Converted to a throttle fraction
                    // once, here, for the Ohm's-law current math below.
                    let pulse_us = source_pin.as_ref().and_then(|p| inputs.pwm_signals.get(p).copied()).unwrap_or(1500.0);
                    let throttle = ((pulse_us - 1500.0) / 500.0).clamp(-1.0, 1.0);
                    spin = throttle;
                    let esc_part_id = nl.instances.get(esc_inst).ok_or_else(|| format!("unknown instance '{esc_inst}'"))?;
                    let (esc_part, _) = cat.get(esc_part_id)?;
                    if let Some(esc_elec) = &esc_part.elec {
                        if let Some(supply_net) = pin_net_by_role(esc_elec, esc_inst, net_of, "power_in") {
                            if let Some(r_winding) = motor_winding_ohms(part) {
                                let v_actual = net_volts_live(resolved_volts, hot, &supply_net);
                                amps = throttle.abs() * v_actual / r_winding;
                                if amps > 0.0 {
                                    let mut s = vec![(supply_net, amps)];
                                    // Also the motor's own terminal wires — the
                                    // same current visibly flows there too, not
                                    // just upstream at the battery.
                                    for p in &motor_pins {
                                        if let Some(n) = net_of.get(&format!("{inst}.{p}")) {
                                            s.push((n.clone(), amps));
                                        }
                                    }
                                    sinks = Some(s);
                                }
                            }
                        }
                    }
                } else {
                    reason = Some("driver channel unpowered".to_string());
                }
            }
            None => reason = Some("no single driver channel resolves (see E01)".to_string()),
        }
    }

    let mut state = InstanceRunState::default();
    state.powered = powered;
    state.spin = Some(spin);
    state.current_a = Some(amps);
    state.reason = reason;

    Ok((state, sinks))
}
