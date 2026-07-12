//! Plain-English prose generation — THE single source (the designer's JS
//! mirror is retired; it now displays strings produced here via WASM).

use crate::catalogue::ElecCatalogue;
use crate::schema::{split_pin, Netlist};
use roboparts::PinDecl;
use serde::Serialize;
use std::collections::BTreeMap;

pub fn role_text(d: &PinDecl) -> String {
    match d.role.as_str() {
        "pos" => "battery positive terminal".into(),
        "gnd" => "ground".into(),
        "power_in" => match d.v_range {
            Some([lo, hi]) => format!("power input (rated {lo}–{hi} V)"),
            None => "power input".into(),
        },
        "power_out" => format!(
            "regulator output ({} V{})",
            d.volts.unwrap_or(0.0),
            d.max_a.map(|a| format!(", max {a} A")).unwrap_or_default()
        ),
        "switch_in" => "master switch input (E40 main path)".into(),
        "switch_out" => "master switch output".into(),
        "motor_in" => "motor terminal".into(),
        "motor_out" => format!(
            "driver output{}",
            d.channel.as_ref().map(|c| format!(" (channel {c})")).unwrap_or_default()
        ),
        "signal_in" => format!(
            "signal input{}",
            d.signal.as_ref().map(|s| format!(" ({s})")).unwrap_or_default()
        ),
        "signal_out" => format!(
            "signal output{}",
            d.signal.as_ref().map(|s| format!(" ({s})")).unwrap_or_default()
        ),
        "mcu_io" => format!("MCU pin [{}]", d.caps.clone().unwrap_or_default().join(", ")),
        "bus_sda" => "I2C data (SDA)".into(),
        "bus_scl" => "I2C clock (SCL)".into(),
        "gpio_in" => "control input".into(),
        "diode_a" => "LED anode (current in)".into(),
        "diode_k" => "LED cathode (current out)".into(),
        "passive" => "passive terminal (either way round)".into(),
        other => other.into(),
    }
}

fn kind_of(nl: &Netlist, cat: &ElecCatalogue, inst: &str) -> String {
    nl.instances
        .get(inst)
        .and_then(|pid| cat.get(pid).ok())
        .map(|(p, _)| p.kind.clone())
        .unwrap_or_default()
}

fn who(nl: &Netlist, cat: &ElecCatalogue, inst: &str) -> String {
    let noun = match kind_of(nl, cat, inst).as_str() {
        "battery" => "the battery pack",
        "switch" => "the power switch",
        "esc" => "the motor controller",
        "mcu" => "the brain",
        "radio" => "the radio receiver",
        "tof" => "a floor sensor",
        "imu" => "the motion sensor",
        "light" => "a line/light sensor",
        "env" => "an environmental sensor",
        "motor" => "a drive motor",
        "regulator" => "the 5V regulator",
        "solar-panel" => "the solar panel",
        "charge-controller" => "the charge controller",
        "led" => "an indicator LED",
        "resistor" => "a resistor",
        "buzzer" => "the buzzer",
        "button" => "a push button",
        "servo" => "a servo",
        "connector" => "a connector",
        "fuse" | "ptc" => "a fuse",
        _ => return format!("'{inst}'"),
    };
    format!("{noun} ({inst})")
}

pub fn net_class(nl: &Netlist, cat: &ElecCatalogue, net: &crate::schema::Net) -> String {
    let is_gnd = net.volts.is_none()
        && net.signal.is_none()
        && net.pins.iter().any(|p| p.ends_with(".GND") || p.ends_with(".-"));
    if is_gnd {
        return "gnd".into();
    }
    let is_motor = net.pins.iter().any(|p| {
        split_pin(p).ok().and_then(|(inst, pin)| {
            let pid = nl.instances.get(inst)?;
            let (part, _) = cat.get(pid).ok()?;
            part.elec.as_ref()?.pins.get(pin).map(|d| {
                d.role == "motor_in" || d.role == "motor_out"
            })
        }) == Some(true)
    });
    if is_motor {
        return "motor".into();
    }
    if let Some(v) = net.volts {
        if v > 6.0 {
            return "vbat".into();
        }
        if v > 4.0 {
            return "v5".into();
        }
        return "v33".into();
    }
    match net.signal.as_deref() {
        Some("pwm") => "pwm".into(),
        Some("uart") => "uart".into(),
        _ => "sig".into(),
    }
}

