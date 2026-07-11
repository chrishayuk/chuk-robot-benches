//! E-checks (specs/robowire.md §3, registered in specs/codes.md). Each check
//! is individually callable; `run_checks` is the M0 composition (E01–04,
//! E10–11, E20–21, E40–41). Bugs become rules: new statically-detectable
//! failures get a code in codes.md first, then a function here.

use crate::catalogue::{ElecCatalogue, PinDecl};
use crate::schema::{split_pin, Bus, Netlist};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckResult {
    pub code: String,
    pub description: String,
    pub pass: bool,
    pub detail: String,
}

fn ok(code: &str, description: &str, detail: String) -> CheckResult {
    CheckResult { code: code.into(), description: description.into(), pass: true, detail }
}

fn fail(code: &str, description: &str, detail: String) -> CheckResult {
    CheckResult { code: code.into(), description: description.into(), pass: false, detail }
}

/// Resolved pin declaration for "inst.PIN".
pub fn pin_decl<'c>(
    nl: &Netlist,
    cat: &'c ElecCatalogue,
    endpoint: &str,
) -> Result<&'c PinDecl, String> {
    let (inst, pin) = split_pin(endpoint)?;
    let part_id = nl
        .instances
        .get(inst)
        .ok_or_else(|| format!("unknown instance '{inst}' in '{endpoint}'"))?;
    let (part, _) = cat.get(part_id)?;
    part.elec
        .as_ref()
        .and_then(|e| e.pins.get(pin))
        .ok_or_else(|| format!("part '{part_id}' has no pin '{pin}' (from '{endpoint}')"))
}

/// Resolve a motor terminal ("inst.PIN") to the single motor_out-role pin
/// driving it on its net, if unambiguous. `None` on any ambiguity (a missing
/// or multiply-connected net, or zero/multiple driver pins on it) — E01 will
/// already be failing in that case. Shared between E01 and
/// `runtime::run_state`'s motor-spin projection.
pub fn motor_output_pin(
    nl: &Netlist,
    cat: &ElecCatalogue,
    motor_terminal: &str,
) -> Result<Option<String>, String> {
    let endpoint = motor_terminal.to_string();
    let nets: Vec<_> = nl.nets.iter().filter(|n| n.pins.contains(&endpoint)).collect();
    if nets.len() != 1 {
        return Ok(None);
    }
    let driver_pins: Vec<_> = nets[0]
        .pins
        .iter()
        .filter(|p| pin_decl(nl, cat, p).map(|d| d.role == "motor_out").unwrap_or(false))
        .collect();
    if driver_pins.len() != 1 {
        return Ok(None);
    }
    Ok(Some(driver_pins[0].clone()))
}

/// E01: every motor terminal pair reaches exactly one driver channel.
pub fn e01_motor_channels(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E01";
    const D: &str = "every motor terminal pair reaches exactly one driver channel";
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };
        let motor_pins: Vec<&String> = elec
            .pins
            .iter()
            .filter(|(_, d)| d.role == "motor_in")
            .map(|(n, _)| n)
            .collect();
        if motor_pins.is_empty() {
            continue;
        }
        let mut channels = Vec::new();
        for pin in &motor_pins {
            let endpoint = format!("{inst}.{pin}");
            let Some(driver_pin) = motor_output_pin(nl, cat, &endpoint)? else {
                return Ok(fail(C, D, format!("{endpoint}: no single driver channel reaches it")));
            };
            let d = pin_decl(nl, cat, &driver_pin)?;
            channels.push((driver_pin, d.channel.clone().unwrap_or_default()));
        }
        let chans: Vec<&String> = channels.iter().map(|(_, c)| c).collect();
        if chans.windows(2).any(|w| w[0] != w[1]) {
            return Ok(fail(C, D, format!("{inst} terminals split across channels {chans:?}")));
        }
        if channels[0].0 == channels[1].0 {
            return Ok(fail(C, D, format!("{inst} both terminals on the same driver pin")));
        }
    }
    Ok(ok(C, D, "all motors paired to single driver channels".into()))
}

