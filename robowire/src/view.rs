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
}

#[derive(Serialize)]
struct Wire {
    id: String,
    class: String,
    dashed: bool,
    pts: Vec<[f64; 3]>,
    ends: Vec<String>,
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

fn wire_class(volts: Option<f64>, signal: Option<&str>, is_gnd: bool) -> String {
    if is_gnd {
        return "gnd".into();
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

    let nodes: Vec<Node> = positions
        .iter()
        .map(|(inst, (pos, kind))| Node {
            id: inst.clone(),
            part: nl.instances[inst].clone(),
            pos: *pos,
            size: size_for(kind),
            kind: kind.clone(),
        })
        .collect();

    let mut wires = Vec::new();
    for (wi, net) in nl.nets.iter().enumerate() {
        let is_gnd = net.volts.is_none()
            && net.signal.is_none()
            && net.pins.iter().any(|p| p.ends_with(".GND") || p.ends_with(".-"));
        let class = wire_class(net.volts, net.signal.as_deref(), is_gnd);
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
                });
            }
        }
    }
    for bus in &nl.buses {
        let (m_inst, _) = split_pin(&bus.sda)?;
        let Some(master) = attach(m_inst) else { continue };
        for (off, dashed) in [(-1.5f64, false), (1.5, true)] {
            // SDA solid, SCL dashed, twisted-pair offset.
            for dev in &bus.devices {
                if let Some(mut d) = attach(&dev.inst) {
                    d[1] += off;
                    let mut m = master;
                    m[1] += off;
                    wires.push(Wire {
                        id: format!("{} {}", bus.id, if dashed { "SCL" } else { "SDA" }),
                        class: "i2c".into(),
                        dashed,
                        pts: arc(m, d, 16.0 + off.abs() * 2.0),
                        ends: vec![bus.sda.clone(), format!("{}.SDA/SCL", dev.inst)],
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
