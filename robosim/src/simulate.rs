//! The orchestrator: `run_state()` walks the netlist graph and produces a
//! `RunState` — event-driven, no timestep, recomputed fresh on every input
//! change. This is the designer's answer to "what does this DO right now" —
//! click the switch, the LED lights; set a throttle, the motor spins — the
//! human standing in for the not-yet-written reflex-kernel firmware, exactly
//! like a bench technician manually driving test points with a bench supply
//! and probes. (When a real firmware emulator arrives, it plugs in at the
//! same seam: it would drive `RunInputs` instead of a human, and everything
//! downstream — the reachability graph, the Ohm's-law current model — is
//! unchanged.)
//!
//! Reuses the same rule logic the E-checks already encode
//! (`led_current_limited`/`motor_output_pin`/`bus_final_addresses` via the
//! `led`/`motor`/`sensor` component modules) rather than re-deriving it, so
//! a check and a run-mode projection can never disagree.
//!
//! Edge cases (documented, not hidden): no battery instance -> everything
//! reads dark/dead, no crash. Multiple batteries -> seeds are unioned; v1
//! does not model electrically-isolated multi-battery domains separately. A
//! switch/button/resistor/wiring instance with pin count != 2 is skipped for
//! bridging (multi-pole parts unsupported in v1) with a `reason` rather than
//! a hard error. Any instance with no `elec` block (e.g. `wheel`) is omitted
//! from `RunState.instances` entirely.

use crate::electrical::{instance_powered, pin_net_by_role, resolve_voltages};
use crate::graph::{bfs, endpoint_net_index, link, link_forward, reach_from};
use crate::types::{InstanceRunState, NetRunState, PwmChannel, RunInputs, RunState};
use robowire::catalogue::ElecCatalogue;
use robowire::schema::Netlist;
use std::collections::{BTreeMap, BTreeSet};

