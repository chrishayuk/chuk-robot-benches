//! Real-component electrical math. Current is never a fixed lookup number:
//! every figure here is Ohm's law against the ACTUAL live voltage a
//! component sees, derived from catalogue-declared REAL component
//! properties — a resistor's `ohms`, an LED's `forward_v`, a motor's winding
//! resistance (`nominal_v / stall_current_a`), a fixed-power device's
//! equivalent resistance (`nominal_v / current_ma`) — so if the voltage
//! changes, the current changes with it. This is still squarely
//! "worst-case/static budget arithmetic, not SPICE" (specs/robowire.md §1):
//! a fixed forward-voltage diode approximation and linear equivalent
//! resistances, not an iterative nonlinear solve. A part missing the
//! catalogue fields a calculation needs (e.g. no `ohms`, no `nominal_v`)
//! simply contributes 0A rather than guessing.

use crate::types::RunInputs;
use robowire::catalogue::{Elec, ElecCatalogue, ElecPart};
use robowire::schema::{split_pin, Netlist};
use std::collections::{BTreeMap, BTreeSet};

/// The net touching `inst`'s first pin declared with role `role`, if wired.
pub fn pin_net_by_role(
    elec: &Elec,
    inst: &str,
    net_of: &BTreeMap<String, String>,
    role: &str,
) -> Option<String> {
    elec.pins
        .iter()
        .find(|(_, d)| d.role == role)
        .and_then(|(p, _)| net_of.get(&format!("{inst}.{p}")).cloned())
}

/// Generic "is this instance's supply live and its return grounded" — the
/// same test for a battery (trivially true: it seeds both sets), a
/// regulator/ESC-BEC/MCU-3V3-out, or a bus sensor. Kinds without a
/// `power_in`/`pos` + `gnd` pin pair (switch, button, led, motor, ...)
/// always resolve `false` here; those kinds report through their own fields
/// instead (`closed`, `lit`, `spin`).
pub fn instance_powered(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    hot: &BTreeSet<String>,
    grounded: &BTreeSet<String>,
    inst: &str,
) -> Result<bool, String> {
    let part_id = nl.instances.get(inst).ok_or_else(|| format!("unknown instance '{inst}'"))?;
    let (part, _) = cat.get(part_id)?;
    let Some(elec) = &part.elec else { return Ok(false) };
    let supply = pin_net_by_role(elec, inst, net_of, "power_in")
        .or_else(|| pin_net_by_role(elec, inst, net_of, "pos"));
    let ret = pin_net_by_role(elec, inst, net_of, "gnd");
    Ok(matches!((supply, ret), (Some(s), Some(r)) if hot.contains(&s) && grounded.contains(&r)))
}

/// Propagate each net's declared voltage across zero-resistance
/// (switch/button/wiring) connections only — a resistor is a real voltage
/// drop and a regulator/BEC output is a fresh regulated source, so neither
/// may inherit a neighbor's voltage this way. Without this, an undeclared
/// intermediate net (e.g. the wire between a closed switch and the next
/// component, which authors don't bother giving its own `volts` since it's
/// obviously "whatever the switch passes through") would incorrectly read
/// as 0V even while energized — the exact bug class this model exists to
/// avoid: a live, current-carrying node reporting nothing.
pub fn resolve_voltages(
    nl: &Netlist,
    undirected_zero_r: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, f64> {
    let mut resolved: BTreeMap<String, f64> = BTreeMap::new();
    for net in &nl.nets {
        let Some(v) = net.volts else { continue };
        if resolved.contains_key(&net.id) {
            continue; // already covered by an earlier seed's component
        }
        for n in crate::graph::bfs(&BTreeSet::from([net.id.clone()]), undirected_zero_r, None) {
            resolved.entry(n).or_insert(v);
        }
    }
    resolved
}

/// Live voltage on a net: its resolved voltage (its own declared `Net.volts`,
/// or one inherited via `resolve_voltages` through a zero-resistance
/// connection) when energized, 0 when dead — the same number
/// `NetRunState.volts` reports, exposed standalone so the Ohm's-law
/// calculations below can use "the ACTUAL voltage here" rather than a fixed
/// nominal figure.
pub fn net_volts_live(resolved_volts: &BTreeMap<String, f64>, hot: &BTreeSet<String>, net_id: &str) -> f64 {
    if !hot.contains(net_id) {
        return 0.0;
    }
    resolved_volts.get(net_id).copied().unwrap_or(0.0)
}

/// A fixed-power device (sensor/MCU/ESC-quiescent/regulator-quiescent/radio/
/// buzzer) modelled as an equivalent resistance derived from its
/// catalogue-declared (current_ma, nominal_v) rated operating point. Ohm's
/// law against the ACTUAL voltage `v_actual` — current scales with whatever
/// voltage the part really sees, not a fixed number. 0.0 if the catalogue
/// doesn't declare both fields.
pub fn equiv_load_current(part: &ElecPart, v_actual: f64) -> f64 {
    match (part.current_ma, part.nominal_v) {
        (Some(ma), Some(v_nom)) if v_nom > 0.0 => (ma / 1000.0) * (v_actual / v_nom).max(0.0),
        _ => 0.0,
    }
}

/// A motor's winding resistance, derived from its catalogue-declared stall
/// point (`nominal_v / stall_current_a`) — `None` if either figure is
/// missing or non-positive.
pub fn motor_winding_ohms(motor: &robowire::catalogue::ElecPart) -> Option<f64> {
    let mp = motor.motor.as_ref()?;
    let nom_v = mp.nominal_v?;
    if nom_v > 0.0 && mp.stall_current_a > 0.0 {
        Some(nom_v / mp.stall_current_a)
    } else {
        None
    }
}

/// A `resistor`'s fixed ohms, or a `potentiometer`'s LIVE ohms — its declared
/// range scaled by the current dial position (`RunInputs.dial_positions`,
/// default 0.5 if the user hasn't touched it) — so turning the dial changes
/// the resistance, and therefore the current, exactly like swapping in a
/// different fixed resistor would. `None` for any other kind, or a
/// potentiometer missing its declared range.
fn series_limiter_ohms(part: &ElecPart, inst: &str, inputs: &RunInputs) -> Option<f64> {
    match part.kind.as_str() {
        "resistor" => part.ohms,
        "potentiometer" => {
            let lo = part.ohms_min.unwrap_or(0.0);
            let hi = part.ohms_max?;
            let pos = inputs.dial_positions.get(inst).copied().unwrap_or(0.5).clamp(0.0, 1.0);
            Some(lo + (hi - lo) * pos)
        }
        _ => None,
    }
}

/// Find the resistor-or-potentiometer in series with a lit LED and the net
/// feeding it — enough to solve I = (V_supply − forward_v) / ohms, the
/// standard hand-calculation for an LED+resistor circuit (a fixed-Vf diode
/// approximation, not an iterative nonlinear SPICE solve). `None` if no such
/// limiter is structurally identifiable (E33 would already be failing) or it
/// declares no usable resistance.
pub fn led_series_supply(
    nl: &Netlist,
    cat: &ElecCatalogue,
    net_of: &BTreeMap<String, String>,
    inputs: &RunInputs,
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
            let Some(ohms) = series_limiter_ohms(other_part, other_inst, inputs) else { continue };
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
