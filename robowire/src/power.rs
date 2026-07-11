//! Power-budget checks (E30, E31, E32 — specs/robowire.md §3 "power
//! budget", M1). Static, worst-case arithmetic, not a live simulator: every
//! switch/button/resistor/potentiometer bridge is treated as unconditionally
//! closed (the worst case a user could put it in), motors draw their
//! declared stall current, potentiometers sit at `ohms_min` (max-current
//! position) — there is no `RunInputs`/live-input concept here, unlike
//! `robosim`'s run mode.
//!
//! Reuses the moved `crate::graph` reachability engine. Deliberately does
//! NOT depend on `robosim` (robosim already depends on robowire — the other
//! way would be circular), so a handful of Ohm's-law helpers below are a
//! small, intentionally-separate re-implementation of `robosim::electrical`'s
//! live equivalents, not a shared import — the worst-case assumptions differ
//! enough (always-closed vs user-gated, stall current vs throttle-scaled,
//! `ohms_min` vs dial position) that this isn't really the same computation
//! wearing a different hat.
//!
//! `Err` is reserved strictly for malformed-netlist conditions (unresolvable
//! instance references) — "no battery found," "regulator missing `max_a`"
//! are `ok()`/`fail()` results, never `Err`, since `main.rs` aborts the
//! *entire* check run (killing every other E-code too) on any `Err` from
//! any check.

use crate::catalogue::{Elec, ElecCatalogue};
use crate::checks::{fail, motor_output_pin, ok, warn, CheckResult};
use crate::graph::{bfs, endpoint_net_index, link, link_forward, reach_from};
use crate::schema::{split_pin, Netlist};
use crate::wire;
use std::collections::{BTreeMap, BTreeSet};

/// The worst-case bridge graph: every switch/button/resistor/potentiometer/
/// wiring bridge unconditionally closed; every power_in->power_out
/// passthrough gated on the passthrough instance's own `gnd` pin actually
/// reaching the ground plane (an ungrounded regulator can't buffer anything,
/// worst case or not — mirrors `robosim::simulate`'s live passthrough gate).
pub(crate) struct WorstCaseGraph {
    /// All worst-case bridges (switch/button/resistor/potentiometer/wiring)
    /// — used for current reachability. Resistor/potentiometer bridge here
    /// (current still flows through a resistor) but not in `undirected_zero_r`
    /// (a resistor is a real voltage drop, not a lossless passthrough).
    pub(crate) undirected: BTreeMap<String, BTreeSet<String>>,
    /// Zero-resistance bridges only (switch/button/wiring) — used for
    /// declared-voltage propagation (`resolve_voltages`) and for E32's
    /// "same physical rail" test.
    pub(crate) undirected_zero_r: BTreeMap<String, BTreeSet<String>>,
    /// Directed power_in -> power_out edges (regulator/BEC/MCU-3V3-out),
    /// gated on the instance's own ground actually being connected.
    pub(crate) forward: BTreeMap<String, BTreeSet<String>>,
}

/// The net touching `inst`'s first pin declared with role `role`, if wired.
pub(crate) fn pin_net_by_role(
    elec: &Elec,
    inst: &str,
    net_of: &BTreeMap<String, String>,
    role: &str,
) -> Option<String> {
    elec.pins.iter().find(|(_, d)| d.role == role).and_then(|(p, _)| net_of.get(&format!("{inst}.{p}")).cloned())
}

pub(crate) fn build(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
) -> Result<WorstCaseGraph, String> {
    let mut undirected: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut undirected_zero_r: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut passthrough: Vec<(Vec<String>, Vec<String>, Option<String>)> = Vec::new();
    let mut gnd_seed: BTreeSet<String> = BTreeSet::new();

    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };

        if matches!(
            part.kind.as_str(),
            "switch" | "button" | "resistor" | "potentiometer" | "wiring" | "fuse" | "ptc" | "connector"
        ) {
            let pins: Vec<&String> = elec.pins.keys().collect();
            if pins.len() == 2 {
                let e0 = format!("{inst}.{}", pins[0]);
                let e1 = format!("{inst}.{}", pins[1]);
                if let (Some(n0), Some(n1)) = (net_of.get(&e0), net_of.get(&e1)) {
                    link(&mut undirected, n0, n1);
                    // Ideal (lossless, voltage-propagating) connections — a
                    // fuse/PTC/connector's own resistance is negligible next
                    // to what this model tracks, same approximation as wiring.
                    if matches!(part.kind.as_str(), "switch" | "button" | "wiring" | "fuse" | "ptc" | "connector") {
                        link(&mut undirected_zero_r, n0, n1);
                    }
                }
            }
        }

        if elec.source.is_some() {
            if let Some(g) = pin_net_by_role(elec, inst, net_of, "gnd") {
                gnd_seed.insert(g);
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
            let gnd_net = pin_net_by_role(elec, inst, net_of, "gnd");
            passthrough.push((in_nets, out_nets, gnd_net));
        }
    }

    let grounded = bfs(&gnd_seed, &undirected, None);

    let mut forward: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (in_nets, out_nets, gnd_net) in &passthrough {
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

    Ok(WorstCaseGraph { undirected, undirected_zero_r, forward })
}

