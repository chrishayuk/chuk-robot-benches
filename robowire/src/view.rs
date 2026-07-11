//! 3D harness view: wiring rendered on the robot geometry it belongs to
//! (robotspec-viewer M3 power-graph overlay, pulled forward). The generator
//! resolves netlist instances to RobotSpec placements — a working preview of
//! the X01 assembly cross-check — routes each net as a loom polyline in mm
//! space, and splices the scene into a self-contained HTML template.

use crate::catalogue::ElecCatalogue;
use crate::schema::{split_pin, Netlist};
use robotspec::schema::{MechSource, RobotSpec};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
struct Node {
    id: String,
    part: String,
    pos: [f64; 3],
    size: [f64; 3],
    kind: String,
    /// Plain-English: what this is and what it does on this robot.
    about: String,
    /// Dossier rows for the inspector panel: [label, value].
    detail: Vec<[String; 2]>,
    /// Every connection this instance participates in.
    conns: Vec<Conn>,
}

#[derive(Serialize)]
struct Conn {
    pin: String,
    role: String,
    net: String,
    class: String,
    others: String,
}

#[derive(Serialize)]
struct Wire {
    id: String,
    class: String,
    dashed: bool,
    pts: Vec<[f64; 3]>,
    ends: Vec<String>,
    /// What this wire carries, human-stated.
    carries: String,
    /// Plain-English: what this wire does in the system, whole sentences.
    about: String,
    /// Why it exists / which rule governs it.
    note: String,
    /// Per-endpoint explanation: "esc.VIN — power input (rated 6–9 V)".
    ends_detail: Vec<String>,
}

#[derive(Serialize)]
struct Scene {
    name: String,
    chassis: ChassisGeo,
    wheels: Vec<WheelGeo>,
    skids: Vec<[f64; 2]>,
    nodes: Vec<Node>,
    wires: Vec<Wire>,
}

#[derive(Serialize)]
struct ChassisGeo {
    profile: Vec<[f64; 2]>, // (x, z) side profile
    half_width: f64,
}

#[derive(Serialize)]
struct WheelGeo {
    pos: [f64; 3],
    radius: f64,
    width: f64,
}

fn size_for(kind: &str) -> [f64; 3] {
    match kind {
        "battery" => [36.0, 20.0, 11.0],
        "esc" => [22.0, 16.0, 6.0],
        "mcu" => [24.0, 18.0, 5.0],
        "radio" => [16.0, 11.0, 4.0],
        "switch" => [12.0, 8.0, 8.0],
        "tof" => [14.0, 10.0, 4.0],
        "imu" => [13.0, 13.0, 4.0],
        "motor" => [15.0, 11.0, 11.0],
        _ => [12.0, 12.0, 6.0],
    }
}

/// Resolve each netlist instance to a 3D position from the RobotSpec.
/// Multi-candidate part ids (two ToFs, two motors) pair sorted instance
/// names with candidates sorted left-to-right (+y first) — deterministic,
/// and the ambiguity this papers over is exactly what X01 will formalize.
fn resolve_positions(
    nl: &Netlist,
    robot: &RobotSpec,
    cat: &ElecCatalogue,
) -> Result<BTreeMap<String, ([f64; 3], String)>, String> {
    // Candidate pools: part id -> [(pos, kind)]
    let mut pools: BTreeMap<String, Vec<[f64; 3]>> = BTreeMap::new();
    for c in &robot.components {
        pools.entry(c.part.clone()).or_default().push(c.pos_mm);
    }
    for s in &robot.sensors {
        pools.entry(s.part.clone()).or_default().push(s.pos_mm);
    }
    for w in &robot.drive.wheels {
        if let Some(m) = &w.motor_part {
            let mut p = w.pos_mm;
            p[1] *= 0.6; // robotspec derive convention: motor inboard
            pools.entry(m.clone()).or_default().push(p);
        }
    }
    for v in pools.values_mut() {
        v.sort_by(|a, b| b[1].partial_cmp(&a[1]).unwrap()); // +y (left) first
    }

    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = BTreeMap::new();
    let mut instances: Vec<(&String, &String)> = nl.instances.iter().collect();
    instances.sort_by_key(|(i, _)| i.to_string());
    for (inst, part_id) in instances {
        let (part, _) = cat.get(part_id)?;
        let Some(pool) = pools.get(part_id) else {
            continue; // un-placed instance (no 3D home); skip rendering it
        };
        let idx = taken.entry(part_id.clone()).or_insert(0);
        if *idx >= pool.len() {
            return Err(format!(
                "netlist has more '{part_id}' instances than the robot places (X01 smell)"
            ));
        }
        out.insert(inst.clone(), (pool[*idx], part.kind.clone()));
        *idx += 1;
    }
    Ok(out)
}

