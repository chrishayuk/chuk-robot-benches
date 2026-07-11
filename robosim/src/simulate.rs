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
//! (`led_current_limited`, `bus_final_addresses`, `motor_output_pin` in
//! `robowire::checks`) rather than re-deriving it, so a check and a run-mode
//! projection can never disagree.
//!
//! Edge cases (documented, not hidden): no battery instance -> everything
//! reads dark/dead, no crash. Multiple batteries -> seeds are unioned; v1
//! does not model electrically-isolated multi-battery domains separately. A
//! switch/button/resistor/wiring instance with pin count != 2 is skipped for
//! bridging (multi-pole parts unsupported in v1) with a `reason` rather than
//! a hard error. Any instance with no `elec` block (e.g. `wheel`) is omitted
//! from `RunState.instances` entirely.

use crate::electrical::{
    equiv_load_current, instance_powered, led_series_supply, motor_winding_ohms, net_volts_live,
    pin_net_by_role, resolve_voltages,
};
use crate::graph::{bfs, endpoint_net_index, link, link_forward, reach_from};
use crate::types::{InstanceRunState, NetRunState, RunInputs, RunState};
use robowire::catalogue::ElecCatalogue;
use robowire::checks::{bus_final_addresses, led_current_limited, motor_output_pin};
use robowire::schema::{split_pin, Netlist};
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

        if matches!(part.kind.as_str(), "switch" | "button" | "resistor" | "wiring") {
            let pin_names: Vec<&String> = elec.pins.keys().collect();
            if pin_names.len() == 2 {
                let closed = match part.kind.as_str() {
                    "switch" => inputs.switches.get(inst).copied().unwrap_or(false),
                    "button" => inputs.buttons.get(inst).copied().unwrap_or(false),
                    _ => true, // resistor / wiring: always conducts
                };
                if closed {
                    let e0 = format!("{inst}.{}", pin_names[0]);
                    let e1 = format!("{inst}.{}", pin_names[1]);
                    if let (Some(n0), Some(n1)) = (net_of.get(&e0), net_of.get(&e1)) {
                        link(&mut undirected, n0, n1);
                        if part.kind.as_str() != "resistor" {
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
            "battery" | "regulator" | "esc" | "mcu" | "tof" | "imu" | "radio" | "buzzer"
        ) {
            st.powered = Some(instance_powered(nl, cat, &net_of, &hot, &grounded, inst)?);
        }

        match part.kind.as_str() {
            "switch" => st.closed = Some(inputs.switches.get(inst).copied().unwrap_or(false)),
            "button" => st.closed = Some(inputs.buttons.get(inst).copied().unwrap_or(false)),
            "regulator" | "esc" | "mcu" | "radio" | "buzzer" => {
                let mut amps = 0.0;
                if st.powered == Some(true) {
                    if let Some(supply_net) = pin_net_by_role(elec, inst, &net_of, "power_in") {
                        let v_actual = net_volts_live(&resolved_volts, &hot, &supply_net);
                        amps = equiv_load_current(part, v_actual);
                        if amps > 0.0 {
                            sinks.push((supply_net, amps));
                        }
                    }
                }
                st.current_a = Some(amps);
            }
            "tof" | "imu" => {
                let default_val = part.range_mm.unwrap_or(0.0);
                st.value = Some(inputs.sensor_values.get(inst).copied().unwrap_or(default_val));
                for bus in &nl.buses {
                    if !bus.devices.iter().any(|d| &d.inst == inst) {
                        continue;
                    }
                    let finals = bus_final_addresses(bus);
                    if let Some(my_addr) = finals.get(inst) {
                        let conflict = finals.values().filter(|a| *a == my_addr).count() > 1;
                        st.bus_conflict = Some(conflict);
                    }
                }
                let mut amps = 0.0;
                if st.powered == Some(true) {
                    if let Some(supply_net) = pin_net_by_role(elec, inst, &net_of, "power_in") {
                        let v_actual = net_volts_live(&resolved_volts, &hot, &supply_net);
                        amps = equiv_load_current(part, v_actual);
                        if amps > 0.0 {
                            sinks.push((supply_net, amps));
                        }
                    }
                }
                st.current_a = Some(amps);
            }
            "led" => {
                let anode = pin_net_by_role(elec, inst, &net_of, "diode_a");
                let cathode = pin_net_by_role(elec, inst, &net_of, "diode_k");
                let anode_hot = anode.as_ref().is_some_and(|n| hot.contains(n));
                let cathode_grounded = cathode.as_ref().is_some_and(|n| grounded.contains(n));
                let lit = anode_hot && cathode_grounded;
                let limited = led_current_limited(nl, cat, inst)?;
                st.lit = Some(lit);
                st.current_limited = Some(limited);

                // A lit LED sustains its forward-voltage drop across itself —
                // resolve that onto its own anode net for display, since a
                // resistor bridge (unlike a switch/wire) never propagates
                // voltage. Without this, a lit, current-carrying LED would
                // show 0V on its own feed net (a resistor is a real boundary,
                // §3a), the same "current flowing, 0V shown" inconsistency
                // already fixed once for undeclared switch/button nets.
                if lit {
                    if let Some(anode_net) = &anode {
                        resolved_volts.entry(anode_net.clone()).or_insert(part.forward_v.unwrap_or(0.0));
                    }
                }

                let mut amps = 0.0;
                if lit && limited {
                    if let Some((supply_net, ohms)) = led_series_supply(nl, cat, &net_of, inst)? {
                        if ohms > 0.0 {
                            let v_supply = net_volts_live(&resolved_volts, &hot, &supply_net);
                            let vf = part.forward_v.unwrap_or(0.0);
                            amps = ((v_supply - vf) / ohms).max(0.0);
                        }
                    }
                    if amps > 0.0 {
                        if let Some(anode_net) = &anode {
                            sinks.push((anode_net.clone(), amps));
                        }
                    }
                }
                st.current_a = Some(amps);

                st.reason = if lit && !limited {
                    Some("no series resistor — would burn out instantly (E33)".to_string())
                } else if !lit {
                    let anode_grounded = anode.as_ref().is_some_and(|n| grounded.contains(n));
                    let cathode_hot = cathode.as_ref().is_some_and(|n| hot.contains(n));
                    if anode_grounded && cathode_hot {
                        Some("reverse polarity — anode is grounded, cathode is hot".to_string())
                    } else if !anode_hot {
                        Some("no power reaching the anode".to_string())
                    } else {
                        Some("cathode not returned to ground".to_string())
                    }
                } else {
                    None
                };
            }
            "motor" => {
                let throttle = inputs.throttles.get(inst).copied().unwrap_or(0.0).clamp(-1.0, 1.0);
                let motor_pins: Vec<String> =
                    elec.pins.iter().filter(|(_, d)| d.role == "motor_in").map(|(p, _)| p.clone()).collect();
                let mut spin = 0.0;
                let mut amps = 0.0;
                let mut reason = None;
                if let Some(pin) = motor_pins.first() {
                    let terminal = format!("{inst}.{pin}");
                    match motor_output_pin(nl, cat, &terminal)? {
                        Some(driver_pin) => {
                            let (esc_inst, _) = split_pin(&driver_pin)?;
                            if instance_powered(nl, cat, &net_of, &hot, &grounded, esc_inst)? {
                                spin = throttle;
                                let esc_part_id = nl
                                    .instances
                                    .get(esc_inst)
                                    .ok_or_else(|| format!("unknown instance '{esc_inst}'"))?;
                                let (esc_part, _) = cat.get(esc_part_id)?;
                                if let Some(esc_elec) = &esc_part.elec {
                                    if let Some(supply_net) =
                                        pin_net_by_role(esc_elec, esc_inst, &net_of, "power_in")
                                    {
                                        if let Some(r_winding) = motor_winding_ohms(part) {
                                            let v_actual = net_volts_live(&resolved_volts, &hot, &supply_net);
                                            amps = throttle.abs() * v_actual / r_winding;
                                            if amps > 0.0 {
                                                sinks.push((supply_net, amps));
                                                // Also the motor's own terminal wires — the
                                                // same current visibly flows there too, not
                                                // just upstream at the battery.
                                                for p in &motor_pins {
                                                    if let Some(n) = net_of.get(&format!("{inst}.{p}")) {
                                                        sinks.push((n.clone(), amps));
                                                    }
                                                }
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
                st.spin = Some(spin);
                st.current_a = Some(amps);
                st.reason = reason;
            }
            _ => {}
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
        nets.insert(
            net.id.clone(),
            NetRunState {
                hot: is_hot,
                grounded: grounded.contains(&net.id),
                volts: if live { resolved_volts.get(&net.id).copied().unwrap_or(0.0) } else { 0.0 },
                amps,
            },
        );
    }

    // Phase 6: battery's own current — the total on its own terminal net,
    // knowable only now that `nets` (and its Σ of every sink) is built.
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        if part.kind != "battery" {
            continue;
        }
        let Some(elec) = &part.elec else { continue };
        let pos_net = pin_net_by_role(elec, inst, &net_of, "pos");
        let amps = pos_net.and_then(|n| nets.get(&n)).map(|ns| ns.amps).unwrap_or(0.0);
        if let Some(st) = instances.get_mut(inst) {
            st.current_a = Some(amps);
        }
    }

    Ok(RunState { nets, instances })
}
