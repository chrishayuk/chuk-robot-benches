//! Run-time signal-path resolution: which `mcu_io` pin ultimately drives a
//! given `signal_in`-role pin (an ESC channel, a servo's `SIG`, ...), and the
//! reverse — which motor a given signal pin ends up driving. Shared by
//! robosim's run-mode data source, so a run-mode slider's value has to flow
//! through the netlist's real wiring to reach a motor, rather than being
//! pinned directly to the motor instance itself (the same "no consumer
//! reconstructs what the sim can compute" principle already applied to
//! `motor_output_pin`, `led_current_limited`, etc.).

use crate::catalogue::ElecCatalogue;
use crate::checks::{motor_output_pin, pin_decl};
use crate::Netlist;

/// Every other pin sharing a net with `pin`, filtered to a given role.
/// Empty (not an error) when the net doesn't resolve cleanly to exactly one
/// such neighbor — same "ambiguous means unresolved" convention
/// `motor_output_pin` already uses for a motor terminal's driver pin.
fn co_net_pins_with_role(
    nl: &Netlist,
    cat: &ElecCatalogue,
    pin: &str,
    role: &str,
) -> Result<Vec<String>, String> {
    let nets: Vec<_> = nl.nets.iter().filter(|n| n.pins.iter().any(|p| p == pin)).collect();
    if nets.len() != 1 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for p in &nets[0].pins {
        if p == pin {
            continue;
        }
        if pin_decl(nl, cat, p)?.role == role {
            out.push(p.clone());
        }
    }
    Ok(out)
}

/// The single `mcu_io` pin driving a `signal_in`-role pin, if the wiring
/// resolves unambiguously (`None` for a floating/dummy signal net, or a net
/// with more than one candidate source).
pub fn signal_source_pin(
    nl: &Netlist,
    cat: &ElecCatalogue,
    signal_in_pin: &str,
) -> Result<Option<String>, String> {
    let mut sources = co_net_pins_with_role(nl, cat, signal_in_pin, "mcu_io")?;
    if sources.len() != 1 {
        return Ok(None);
    }
    Ok(Some(sources.remove(0)))
}

/// For a motor terminal: its driver channel's own signal-in pin, then that
/// pin's `mcu_io` source — the two hops from `m1.M+` to `mcu.GP2`. `None` at
/// any hop (no driver, no declared channel, no matching signal pin, no MCU
/// wired) means "nothing supplies this motor a signal" — a real, legal
/// circuit state (see `harness/lessons/02-motor-driver.json`, which has no
/// brain yet), not an error.
pub fn motor_signal_source_pin(
    nl: &Netlist,
    cat: &ElecCatalogue,
    motor_terminal: &str,
) -> Result<Option<String>, String> {
    let Some(driver_pin) = motor_output_pin(nl, cat, motor_terminal)? else { return Ok(None) };
    let Some(channel) = pin_decl(nl, cat, &driver_pin)?.channel.clone() else { return Ok(None) };
    let (esc_inst, _) = crate::schema::split_pin(&driver_pin)?;
    let sig_pin = single_pin_by_role_and_channel(nl, cat, esc_inst, "signal_in", &channel)?;
    let Some(sig_pin) = sig_pin else { return Ok(None) };
    signal_source_pin(nl, cat, &sig_pin)
}

/// Every `mcu_io` pin on `mcu_inst` that's actually wired to drive something
/// (a `signal_in`-role pin), paired with the motor instance it resolves to
/// when that's determinable — the single source of truth for both which
/// run-mode sliders to render on the MCU's own row and how to label them.
pub fn mcu_drivable_pins(
    nl: &Netlist,
    cat: &ElecCatalogue,
    mcu_inst: &str,
) -> Result<Vec<(String, Option<String>)>, String> {
    let Some(part_id) = nl.instances.get(mcu_inst) else { return Ok(Vec::new()) };
    let (part, _) = cat.get(part_id)?;
    let Some(elec) = &part.elec else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for (pin, decl) in &elec.pins {
        if decl.role != "mcu_io" {
            continue;
        }
        let endpoint = format!("{mcu_inst}.{pin}");
        let sig_pins = co_net_pins_with_role(nl, cat, &endpoint, "signal_in")?;
        let [sig_pin] = sig_pins.as_slice() else { continue };
        out.push((pin.clone(), driven_motor_inst(nl, cat, sig_pin)?));
    }
    out.sort();
    Ok(out)
}

/// The one pin on `inst` with the given role and declared channel, if
/// exactly one exists.
fn single_pin_by_role_and_channel(
    nl: &Netlist,
    cat: &ElecCatalogue,
    inst: &str,
    role: &str,
    channel: &str,
) -> Result<Option<String>, String> {
    let part_id = nl.instances.get(inst).ok_or_else(|| format!("no such instance '{inst}'"))?;
    let (part, _) = cat.get(part_id)?;
    let Some(elec) = &part.elec else { return Ok(None) };
    let matches: Vec<String> = elec
        .pins
        .iter()
        .filter(|(_, d)| d.role == role && d.channel.as_deref() == Some(channel))
        .map(|(pin, _)| format!("{inst}.{pin}"))
        .collect();
    if matches.len() != 1 {
        return Ok(None);
    }
    Ok(Some(matches[0].clone()))
}

/// The motor instance whose terminals resolve, via channel matching, back to
/// this `signal_in` pin's own driver instance and channel — the reverse hop
/// of `motor_signal_source_pin`, used only to label a run-mode slider
/// ("GP2 -> m1"), never for any electrical computation.
fn driven_motor_inst(
    nl: &Netlist,
    cat: &ElecCatalogue,
    signal_in_pin: &str,
) -> Result<Option<String>, String> {
    let Some(channel) = pin_decl(nl, cat, signal_in_pin)?.channel.clone() else { return Ok(None) };
    let (driver_inst, _) = crate::schema::split_pin(signal_in_pin)?;
    let part_id = nl.instances.get(driver_inst).ok_or_else(|| format!("no such instance '{driver_inst}'"))?;
    let (part, _) = cat.get(part_id)?;
    let Some(elec) = &part.elec else { return Ok(None) };
    // A channel has TWO motor_out pins (+/-, e.g. M1+/M1-), unlike signal_in
    // which has exactly one — so this can't reuse single_pin_by_role_and_channel.
    // Either terminal's net leads to the same motor instance in a legal netlist.
    let motor_out_pins: Vec<String> = elec
        .pins
        .iter()
        .filter(|(_, d)| d.role == "motor_out" && d.channel.as_deref() == Some(channel.as_str()))
        .map(|(pin, _)| format!("{driver_inst}.{pin}"))
        .collect();
    for mp in &motor_out_pins {
        let motor_pins = co_net_pins_with_role(nl, cat, mp, "motor_in")?;
        if let Some(m) = motor_pins.first() {
            return Ok(Some(crate::schema::split_pin(m)?.0.to_string()));
        }
    }
    Ok(None)
}