/// E02: power pins reach a rail of legal voltage.
pub fn e02_rail_voltages(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E02";
    const D: &str = "power pins reach a rail of legal voltage";
    for net in &nl.nets {
        for p in &net.pins {
            let decl = pin_decl(nl, cat, p)?;
            if decl.role == "power_in" {
                let Some(volts) = net.volts else {
                    return Ok(fail(C, D, format!("{p} on net '{}' with no declared voltage", net.id)));
                };
                if let Some([lo, hi]) = decl.v_range {
                    if volts < lo || volts > hi {
                        return Ok(fail(
                            C,
                            D,
                            format!("{p} rated [{lo},{hi}]V on {volts}V net '{}'", net.id),
                        ));
                    }
                }
            }
            // Declared rail voltage must match any power_out source on it.
            if decl.role == "power_out" {
                if let (Some(pv), Some(nv)) = (decl.volts, net.volts) {
                    if (pv - nv).abs() > 1e-9 {
                        return Ok(fail(
                            C,
                            D,
                            format!("{p} outputs {pv}V but net '{}' declares {nv}V", net.id),
                        ));
                    }
                }
            }
        }
    }
    Ok(ok(C, D, "all power pins within rated ranges".into()))
}

/// E03: polarity continuity — no net mixes supply-positive and ground roles.
pub fn e03_polarity(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E03";
    const D: &str = "polarity continuity (no +/- swap reachable)";
    for net in &nl.nets {
        let mut has_pos = false;
        let mut has_gnd = false;
        for p in &net.pins {
            let decl = pin_decl(nl, cat, p)?;
            match decl.role.as_str() {
                "pos" | "power_out" | "switch_out" | "switch_in" | "power_in" => {
                    has_pos = true;
                }
                "gnd" => has_gnd = true,
                _ => {}
            }
        }
        if has_pos && has_gnd {
            return Ok(fail(C, D, format!("net '{}' mixes supply and ground pins", net.id)));
        }
    }
    Ok(ok(C, D, "no supply/ground mixing on any net".into()))
}

/// E04: no floating required pins (bus membership counts for SDA/SCL).
pub fn e04_required_pins(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E04";
    const D: &str = "no floating required pins";
    let mut connected: Vec<String> = Vec::new();
    for net in &nl.nets {
        connected.extend(net.pins.iter().cloned());
    }
    for bus in &nl.buses {
        connected.push(bus.sda.clone());
        connected.push(bus.scl.clone());
        for dev in &bus.devices {
            connected.push(format!("{}.SDA", dev.inst));
            connected.push(format!("{}.SCL", dev.inst));
            if let Some(x) = &dev.xshut {
                connected.push(x.clone());
                connected.push(format!("{}.XSHUT", dev.inst));
            }
        }
    }
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };
        for (pin, decl) in &elec.pins {
            if decl.required && !connected.contains(&format!("{inst}.{pin}")) {
                return Ok(fail(C, D, format!("{inst}.{pin} (required) is floating")));
            }
        }
    }
    Ok(ok(C, D, "all required pins connected".into()))
}

/// E10: MCU nets use pins with the required capability.
pub fn e10_pin_capability(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E10";
    const D: &str = "MCU pins carry required capabilities";
    let mut demands: Vec<(String, String)> = Vec::new(); // (endpoint, needed cap)
    for net in &nl.nets {
        if let Some(sig) = &net.signal {
            for p in &net.pins {
                let decl = pin_decl(nl, cat, p)?;
                if decl.role == "mcu_io" {
                    // uart nets need the receiving direction capability.
                    let needed = if sig == "uart" { "uart_rx".to_string() } else { sig.clone() };
                    demands.push((p.clone(), needed));
                }
            }
        }
    }
    for bus in &nl.buses {
        demands.push((bus.sda.clone(), "i2c_sda".into()));
        demands.push((bus.scl.clone(), "i2c_scl".into()));
        for dev in &bus.devices {
            if let Some(x) = &dev.xshut {
                demands.push((x.clone(), "gpio".into()));
            }
        }
    }
    for (endpoint, needed) in demands {
        let decl = pin_decl(nl, cat, &endpoint)?;
        let caps = decl.caps.clone().unwrap_or_default();
        if !caps.contains(&needed) {
            return Ok(fail(C, D, format!("{endpoint} lacks capability '{needed}' (has {caps:?})")));
        }
    }
    Ok(ok(C, D, "all demanded capabilities present".into()))
}

/// E11: no MCU io pin double-booked across nets/buses.
pub fn e11_pin_double_booking(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E11";
    const D: &str = "no MCU pin double-booked";
    let mut uses: BTreeMap<String, u32> = BTreeMap::new();
    let mut bump = |e: &String| *uses.entry(e.clone()).or_insert(0) += 1;
    for net in &nl.nets {
        for p in &net.pins {
            bump(p);
        }
    }
    for bus in &nl.buses {
        bump(&bus.sda);
        bump(&bus.scl);
        for dev in &bus.devices {
            if let Some(x) = &dev.xshut {
                bump(x);
            }
        }
    }
    for (endpoint, n) in &uses {
        if *n > 1 {
            let decl = pin_decl(nl, cat, endpoint)?;
            if decl.role == "mcu_io" {
                return Ok(fail(C, D, format!("{endpoint} used {n} times")));
            }
        }
    }
    Ok(ok(C, D, "all MCU pins single-purpose".into()))
}