pub fn wire_about(nl: &Netlist, cat: &ElecCatalogue, net: &crate::schema::Net) -> String {
    let cls = net_class(nl, cat, net);
    let mut insts: Vec<String> = Vec::new();
    for p in &net.pins {
        if let Ok((i, _)) = split_pin(p) {
            if !insts.contains(&i.to_string()) {
                insts.push(i.to_string());
            }
        }
    }
    let list_who = |skip_kind: &str| -> String {
        let named: Vec<String> = insts
            .iter()
            .filter(|i| kind_of(nl, cat, i) != skip_kind)
            .map(|i| who(nl, cat, i))
            .collect();
        match named.len() {
            0 => String::new(),
            1 => named[0].clone(),
            _ => format!(
                "{} and {}",
                named[..named.len() - 1].join(", "),
                named[named.len() - 1]
            ),
        }
    };
    match cls.as_str() {
        "vbat" => format!(
            "The main power line: raw {} V battery power flowing between {}. Nothing downstream runs without it — and E40 demands the master switch sits in this path.",
            net.volts.unwrap_or(7.4),
            list_who("")
        ),
        "v5" => format!(
            "The 5-volt supply: a regulator makes clean 5 V and feeds {} — this is what keeps them alive.",
            list_who("esc")
        ),
        "v33" => format!(
            "The 3.3-volt supply: the brain's onboard regulator powers {} through this line.",
            list_who("mcu")
        ),
        "gnd" => "The shared ground return. Every component's current flows back to the battery through this — it is the other half of every circuit.".into(),
        "motor" => {
            let m = insts.iter().find(|i| kind_of(nl, cat, i) == "motor");
            format!(
                "Motor power: the controller pushes current down this wire to spin {}. Reverse the current and the wheel reverses.",
                m.map(|i| who(nl, cat, i)).unwrap_or_else(|| "the motor".into())
            )
        }
        "pwm" => "A drive command line: the brain sets one motor channel's speed by sending timed pulses (PWM) down this wire. On radio loss the failsafe holds it at neutral — that is what stops the robot (E41).".into(),
        "uart" => "The control link: the radio receiver streams the driver's stick positions to the brain over this wire. If those frames stop arriving, the failsafe fires.".into(),
        _ => "A signal line.".into(),
    }
}

#[derive(Serialize)]
pub struct NetProse {
    pub about: String,
    pub ends: Vec<String>,
}

/// Everything the designer needs to explain the current netlist: role text
/// for every pin of every placed instance, prose per net.
pub fn describe(nl: &Netlist, cat: &ElecCatalogue) -> serde_json::Value {
    let mut pins: BTreeMap<String, String> = BTreeMap::new();
    for (inst, pid) in &nl.instances {
        if let Ok((part, _)) = cat.get(pid) {
            if let Some(elec) = &part.elec {
                for (pin, d) in &elec.pins {
                    pins.insert(format!("{inst}.{pin}"), role_text(d));
                }
            }
        }
    }
    let mut nets: BTreeMap<String, NetProse> = BTreeMap::new();
    for net in &nl.nets {
        let ends = net
            .pins
            .iter()
            .map(|p| {
                let role = pins.get(p).cloned().unwrap_or_default();
                if role.is_empty() {
                    p.clone()
                } else {
                    format!("{p} — {role}")
                }
            })
            .collect();
        nets.insert(
            net.id.clone(),
            NetProse { about: wire_about(nl, cat, net), ends },
        );
    }
    serde_json::json!({ "pins": pins, "nets": nets })
}