/// Propagate each net's declared voltage across zero-resistance
/// (switch/button/wiring) connections only — same rationale as
/// `robosim::electrical::resolve_voltages`, standalone here since a
/// resistor/potentiometer is a real drop and must not inherit a neighbor's
/// voltage this way.
pub(crate) fn resolve_voltages(
    nl: &Netlist,
    undirected_zero_r: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, f64> {
    let mut resolved: BTreeMap<String, f64> = BTreeMap::new();
    for net in &nl.nets {
        let Some(v) = net.volts else { continue };
        if resolved.contains_key(&net.id) {
            continue;
        }
        for n in bfs(&BTreeSet::from([net.id.clone()]), undirected_zero_r, None) {
            resolved.entry(n).or_insert(v);
        }
    }
    resolved
}

/// The resistor-or-potentiometer in series with `led_inst`, at its
/// worst-case (max-current) resistance — a potentiometer at `ohms_min`, a
/// fixed resistor at its declared `ohms` — and the net feeding it. `None` if
/// no such limiter is structurally identifiable (E33 would already be
/// failing) or it declares no usable resistance.
fn led_series_supply(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    led_inst: &str,
) -> Result<Option<(String, f64)>, String> {
    let prefix = format!("{led_inst}.");
    for net in &nl.nets {
        if !net.pins.iter().any(|p| p.starts_with(&prefix)) {
            continue;
        }
        for p in &net.pins {
            let (other_inst, _) = split_pin(p)?;
            if other_inst == led_inst {
                continue;
            }
            let Some(other_part_id) = nl.instances.get(other_inst) else { continue };
            let (other_part, _) = cat.get(other_part_id)?;
            let ohms = match other_part.kind.as_str() {
                "resistor" => other_part.ohms,
                "potentiometer" => other_part.ohms_min,
                _ => None,
            };
            let Some(ohms) = ohms else { continue };
            let Some(other_elec) = &other_part.elec else { continue };
            for pin in other_elec.pins.keys() {
                let ep = format!("{other_inst}.{pin}");
                if net.pins.contains(&ep) {
                    continue; // the pin already on the LED's own net
                }
                if let Some(supply_net) = net_of.get(&ep) {
                    return Ok(Some((supply_net.clone(), ohms)));
                }
            }
        }
    }
    Ok(None)
}

/// Worst-case current sinks: (net, amps) pairs, summed by `reach_from` in
/// the checks below. A fixed-power kind (regulator/esc/mcu/radio/buzzer/
/// tof/imu/servo) draws its declared `current_ma` at its own `power_in` net
/// — a single representative operating point, not a true peak-current
/// figure (which doesn't exist in the catalogue yet), accepted as a
/// documented approximation. A motor draws its declared `stall_current_a`
/// (the true worst case — no Ohm's-law scaling needed) attributed to the
/// ESC driving it, not its own terminal net, since that's the net a rail
/// budget actually needs to see. An LED+resistor/potentiometer leg draws
/// `(v_supply - forward_v) / ohms` at its own anode net.
pub(crate) fn worst_case_sinks(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    resolved_volts: &BTreeMap<String, f64>,
) -> Result<Vec<(String, f64)>, String> {
    let mut sinks: Vec<(String, f64)> = Vec::new();

    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };

        match part.kind.as_str() {
            "regulator" | "esc" | "mcu" | "radio" | "buzzer" | "tof" | "imu" | "servo" => {
                if let Some(ma) = part.current_ma {
                    if let Some(net) = pin_net_by_role(elec, inst, net_of, "power_in") {
                        sinks.push((net, ma / 1000.0));
                    }
                }
            }
            "motor" => {
                let Some(mp) = &part.motor else { continue };
                let Some((pin, _)) = elec.pins.iter().find(|(_, d)| d.role == "motor_in") else { continue };
                let terminal = format!("{inst}.{pin}");
                let Some(driver_pin) = motor_output_pin(nl, cat, &terminal)? else { continue };
                let (esc_inst, _) = split_pin(&driver_pin)?;
                let Some(esc_part_id) = nl.instances.get(esc_inst) else { continue };
                let (esc_part, _) = cat.get(esc_part_id)?;
                let Some(esc_elec) = &esc_part.elec else { continue };
                if let Some(supply_net) = pin_net_by_role(esc_elec, esc_inst, net_of, "power_in") {
                    sinks.push((supply_net, mp.stall_current_a));
                }
                // Also the motor's own terminal wires — the same worst-case
                // current genuinely flows there too (mirrors
                // `robosim::simulate`'s identical attribution), so a thin
                // gauge declared on a motor leg is checkable by E31 even
                // though that wire never touches the ESC's supply net.
                for (p, d) in &elec.pins {
                    if d.role != "motor_in" {
                        continue;
                    }
                    if let Some(n) = net_of.get(&format!("{inst}.{p}")) {
                        sinks.push((n.clone(), mp.stall_current_a));
                    }
                }
            }
            "led" => {
                if let Some((supply_net, ohms)) = led_series_supply(nl, cat, net_of, inst)? {
                    if ohms > 0.0 {
                        let v = resolved_volts.get(&supply_net).copied().unwrap_or(0.0);
                        let vf = part.forward_v.unwrap_or(0.0);
                        let amps = ((v - vf) / ohms).max(0.0);
                        if amps > 0.0 {
                            if let Some(anode_net) = pin_net_by_role(elec, inst, net_of, "diode_a") {
                                sinks.push((anode_net, amps));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(sinks)
}

/// E30: per-rail worst-case draw vs source capability — a battery's Σ
/// worst-case downstream current vs `c_rating * capacity_mah`, and any
/// `power_out` pin's own declared `max_a` vs its Σ worst-case downstream
/// load.
fn e30_power_budget(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    g: &WorstCaseGraph,
    sinks: &[(String, f64)],
) -> Result<CheckResult, String> {
    const C: &str = "E30";
    const D: &str = "per-rail worst-case draw vs source capability (C-rating, regulator)";

    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };

        if let Some(source) = &elec.source {
            if let (Some(c_rating), Some(capacity_mah)) = (source.c_rating, source.capacity_mah) {
                if let Some(pos_net) = pin_net_by_role(elec, inst, net_of, "pos") {
                    let reach = reach_from(&pos_net, &g.undirected, &g.forward);
                    let total: f64 = sinks.iter().filter(|(n, _)| reach.contains(n)).map(|(_, a)| a).sum();
                    let cap_a = c_rating * capacity_mah / 1000.0;
                    if total > cap_a {
                        return Ok(fail(
                            C,
                            D,
                            format!(
                                "battery '{inst}': worst-case draw {total:.2}A exceeds its {c_rating}C x \
                                 {capacity_mah}mAh capacity ({cap_a:.2}A)"
                            ),
                        ));
                    }
                }
            }
        }

        for (pin, decl) in &elec.pins {
            if decl.role != "power_out" {
                continue;
            }
            let Some(max_a) = decl.max_a else { continue };
            let Some(out_net) = net_of.get(&format!("{inst}.{pin}")) else { continue };
            let reach = reach_from(out_net, &g.undirected, &g.forward);
            let total: f64 = sinks.iter().filter(|(n, _)| reach.contains(n)).map(|(_, a)| a).sum();
            if total > max_a {
                return Ok(fail(
                    C,
                    D,
                    format!("{inst}.{pin}: worst-case downstream draw {total:.2}A exceeds its rated {max_a:.2}A"),
                ));
            }
        }
    }

    Ok(ok(C, D, "every rail's worst-case draw fits its source's rated capacity".into()))
}

/// E31: wire gauge vs worst-case current per segment (ampacity table). Only
/// nets with a declared `gauge_awg` are checked. A fan-out net (more than 2
/// pins) gets one lumped gauge covering the whole net, including every
/// fanned-out branch — not new imprecision this check introduces (the whole
/// codebase already treats a `Net` as one equipotential node with one
/// aggregate current), but worth being explicit about here. Also covers the
/// spec's "connector ratings" clause: any `fuse`/`ptc`/`connector`-kind
/// instance with a declared `rated_a` is checked the same way, against the
/// worst-case current reaching either of its two (bridged, same-component)
/// pins.
fn e31_wire_ampacity(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    g: &WorstCaseGraph,
    sinks: &[(String, f64)],
) -> Result<CheckResult, String> {
    const C: &str = "E31";
    const D: &str = "wire gauge vs worst-case current per segment (ampacity table); connector/fuse ratings";

    for net in &nl.nets {
        let Some(awg) = net.gauge_awg else { continue };
        let Some(ampacity) = wire::awg_ampacity(awg) else { continue };
        let reach = reach_from(&net.id, &g.undirected, &g.forward);
        let total: f64 = sinks.iter().filter(|(n, _)| reach.contains(n)).map(|(_, a)| a).sum();
        if total > ampacity {
            return Ok(fail(
                C,
                D,
                format!(
                    "net '{}' ({awg}AWG, rated {ampacity:.2}A): worst-case current {total:.2}A exceeds ampacity \
                     (lumped across the whole net, including any fan-out — no per-segment topology)",
                    net.id
                ),
            ));
        }
    }

    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        if !matches!(part.kind.as_str(), "fuse" | "ptc" | "connector") {
            continue;
        }
        let Some(rated_a) = part.rated_a else { continue };
        let Some(elec) = &part.elec else { continue };
        let Some((pin, _)) = elec.pins.iter().next() else { continue };
        let Some(net_id) = net_of.get(&format!("{inst}.{pin}")) else { continue };
        let reach = reach_from(net_id, &g.undirected, &g.forward);
        let total: f64 = sinks.iter().filter(|(n, _)| reach.contains(n)).map(|(_, a)| a).sum();
        if total > rated_a {
            return Ok(fail(
                C,
                D,
                format!(
                    "{inst}: rated {rated_a:.2}A but the worst-case current reaching it is {total:.2}A"
                ),
            ));
        }
    }

    Ok(ok(C, D, "every gauge-declared net and every rated fuse/connector fits its worst-case current".into()))
}

/// E32 (warn-tier): MCU rail's exposure to motor-stall sag. Warns if any
/// MCU's `power_in` net is in the same zero-resistance-only reachability
/// component as any motor-driving ESC's own `power_in` net — i.e. no
/// regulator/BEC hop (a `forward` edge, always ground-gated by `build`)
/// buffers them, so they're electrically the same physical rail.
fn e32_brownout_topology(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    g: &WorstCaseGraph,
) -> Result<CheckResult, String> {
    const C: &str = "E32";
    const D: &str = "brownout topology: MCU rail exposure to motor-stall sag";

    let mcu_insts: Vec<&String> = nl
        .instances
        .iter()
        .map(|(inst, pid)| Ok::<_, String>((inst, cat.get(pid)?.0.kind == "mcu")))
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .filter(|(_, is_mcu)| *is_mcu)
        .map(|(inst, _)| inst)
        .collect();
    if mcu_insts.is_empty() {
        return Ok(ok(C, D, "no MCU present — not applicable".into()));
    }

    // ESCs that actually drive at least one motor.
    let mut motor_escs: BTreeSet<String> = BTreeSet::new();
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        if part.kind != "motor" {
            continue;
        }
        let Some(elec) = &part.elec else { continue };
        let Some((pin, _)) = elec.pins.iter().find(|(_, d)| d.role == "motor_in") else { continue };
        let terminal = format!("{inst}.{pin}");
        if let Some(driver_pin) = motor_output_pin(nl, cat, &terminal)? {
            let (esc_inst, _) = split_pin(&driver_pin)?;
            motor_escs.insert(esc_inst.to_string());
        }
    }

    for mcu_inst in &mcu_insts {
        let part_id = &nl.instances[*mcu_inst];
        let (mcu_part, _) = cat.get(part_id)?;
        let Some(mcu_elec) = &mcu_part.elec else { continue };
        let Some(mcu_supply) = pin_net_by_role(mcu_elec, mcu_inst, net_of, "power_in") else { continue };

        for esc_inst in &motor_escs {
            let esc_part_id = &nl.instances[esc_inst];
            let (esc_part, _) = cat.get(esc_part_id)?;
            let Some(esc_elec) = &esc_part.elec else { continue };
            let Some(esc_supply) = pin_net_by_role(esc_elec, esc_inst, net_of, "power_in") else { continue };

            let same_rail =
                bfs(&BTreeSet::from([esc_supply.clone()]), &g.undirected_zero_r, None).contains(&mcu_supply);
            if same_rail {
                return Ok(warn(
                    C,
                    D,
                    format!(
                        "{mcu_inst}'s supply net '{mcu_supply}' shares an unbuffered rail with {esc_inst}'s motor \
                         supply '{esc_supply}' — no regulator/BEC hop between them"
                    ),
                ));
            }
        }
    }

    Ok(ok(C, D, "no MCU shares an unbuffered rail with a motor-driving ESC".into()))
}

/// The M1 power-budget composition — E30, E31, E32.
pub fn power_checks(nl: &Netlist, cat: &ElecCatalogue) -> Result<Vec<CheckResult>, String> {
    let net_of = endpoint_net_index(nl);
    let g = build(nl, cat, &net_of)?;
    let resolved_volts = resolve_voltages(nl, &g.undirected_zero_r);
    let sinks = worst_case_sinks(nl, cat, &net_of, &resolved_volts)?;
    Ok(vec![
        e30_power_budget(nl, cat, &net_of, &g, &sinks)?,
        e31_wire_ampacity(nl, cat, &net_of, &g, &sinks)?,
        e32_brownout_topology(nl, cat, &net_of, &g)?,
    ])
}