/// Each device's final I2C address after its reassignment plan (`reassign_to`
/// if declared, else its default `addr`) — the grouping E20 fails on when a
/// group has more than one member, shared with `runtime::run_state`'s
/// `bus_conflict` projection.
pub fn bus_final_addresses(bus: &Bus) -> BTreeMap<String, String> {
    bus.devices
        .iter()
        .map(|dev| (dev.inst.clone(), dev.reassign_to.clone().unwrap_or_else(|| dev.addr.clone())))
        .collect()
}

/// E20: I2C address collisions after the reassignment plan.
pub fn e20_i2c_addresses(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E20";
    const D: &str = "I2C addresses unique after reassignment plan";
    for bus in &nl.buses {
        for dev in &bus.devices {
            let part_id = nl
                .instances
                .get(&dev.inst)
                .ok_or_else(|| format!("unknown instance '{}'", dev.inst))?;
            let (part, _) = cat.get(part_id)?;
            let bus_decl = part.elec.as_ref().and_then(|e| e.bus.as_ref());
            if let Some(re) = &dev.reassign_to {
                let Some(bd) = bus_decl else {
                    return Ok(fail(C, D, format!("{} reassigned but part declares no bus", dev.inst)));
                };
                if !bd.addr_reassignable {
                    return Ok(fail(C, D, format!("{} address not reassignable", dev.inst)));
                }
                if bd.requires_xshut && dev.xshut.is_none() {
                    return Ok(fail(
                        C,
                        D,
                        format!(
                            "{} reassignment to {re} requires a wired XSHUT and none is assigned",
                            dev.inst
                        ),
                    ));
                }
            }
        }
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (inst, addr) in bus_final_addresses(bus) {
            groups.entry(addr).or_default().push(inst);
        }
        for (addr, insts) in &groups {
            if insts.len() > 1 {
                return Ok(fail(
                    C,
                    D,
                    format!("bus '{}': {insts:?} collide at {addr} (the dual-0x29 classic)", bus.id),
                ));
            }
        }
    }
    Ok(ok(C, D, "all bus addresses unique post-reassignment".into()))
}

/// E21: bus voltage-domain consistency.
pub fn e21_bus_voltage(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E21";
    const D: &str = "bus voltage-domain consistency";
    for bus in &nl.buses {
        let master_io = 3.3; // v0: RP2350-class masters; part-declared later
        for dev in &bus.devices {
            let part_id = &nl.instances[&dev.inst];
            let (part, _) = cat.get(part_id)?;
            if let Some(bd) = part.elec.as_ref().and_then(|e| e.bus.as_ref()) {
                if let Some(io) = bd.io_v {
                    if (io - master_io).abs() > 0.35 {
                        return Ok(fail(
                            C,
                            D,
                            format!("{}: device io {io}V vs bus master {master_io}V", dev.inst),
                        ));
                    }
                }
            }
        }
    }
    Ok(ok(C, D, "all bus devices in the master's voltage domain".into()))
}

/// Whether `inst` (an LED) is wired to anything at all.
fn led_is_wired(nl: &Netlist, inst: &str) -> bool {
    let prefix = format!("{inst}.");
    nl.nets.iter().any(|net| net.pins.iter().any(|p| p.starts_with(&prefix)))
}

/// Whether `inst` (an LED) has a resistor-or-potentiometer-kind neighbor on
/// any net it's wired to — the E33 series-current-limiter rule, shared with
/// `robosim`'s `lit` projection. A potentiometer wired as a rheostat limits
/// current exactly like a fixed resistor (its value just moves).
pub fn led_current_limited(nl: &Netlist, cat: &ElecCatalogue, inst: &str) -> Result<bool, String> {
    let prefix = format!("{inst}.");
    let mut limited = false;
    for net in &nl.nets {
        if !net.pins.iter().any(|p| p.starts_with(&prefix)) {
            continue;
        }
        for p in &net.pins {
            let (other_inst, _) = split_pin(p)?;
            if other_inst == inst {
                continue;
            }
            let other_part = nl.instances.get(other_inst).and_then(|pid| cat.get(pid).ok());
            if other_part.map_or(false, |(op, _)| matches!(op.kind.as_str(), "resistor" | "potentiometer")) {
                limited = true;
            }
        }
    }
    Ok(limited)
}

