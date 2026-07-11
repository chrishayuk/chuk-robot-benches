//! The power graph derived artifact (specs/robowire.md §4 item 1) and the
//! wiring-mass estimate (§4 item 2) — the two pieces of robowire M1 that
//! were left open after E30-32 landed. Reuses `power.rs`'s worst-case
//! reachability machinery rather than re-deriving it.
//!
//! `robotspec::PowerGraph`/`DerivedRecord` are plain-data types owned by
//! `robotspec` (so the graph can live on `DerivedRecord`) — this module is
//! what actually computes them, since `robotspec` has no netlist type of
//! its own and can't depend on `robowire` (robowire already depends on
//! robotspec; the other way would be circular).

use crate::catalogue::ElecCatalogue;
use crate::checks::motor_output_pin;
use crate::graph::{endpoint_net_index, reach_from};
use crate::power::{build, pin_net_by_role, resolve_voltages, worst_case_sinks};
use crate::schema::{split_pin, Netlist};
use crate::wire;
use robotspec::{PowerChain, PowerGraph, PowerRail, WireSegment};

/// Total wiring-loom mass: bare-copper conductor mass for every
/// gauge+length-declared net, plus the catalogue `mass_g` of every
/// connector/fuse/PTC instance already in the netlist. Replaces the flat
/// `harness-allowance` placeholder that stood in for this until now — nets
/// with no declared gauge/length simply contribute 0 (no guessing).
pub fn wiring_mass_g(nl: &Netlist, cat: &ElecCatalogue) -> Result<f64, String> {
    let mut total = 0.0;
    for net in &nl.nets {
        if let (Some(awg), Some(length_mm)) = (net.gauge_awg, net.length_mm) {
            if let Some(g_per_m) = wire::copper_mass_g_per_m(awg) {
                total += g_per_m * (length_mm / 1000.0);
            }
        }
    }
    for part_id in nl.instances.values() {
        let (part, _) = cat.get(part_id)?;
        if matches!(part.kind.as_str(), "connector" | "fuse" | "ptc") {
            total += part.mass_g;
        }
    }
    Ok(total)
}