fn role_text(d: &crate::catalogue::PinDecl) -> String {
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
        other => other.into(),
    }
}

fn class_carries(class: &str, volts: Option<f64>) -> String {
    match class {
        "vbat" => format!("{} V pack rail", volts.unwrap_or(7.4)),
        "v5" => "5 V BEC logic rail".into(),
        "v33" => "3.3 V sensor rail".into(),
        "gnd" => "common ground return".into(),
        "pwm" => "PWM drive command".into(),
        "uart" => "CRSF control stream".into(),
        "xshut" => "boot-reset control (address reassignment)".into(),
        _ => "signal".into(),
    }
}

fn class_note(class: &str) -> &'static str {
    match class {
        "vbat" => "Main pack power. E40 requires the master switch in this path before any load sees the battery.",
        "v5" => "From the ESC's BEC regulator; feeds the MCU board and receiver.",
        "v33" => "From the MCU's onboard regulator (300 mA budget); feeds both ToFs and the IMU.",
        "gnd" => "Common return, star-spliced at the loom junction.",
        "pwm" => "MCU-to-ESC drive; part of the E41 failsafe stop chain — neutral on link loss.",
        "uart" => "Receiver-to-MCU control link; frame loss here is what triggers the failsafe (E41).",
        "xshut" => "E20 recipe: hold this device in reset at boot while its twin is re-addressed, then release.",
        _ => "",
    }
}

fn wire_class(
    volts: Option<f64>,
    signal: Option<&str>,
    is_gnd: bool,
    is_motor: bool,
) -> String {
    if is_gnd {
        return "gnd".into();
    }
    if is_motor {
        return "motor".into();
    }
    if let Some(v) = volts {
        if v > 6.0 {
            return "vbat".into();
        }
        if v > 4.0 {
            return "v5".into();
        }
        return "v33".into();
    }
    match signal {
        Some("pwm") => "pwm".into(),
        Some("uart") => "uart".into(),
        _ => "sig".into(),
    }
}

/// Loom arc between two attach points, lifted at the midpoint.
fn arc(p0: [f64; 3], p1: [f64; 3], lift: f64) -> Vec<[f64; 3]> {
    let mid = [
        (p0[0] + p1[0]) / 2.0,
        (p0[1] + p1[1]) / 2.0,
        p0[2].max(p1[2]) + lift,
    ];
    let mut pts = Vec::with_capacity(13);
    for i in 0..=12 {
        let t = i as f64 / 12.0;
        let a = (1.0 - t) * (1.0 - t);
        let b = 2.0 * t * (1.0 - t);
        let c = t * t;
        pts.push([
            a * p0[0] + b * mid[0] + c * p1[0],
            a * p0[1] + b * mid[1] + c * p1[1],
            a * p0[2] + b * mid[2] + c * p1[2],
        ]);
    }
    pts
}