pub fn run_state(nl: &Netlist, cat: &ElecCatalogue, inputs: &RunInputs) -> Result<RunState, String> {
    let net_of = endpoint_net_index(nl);

    // Phase 1: undirected bridge edges (switch/button user-gated; resistor/
    // wiring always bridge) + collect each instance's power_in/power_out nets
    // for phase 3's passthrough edges (gating on grounding needs `grounded`
    // computed first, so we only stage the candidates here). `undirected_zero_r`
    // is the same bridges MINUS resistors — switch/button/wiring are ideal
    // (lossless) connections that carry a net's voltage across unchanged;
    // a resistor is a real drop and must not (see `resolve_voltages`).
    let mut undirected: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut undirected_zero_r: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut passthrough_candidates: Vec<(Vec<String>, Vec<String>, Option<String>)> = Vec::new();

    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };

        if matches!(
            part.kind.as_str(),
            "switch" | "button" | "resistor" | "potentiometer" | "wiring" | "fuse" | "ptc" | "connector"
        ) {
            let pin_names: Vec<&String> = elec.pins.keys().collect();
            if pin_names.len() == 2 {
                let closed = match part.kind.as_str() {
                    "switch" => inputs.switches.get(inst).copied().unwrap_or(false),
                    "button" => inputs.buttons.get(inst).copied().unwrap_or(false),
                    _ => true, // resistor/potentiometer/wiring/fuse/ptc/connector: always conducts
                };
                // Zero-resistance (ideal) kinds carry a net's voltage across
                // unchanged (resolve_voltages); resistor/potentiometer are a
                // real drop and must not. A fuse/PTC/connector's own
                // resistance is negligible next to what this model tracks —
                // same approximation as wiring.
                let zero_resistance =
                    matches!(part.kind.as_str(), "switch" | "button" | "wiring" | "fuse" | "ptc" | "connector");
                if closed {
                    let e0 = format!("{inst}.{}", pin_names[0]);
                    let e1 = format!("{inst}.{}", pin_names[1]);
                    if let (Some(n0), Some(n1)) = (net_of.get(&e0), net_of.get(&e1)) {
                        link(&mut undirected, n0, n1);
                        if zero_resistance {
                            link(&mut undirected_zero_r, n0, n1);
                        }
                    }
                }
            }
        }

        let in_nets: Vec<String> = elec
            .pins
            .iter()
            .filter(|(_, d)| d.role == "power_in")
            .filter_map(|(p, _)| net_of.get(&format!("{inst}.{p}")).cloned())
            .collect();
        let out_nets: Vec<String> = elec
            .pins
            .iter()
            .filter(|(_, d)| d.role == "power_out")
            .filter_map(|(p, _)| net_of.get(&format!("{inst}.{p}")).cloned())
            .collect();
        if !in_nets.is_empty() && !out_nets.is_empty() {
            let gnd_net = pin_net_by_role(elec, inst, &net_of, "gnd");
            passthrough_candidates.push((in_nets, out_nets, gnd_net));
        }
    }

    // Phase 2: battery seeds, then the ground plane (independent of passthrough
    // gating — a shared gnd net doesn't route through any power_in/power_out).
    let mut hot_seed: BTreeSet<String> = BTreeSet::new();
    let mut gnd_seed: BTreeSet<String> = BTreeSet::new();
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };
        if elec.source.is_none() {
            continue;
        }
        for (pin, decl) in &elec.pins {
            let Some(net) = net_of.get(&format!("{inst}.{pin}")) else { continue };
            match decl.role.as_str() {
                "pos" => {
                    hot_seed.insert(net.clone());
                }
                "gnd" => {
                    gnd_seed.insert(net.clone());
                }
                _ => {}
            }
        }
    }
    let grounded = bfs(&gnd_seed, &undirected, None);

    // Phase 3: passthrough edges, gated on each instance's own ground actually
    // being connected — an ungrounded regulator doesn't really pass power on.
    let mut forward: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (in_nets, out_nets, gnd_net) in &passthrough_candidates {
        let grounded_ok = gnd_net.as_ref().map_or(true, |g| grounded.contains(g));
        if !grounded_ok {
            continue;
        }
        for i in in_nets {
            for o in out_nets {
                link_forward(&mut forward, i, o);
            }
        }
    }
    let hot = bfs(&hot_seed, &undirected, Some(&forward));

    // Voltage resolution: an undeclared intermediate net (e.g. the wire
    // between a closed switch and the next component) inherits its voltage
    // from whatever it's ideally (losslessly) connected to, rather than
    // reading as 0V — see `resolve_voltages`.
    let mut resolved_volts = resolve_voltages(nl, &undirected_zero_r);

    // Phase 4: per-instance projection AND current sinks, in one pass — every
    // current figure is Ohm's law against the ACTUAL live voltage the
    // component sees (module docs), computed once here and reused both for
    // this instance's own `current_a` and for the net-level Σ below.
    let mut instances: BTreeMap<String, InstanceRunState> = BTreeMap::new();
    let mut sinks: Vec<(String, f64)> = Vec::new();

    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };
        let mut st = InstanceRunState::default();

        if matches!(
            part.kind.as_str(),
            "battery" | "regulator" | "esc" | "mcu" | "tof" | "imu" | "light" | "env" | "radio" | "buzzer" | "servo"
                | "charge-controller"
        ) {
            st.powered = Some(instance_powered(nl, cat, &net_of, &hot, &grounded, inst)?);
        }
        let powered = st.powered.unwrap_or(false);

        match part.kind.as_str() {
            "switch" => st.closed = Some(inputs.switches.get(inst).copied().unwrap_or(false)),
            "button" => st.closed = Some(inputs.buttons.get(inst).copied().unwrap_or(false)),
            "regulator" | "esc" | "mcu" | "radio" | "buzzer" | "servo" | "charge-controller" => {
                let (fp_state, sink) =
                    crate::fixed_power::compute(&net_of, &hot, &resolved_volts, elec, part, inst, powered);
                st = fp_state;
                if let Some(s) = sink {
                    sinks.push(s);
                }
            }
            "tof" | "imu" | "light" | "env" => {
                let (sensor_state, sink) =
                    crate::sensor::compute(nl, &net_of, &hot, &resolved_volts, inputs, inst, part, elec, powered);
                st = sensor_state;
                if let Some(s) = sink {
                    sinks.push(s);
                }
            }
            "led" => {
                let (led_state, sink) =
                    crate::led::compute(nl, cat, &net_of, &hot, &grounded, &mut resolved_volts, inputs, inst, part, elec)?;
                st = led_state;
                if let Some(s) = sink {
                    sinks.push(s);
                }
            }
            "motor" => {
                let (motor_state, motor_sinks) =
                    crate::motor::compute(nl, cat, &net_of, &hot, &grounded, &resolved_volts, inputs, inst, part, elec)?;
                st = motor_state;
                if let Some(s) = motor_sinks {
                    sinks.extend(s);
                }
            }
            _ => {}
        }

        // An MCU is otherwise a plain fixed_power sink — this is the one
        // thing distinct about it: which of its own pins actually drive
        // something (an ESC channel, a servo), for the run panel to render
        // a slider on the MCU's row per real signal path rather than one
        // hardcoded to "throttle".
        if part.kind == "mcu" {
            st.pwm_channels = Some(
                robowire::signal::mcu_drivable_pins(nl, cat, inst)?
                    .into_iter()
                    .map(|(pin, drives)| PwmChannel { pin, drives })
                    .collect(),
            );
        }

        instances.insert(inst.clone(), st);
    }

    // Phase 5: nets — hot/grounded/volts as before. `amps` is NOT gated on
    // `hot`: a motor's own terminal wires carry its full current while being
    // PWM-driven by the ESC, not steadily connected to the positive rail, so
    // they're never `hot` in the reachability sense — but current still
    // flows there and must show. `reach_from` always includes the net
    // itself, so a sink attributed directly to a rail-disconnected leaf net
    // (a motor terminal) is still counted even with nothing else reachable.
    let mut nets: BTreeMap<String, NetRunState> = BTreeMap::new();
    for net in &nl.nets {
        let is_hot = hot.contains(&net.id);
        let reach = reach_from(&net.id, &undirected, &forward);
        let amps: f64 = sinks.iter().filter(|(n, _)| reach.contains(n)).map(|(_, a)| a).sum();
        // Live: `hot` (on the switched rail graph) OR actually carrying
        // current — the latter covers legitimately-live nets that `hot`
        // doesn't reach: a motor's PWM-driven terminals, a lit LED's own
        // (resistor-isolated) anode net.
        let live = is_hot || amps > 0.0;
        let wire_drop_v = if amps > 0.0 {
            robowire::wire::net_resistance_ohms(net).map(|r| amps * r)
        } else {
            None
        };
        nets.insert(
            net.id.clone(),
            NetRunState {
                hot: is_hot,
                grounded: grounded.contains(&net.id),
                volts: if live { resolved_volts.get(&net.id).copied().unwrap_or(0.0) } else { 0.0 },
                amps,
                wire_drop_v,
            },
        );
    }

    // Phase 6: battery finalization — its own current is only knowable now
    // that `nets` (and its Σ of every sink) is built.
    crate::battery::finalize(nl, cat, &net_of, &nets, &mut instances)?;

    Ok(RunState { nets, instances })
}