/// Derive the power graph: one rail per battery and per `power_out` pin
/// with a declared `max_a` (the same sources E30 checks), one segment per
/// gauge-declared net (the same nets E31 checks), one chain per motor
/// (source -> ESC -> motor). `sense_points` is left empty — no
/// current-sense part exists in the catalogue yet, so nothing is
/// fabricated there.
pub fn derive_power_graph(nl: &Netlist, cat: &ElecCatalogue) -> Result<PowerGraph, String> {
    let net_of = endpoint_net_index(nl);
    let g = build(nl, cat, &net_of)?;
    let resolved_volts = resolve_voltages(nl, &g.undirected_zero_r);
    let sinks = worst_case_sinks(nl, cat, &net_of, &resolved_volts)?;

    let mut rails = Vec::new();
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };

        if let Some(source) = &elec.source {
            if let Some(pos_net) = pin_net_by_role(elec, inst, &net_of, "pos") {
                let reach = reach_from(&pos_net, &g.undirected, &g.forward);
                let worst_case_a: f64 = sinks.iter().filter(|(n, _)| reach.contains(n)).map(|(_, a)| a).sum();
                let capacity_a = match (source.c_rating, source.capacity_mah) {
                    (Some(c), Some(mah)) => Some(c * mah / 1000.0),
                    _ => None,
                };
                rails.push(PowerRail {
                    source: inst.clone(),
                    worst_case_a,
                    capacity_a,
                    margin_a: capacity_a.map(|c| c - worst_case_a),
                });
            }
        }

        for (pin, decl) in &elec.pins {
            if decl.role != "power_out" {
                continue;
            }
            let Some(max_a) = decl.max_a else { continue };
            let Some(out_net) = net_of.get(&format!("{inst}.{pin}")) else { continue };
            let reach = reach_from(out_net, &g.undirected, &g.forward);
            let worst_case_a: f64 = sinks.iter().filter(|(n, _)| reach.contains(n)).map(|(_, a)| a).sum();
            rails.push(PowerRail {
                source: format!("{inst}.{pin}"),
                worst_case_a,
                capacity_a: Some(max_a),
                margin_a: Some(max_a - worst_case_a),
            });
        }
    }

    let mut segments = Vec::new();
    for net in &nl.nets {
        let (Some(awg), Some(length_mm)) = (net.gauge_awg, net.length_mm) else { continue };
        let reach = reach_from(&net.id, &g.undirected, &g.forward);
        let worst_case_a: f64 = sinks.iter().filter(|(n, _)| reach.contains(n)).map(|(_, a)| a).sum();
        segments.push(WireSegment {
            net: net.id.clone(),
            gauge_awg: awg,
            length_mm,
            resistance_ohms: wire::net_resistance_ohms(net),
            worst_case_a,
            ampacity_a: wire::awg_ampacity(awg),
        });
    }

    let mut chains = Vec::new();
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        if part.kind != "motor" {
            continue;
        }
        let Some(elec) = &part.elec else { continue };
        let Some((pin, _)) = elec.pins.iter().find(|(_, d)| d.role == "motor_in") else { continue };
        let terminal = format!("{inst}.{pin}");
        let Some(driver_pin) = motor_output_pin(nl, cat, &terminal)? else { continue };
        let (esc_inst, _) = split_pin(&driver_pin)?;
        let Some(esc_part_id) = nl.instances.get(esc_inst) else { continue };
        let (esc_part, _) = cat.get(esc_part_id)?;
        let Some(esc_elec) = &esc_part.elec else { continue };
        let Some(esc_supply) = pin_net_by_role(esc_elec, esc_inst, &net_of, "power_in") else { continue };

        // Which battery's own reachability (battery -> ... -> ESC, forward
        // direction, same semantics as E30's per-battery scoping) includes
        // this ESC's supply net — not the other way around, since a
        // regulator hop in between would make a naive "walk back from the
        // ESC" query miss the battery across its own directed forward edge.
        let mut source = String::new();
        for (batt_inst, batt_part_id) in &nl.instances {
            let (batt_part, _) = cat.get(batt_part_id)?;
            let Some(batt_elec) = &batt_part.elec else { continue };
            if batt_elec.source.is_none() {
                continue;
            }
            let Some(batt_pos) = pin_net_by_role(batt_elec, batt_inst, &net_of, "pos") else { continue };
            let reach = reach_from(&batt_pos, &g.undirected, &g.forward);
            if reach.contains(&esc_supply) {
                source = batt_inst.clone();
                break;
            }
        }

        chains.push(PowerChain { source, esc: esc_inst.to_string(), motor: inst.clone() });
    }

    Ok(PowerGraph { rails, segments, chains, sense_points: Vec::new() })
}

/// Attach the power graph, and the wiring-mass-derived total, to an
/// already-derived `DerivedRecord`. `robotspec::derive()` itself is
/// untouched by this feature (a bare call still reports `mass_wiring_g:
/// 0.0`, `power: None`) — this is a purely additive enrichment step.
/// D02 (mass within class limit) was evaluated against the pre-wiring
/// total inside `derive()`; it's re-run here so the merged record's own
/// `checks` stay honest about the number it now reports, rather than
/// silently going stale.
pub fn attach_power_graph(
    mut derived: robotspec::DerivedRecord,
    nl: &Netlist,
    cat: &ElecCatalogue,
) -> Result<robotspec::DerivedRecord, String> {
    let pg = derive_power_graph(nl, cat)?;
    let wiring_g = wiring_mass_g(nl, cat)?;
    derived.mass_wiring_g = wiring_g;
    derived.mass_total_g += wiring_g;
    derived.budget_margin_g -= wiring_g;
    if let Some(d02) = derived.checks.iter_mut().find(|c| c.code == "D02") {
        *d02 = robotspec::checks::d02_mass_limit(derived.mass_total_g);
    }
    derived.power = Some(pg);
    Ok(derived)
}