pub fn build_scene(
    nl: &Netlist,
    robot: &RobotSpec,
    cat: &ElecCatalogue,
) -> Result<String, String> {
    let positions = resolve_positions(nl, robot, cat)?;
    let attach = |inst: &str| -> Option<[f64; 3]> {
        positions.get(inst).map(|(p, kind)| {
            let s = size_for(kind);
            [p[0], p[1], p[2] + s[2] / 2.0]
        })
    };

    let MechSource::Parametric { chassis } = &robot.sources.mech;
    let (l, h, hn, lw) = (
        chassis.length_mm,
        chassis.rear_height_mm,
        chassis.nose_height_mm,
        chassis.wedge_length_mm,
    );
    let scene_chassis = ChassisGeo {
        profile: vec![
            [-l / 2.0, 0.0],
            [l / 2.0, 0.0],
            [l / 2.0, hn],
            [l / 2.0 - lw, h],
            [-l / 2.0, h],
        ],
        half_width: chassis.width_mm / 2.0,
    };

    // Wheel geometry from the robot; radius/width defaults if the part
    // lacks them (mass-only catalogue entries).
    let wheels: Vec<WheelGeo> = robot
        .drive
        .wheels
        .iter()
        .map(|w| WheelGeo { pos: w.pos_mm, radius: 16.0, width: 8.0 })
        .collect();

    // Pin-declaration lookup for explanations.
    let decl_of = |endpoint: &str| -> Option<(String, crate::catalogue::PinDecl)> {
        let (inst, pin) = split_pin(endpoint).ok()?;
        let part_id = nl.instances.get(inst)?;
        let (part, _) = cat.get(part_id).ok()?;
        part.elec
            .as_ref()
            .and_then(|e| e.pins.get(pin))
            .map(|d| (endpoint.to_string(), d.clone()))
    };
    let end_detail = |endpoint: &str| -> String {
        match decl_of(endpoint) {
            Some((e, d)) => format!("{e} — {}", role_text(&d)),
            None => endpoint.to_string(),
        }
    };

    // Per-net class map (shared by wires and node connection lists).
    let mut net_class: BTreeMap<String, String> = BTreeMap::new();
    for net in &nl.nets {
        let is_gnd = net.volts.is_none()
            && net.signal.is_none()
            && net.pins.iter().any(|p| p.ends_with(".GND") || p.ends_with(".-"));
        let is_motor = net.pins.iter().any(|p| {
            decl_of(p).map_or(false, |(_, d)| d.role == "motor_in" || d.role == "motor_out")
        });
        net_class.insert(
            net.id.clone(),
            wire_class(net.volts, net.signal.as_deref(), is_gnd, is_motor),
        );
    }

    // Plain-English naming with left/right from actual placement.
    let side_of = |inst: &str| -> &'static str {
        match positions.get(inst) {
            Some((p, _)) if p[1] > 1.0 => "left ",
            Some((p, _)) if p[1] < -1.0 => "right ",
            _ => "",
        }
    };
    let who = |inst: &str| -> String {
        let kind = positions.get(inst).map(|(_, k)| k.as_str()).unwrap_or("");
        let side = side_of(inst);
        let noun = match kind {
            "battery" => "the battery pack".to_string(),
            "switch" => "the power switch".to_string(),
            "esc" => "the motor controller".to_string(),
            "mcu" => "the brain".to_string(),
            "radio" => "the radio receiver".to_string(),
            "tof" => format!("the {side}floor sensor"),
            "imu" => "the motion sensor".to_string(),
            "motor" => format!("the {side}drive motor"),
            _ => format!("'{inst}'"),
        };
        format!("{noun} ({inst})")
    };
    let insts_of = |pins: &[String]| -> Vec<String> {
        let mut seen = Vec::new();
        for p in pins {
            if let Ok((i, _)) = split_pin(p) {
                if !seen.contains(&i.to_string()) {
                    seen.push(i.to_string());
                }
            }
        }
        seen
    };
    let list_who = |insts: &[String], skip: &str| -> String {
        let named: Vec<String> =
            insts.iter().filter(|i| *i != skip).map(|i| who(i)).collect();
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
    let wire_about = |class: &str, net: &crate::schema::Net| -> String {
        let insts = insts_of(&net.pins);
        match class {
            "vbat" => format!(
                "The robot's main power line: raw {} V battery power flowing from {} to {}. \
                 Nothing downstream runs without it.",
                net.volts.unwrap_or(7.4),
                who(&insts[0]),
                list_who(&insts, &insts[0]),
            ),
            "v5" => format!(
                "The 5-volt supply. The motor controller's built-in regulator makes clean 5 V \
                 from battery power and feeds it to {} — this is what keeps them alive.",
                list_who(&insts, "esc"),
            ),
            "v33" => format!(
                "The 3.3-volt sensor supply. The brain's onboard regulator powers {} through \
                 this line.",
                list_who(&insts, "mcu"),
            ),
            "gnd" => "The shared ground return. Every component's current flows back to the \
                      battery through this line — it is the other half of every circuit on \
                      the robot."
                .to_string(),
            "motor" => {
                let m = insts
                    .iter()
                    .find(|i| positions.get(*i).map_or(false, |(_, k)| k == "motor"));
                format!(
                    "Motor power: the motor controller pushes current down this wire to spin \
                     {}. Reverse the current and the wheel reverses.",
                    m.map(|i| who(i)).unwrap_or_else(|| "the motor".into()),
                )
            }
            "pwm" => {
                let target = insts
                    .iter()
                    .find(|i| positions.get(*i).map_or(false, |(_, k)| k == "esc"));
                format!(
                    "A drive command line: the brain tells {} how fast to run one motor \
                     channel by sending timed pulses (PWM) down this wire. On radio loss the \
                     failsafe holds it at neutral, which is what stops the robot.",
                    target.map(|i| who(i)).unwrap_or_else(|| "the ESC".into()),
                )
            }
            "uart" => "The control link: the radio receiver streams the driver's stick \
                       positions to the brain over this wire. If those frames stop arriving, \
                       the brain knows the radio is gone and triggers the failsafe."
                .to_string(),
            _ => "A signal line.".to_string(),
        }
    };

    let nodes: Vec<Node> = positions
        .iter()
        .map(|(inst, (pos, kind))| {
            let part_id = &nl.instances[inst];
            let (part, hash) = cat.get(part_id).expect("resolved earlier");
            let mut detail: Vec<[String; 2]> = vec![
                ["part".into(), format!("{part_id} @{}", &hash[..8])],
                ["kind".into(), part.kind.clone()],
                ["mass".into(), format!("{} g", part.mass_g)],
            ];
            if let Some(elec) = &part.elec {
                if let Some(src) = &elec.source {
                    detail.push([
                        "source".into(),
                        format!(
                            "{} V{}{}",
                            src.volts,
                            src.c_rating.map(|c| format!(", {c}C")).unwrap_or_default(),
                            src.capacity_mah
                                .map(|m| format!(", {m} mAh"))
                                .unwrap_or_default()
                        ),
                    ]);
                }
                if let Some(bus) = &elec.bus {
                    detail.push([
                        "bus".into(),
                        format!(
                            "{} default {}{}",
                            bus.kind,
                            bus.default_addr,
                            if bus.addr_reassignable {
                                " (reassignable via XSHUT)"
                            } else {
                                ""
                            }
                        ),
                    ]);
                }
            }
            if !part.notes.is_empty() {
                detail.push(["notes".into(), part.notes.clone()]);
            }

            // Connection rows: every net, bus and XSHUT touching this instance.
            let prefix = format!("{inst}.");
            let mut conns: Vec<Conn> = Vec::new();
            for net in &nl.nets {
                for p in &net.pins {
                    if let Some(pin) = p.strip_prefix(&prefix) {
                        let others: Vec<&str> = net
                            .pins
                            .iter()
                            .filter(|q| *q != p)
                            .map(|s| s.as_str())
                            .collect();
                        conns.push(Conn {
                            pin: pin.to_string(),
                            role: decl_of(p).map(|(_, d)| role_text(&d)).unwrap_or_default(),
                            net: net.id.clone(),
                            class: net_class[&net.id].clone(),
                            others: others.join(" · "),
                        });
                    }
                }
            }
            for bus in &nl.buses {
                for endpoint in [&bus.sda, &bus.scl] {
                    if let Some(pin) = endpoint.strip_prefix(&prefix) {
                        conns.push(Conn {
                            pin: pin.to_string(),
                            role: decl_of(endpoint).map(|(_, d)| role_text(&d)).unwrap_or_default(),
                            net: bus.id.clone(),
                            class: "i2c".into(),
                            others: format!("{} devices on the bus", bus.devices.len()),
                        });
                    }
                }
                for dev in &bus.devices {
                    if dev.inst == **inst {
                        let addr = dev.reassign_to.as_ref().unwrap_or(&dev.addr);
                        conns.push(Conn {
                            pin: "SDA/SCL".into(),
                            role: format!("bus device at {addr}"),
                            net: bus.id.clone(),
                            class: "i2c".into(),
                            others: format!("master {} · {}", bus.sda, bus.scl),
                        });
                    }
                    if let Some(x) = &dev.xshut {
                        if let Some(pin) = x.strip_prefix(&prefix) {
                            conns.push(Conn {
                                pin: pin.to_string(),
                                role: "XSHUT driver".into(),
                                net: format!("XSHUT {}", dev.inst),
                                class: "xshut".into(),
                                others: format!("{}.XSHUT", dev.inst),
                            });
                        }
                        if dev.inst == **inst {
                            conns.push(Conn {
                                pin: "XSHUT".into(),
                                role: "reset input (E20 reassignment)".into(),
                                net: format!("XSHUT {}", dev.inst),
                                class: "xshut".into(),
                                others: x.clone(),
                            });
                        }
                    }
                }
            }

            let mut about = if part.description.is_empty() {
                format!("A {} (no catalogue description yet).", part.kind)
            } else {
                part.description.clone()
            };
            let side = side_of(inst).trim();
            if !side.is_empty() && (kind == "tof" || kind == "motor") {
                about.push_str(&format!(" This unit handles the {side} side."));
            }
            Node {
                id: (*inst).clone(),
                part: part_id.clone(),
                pos: *pos,
                size: size_for(kind),
                kind: kind.clone(),
                about,
                detail,
                conns,
            }
        })
        .collect();

    let mut wires = Vec::new();
    for (wi, net) in nl.nets.iter().enumerate() {
        let class = net_class[&net.id].clone();
        let carries = class_carries(&class, net.volts);
        let about = wire_about(&class, net);
        let note = class_note(&class).to_string();
        let ends_detail: Vec<String> = net.pins.iter().map(|p| end_detail(p)).collect();
        let ends: Vec<[f64; 3]> = net
            .pins
            .iter()
            .filter_map(|p| split_pin(p).ok().and_then(|(i, _)| attach(i)))
            .collect();
        let end_names: Vec<String> = net.pins.clone();
        let lift = 10.0 + (wi % 5) as f64 * 3.0;
        if ends.len() == 2 {
            wires.push(Wire {
                id: net.id.clone(),
                class,
                dashed: false,
                pts: arc(ends[0], ends[1], lift),
                ends: end_names,
                carries,
                about,
                note,
                ends_detail,
            });
        } else if ends.len() > 2 {
            // Star junction: a splice node at the centroid.
            let n = ends.len() as f64;
            let j = [
                ends.iter().map(|p| p[0]).sum::<f64>() / n,
                ends.iter().map(|p| p[1]).sum::<f64>() / n,
                ends.iter().map(|p| p[2]).sum::<f64>() / n + lift,
            ];
            for (k, e) in ends.iter().enumerate() {
                wires.push(Wire {
                    id: format!("{}~{}", net.id, k),
                    class: class.clone(),
                    dashed: false,
                    pts: arc(*e, j, 4.0),
                    ends: end_names.clone(),
                    carries: carries.clone(),
                    about: about.clone(),
                    note: note.clone(),
                    ends_detail: ends_detail.clone(),
                });
            }
        }
    }
    for bus in &nl.buses {
        let (m_inst, _) = split_pin(&bus.sda)?;
        let Some(master) = attach(m_inst) else { continue };
        // Final address map for the bus-level explanation.
        let addr_map: Vec<String> = bus
            .devices
            .iter()
            .map(|d| {
                format!(
                    "{} @{}{}",
                    d.inst,
                    d.reassign_to.as_ref().unwrap_or(&d.addr),
                    if d.reassign_to.is_some() { " (reassigned)" } else { "" }
                )
            })
            .collect();
        for (off, dashed) in [(-1.5f64, false), (1.5, true)] {
            // SDA solid, SCL dashed, twisted-pair offset.
            let line = if dashed { "SCL" } else { "SDA" };
            for dev in &bus.devices {
                if let Some(mut d) = attach(&dev.inst) {
                    d[1] += off;
                    let mut m = master;
                    m[1] += off;
                    let final_addr = dev.reassign_to.as_ref().unwrap_or(&dev.addr);
                    wires.push(Wire {
                        id: format!("{} {}", bus.id, line),
                        class: "i2c".into(),
                        dashed,
                        pts: arc(m, d, 16.0 + off.abs() * 2.0),
                        ends: vec![bus.sda.clone(), format!("{}.SDA/SCL", dev.inst)],
                        carries: format!("I2C {line} — {} at {final_addr}", dev.inst),
                        about: format!(
                            "The sensor party line ({line}): the brain polls each sensor in \
                             turn over this shared two-wire bus — one wire carries the data \
                             (SDA), the other the clock that paces it (SCL). This branch goes \
                             to {}, which answers only to address {final_addr}.",
                            who(&dev.inst),
                        ),
                        note: format!("Bus map after reassignment: {}", addr_map.join(", ")),
                        ends_detail: vec![
                            end_detail(if dashed { &bus.scl } else { &bus.sda }),
                            format!("{}.{line} — bus device at {final_addr}", dev.inst),
                        ],
                    });
                }
            }
        }
        for dev in &bus.devices {
            if let Some(x) = &dev.xshut {
                let (xi, _) = split_pin(x)?;
                if let (Some(a), Some(b)) = (attach(xi), attach(&dev.inst)) {
                    wires.push(Wire {
                        id: format!("XSHUT {}", dev.inst),
                        class: "xshut".into(),
                        dashed: true,
                        pts: arc(a, b, 22.0),
                        ends: vec![x.clone(), format!("{}.XSHUT", dev.inst)],
                        carries: class_carries("xshut", None),
                        about: format!(
                            "A wake-up line for address surgery: both floor sensors boot at \
                             the same address, so at power-up the brain holds {} in reset \
                             through this wire while it renames the other one. After that, \
                             both can share the bus without talking over each other.",
                            who(&dev.inst),
                        ),
                        note: class_note("xshut").to_string(),
                        ends_detail: vec![
                            end_detail(x),
                            format!("{}.XSHUT — reset input", dev.inst),
                        ],
                    });
                }
            }
        }
    }

    let scene = Scene {
        name: nl.name.clone(),
        chassis: scene_chassis,
        wheels,
        skids: robot.skids.clone(),
        nodes,
        wires,
    };
    let data = serde_json::to_string(&scene).map_err(|e| e.to_string())?;
    const PLACEHOLDER: &str = "//__SCENE__\n{};";
    let template = include_str!("../templates/view.html");
    if !template.contains(PLACEHOLDER) {
        return Err("view template missing //__SCENE__ placeholder".into());
    }
    Ok(template.replace(PLACEHOLDER, &format!("{data};")))
}