/// E33: every LED must see a series current limiter — a bare LED across a
/// rail (or a GPIO) is a statically detectable dead part. Registered in
/// specs/codes.md before this function existed, per bugs-become-rules.
pub fn e33_led_current_limit(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E33";
    const D: &str = "LEDs have series current limiting";
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        if part.kind != "led" {
            continue;
        }
        if led_is_wired(nl, inst) && !led_current_limited(nl, cat, inst)? {
            return Ok(fail(
                C,
                D,
                format!("{inst}: no resistor on either side — it will burn on first power"),
            ));
        }
    }
    Ok(ok(C, D, "all wired LEDs are current-limited".into()))
}

/// E40: a switch interrupts the main power path.
pub fn e40_power_switch(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E40";
    const D: &str = "switch interrupts the main power path (tech-check)";
    // Find battery positive pins.
    for (inst, part_id) in &nl.instances {
        let (part, _) = cat.get(part_id)?;
        let Some(elec) = &part.elec else { continue };
        if elec.source.is_none() {
            continue;
        }
        for (pin, decl) in &elec.pins {
            if decl.role != "pos" {
                continue;
            }
            let endpoint = format!("{inst}.{pin}");
            let Some(net) = nl.nets.iter().find(|n| n.pins.contains(&endpoint)) else {
                return Ok(fail(C, D, format!("{endpoint} not connected")));
            };
            let mut has_switch_in = false;
            for p in &net.pins {
                let d = pin_decl(nl, cat, p)?;
                match d.role.as_str() {
                    "switch_in" => has_switch_in = true,
                    "power_in" => {
                        return Ok(fail(
                            C,
                            D,
                            format!("load {p} sits on the battery net '{}' ahead of any switch", net.id),
                        ));
                    }
                    _ => {}
                }
            }
            if !has_switch_in {
                return Ok(fail(C, D, format!("battery net '{}' has no switch in path", net.id)));
            }
        }
    }
    Ok(ok(C, D, "battery positive reaches loads only through a switch".into()))
}

/// E41: failsafe stop chain declared and MCU-reachable.
pub fn e41_failsafe_chain(nl: &Netlist, cat: &ElecCatalogue) -> Result<CheckResult, String> {
    const C: &str = "E41";
    const D: &str = "failsafe stop chain declared and complete";
    // The stop chain is a robot-control requirement: without a radio link
    // there is nothing to lose signal from (battery-and-bulb circuits).
    let has_radio = nl.instances.values().any(|pid| {
        cat.get(pid).map_or(false, |(p, _)| p.kind == "radio")
    });
    if !has_radio {
        return Ok(ok(C, D, "no radio link — stop chain not applicable".into()));
    }
    let Some(fs) = &nl.failsafe else {
        return Ok(fail(C, D, "no failsafe declaration".into()));
    };
    if fs.stop_pins.is_empty() {
        return Ok(fail(C, D, "failsafe declares no stop pins".into()));
    }
    for stop in &fs.stop_pins {
        let Some(net) = nl.nets.iter().find(|n| n.pins.contains(stop)) else {
            return Ok(fail(C, D, format!("stop pin {stop} not connected")));
        };
        let mut mcu_reachable = false;
        for p in &net.pins {
            if pin_decl(nl, cat, p)?.role == "mcu_io" {
                mcu_reachable = true;
            }
        }
        if !mcu_reachable {
            return Ok(fail(C, D, format!("stop pin {stop} has no MCU pin on net '{}'", net.id)));
        }
    }
    Ok(ok(C, D, format!("stop chain complete: {}", fs.rx_loss)))
}

/// The M0 composition.
pub fn run_checks(nl: &Netlist, cat: &ElecCatalogue) -> Result<Vec<CheckResult>, String> {
    Ok(vec![
        e01_motor_channels(nl, cat)?,
        e02_rail_voltages(nl, cat)?,
        e03_polarity(nl, cat)?,
        e04_required_pins(nl, cat)?,
        e10_pin_capability(nl, cat)?,
        e11_pin_double_booking(nl, cat)?,
        e20_i2c_addresses(nl, cat)?,
        e21_bus_voltage(nl, cat)?,
        e33_led_current_limit(nl, cat)?,
        e40_power_switch(nl, cat)?,
        e41_failsafe_chain(nl, cat)?,
    ])
}
