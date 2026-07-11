//! The inspector (specs/robotspec-viewer.md), repo edition. Skips the
//! prototype's private-derivation stage entirely: the ledger displays the
//! record computed by THIS crate's pipeline — one derivation codebase by
//! construction (viewer M1 acceptance), robot hash in the HUD.
//!
//! Scene-builder duplication with robowire::view is known and flagged: the
//! shared scene library is the Phase-2 extraction (viewer spec §2 contract 1).

use crate::catalogue::Catalogue;
use crate::derive::DerivedRecord;
use crate::schema::{MechSource, RobotSpec};
use serde::Serialize;

#[derive(Serialize)]
struct Scene {
    name: String,
    revision: String,
    robot_hash: String,
    body_hash: String,
    pipeline: String,
    chassis: ChassisGeo,
    wheels: Vec<WheelGeo>,
    nodes: Vec<Node>,
    cones: Vec<Cone>,
    cog: [f64; 3],
    support: Vec<[f64; 2]>,
    tip: TipInfo,
    cube_mm: f64,
    cube_fit: bool,
    bbox: [f64; 3],
    ledger: Vec<LedgerRow>,
    checks: Vec<CheckRow>,
}

#[derive(Serialize)]
struct ChassisGeo {
    profile: Vec<[f64; 2]>,
    half_width: f64,
}

#[derive(Serialize)]
struct WheelGeo {
    pos: [f64; 3],
    radius: f64,
    width: f64,
}

#[derive(Serialize)]
struct Node {
    id: String,
    part: String,
    kind: String,
    pos: [f64; 3],
    size: [f64; 3],
    about: String,
    mass_g: f64,
}

#[derive(Serialize)]
struct Cone {
    apex: [f64; 3],
    dir: [f64; 3],
    half_angle_deg: f64,
    length: f64,
    label: String,
}

#[derive(Serialize)]
struct TipInfo {
    edge: String,
    distance_mm: f64,
    angle_deg: f64,
    energy_mj: f64,
}

#[derive(Serialize)]
struct LedgerRow {
    k: String,
    v: String,
    verdict: String, // "ok" | "warn" | "bad" | ""
}

#[derive(Serialize)]
struct CheckRow {
    code: String,
    desc: String,
    pass: bool,
    detail: String,
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
        _ => [12.0, 12.0, 6.0],
    }
}

pub fn build_inspector(
    spec: &RobotSpec,
    cat: &Catalogue,
    derived: &DerivedRecord,
) -> Result<String, String> {
    let MechSource::Parametric { chassis } = &spec.sources.mech;
    let (l, h, hn, lw) = (
        chassis.length_mm,
        chassis.rear_height_mm,
        chassis.nose_height_mm,
        chassis.wedge_length_mm,
    );

    let mut nodes = Vec::new();
    let mut cones = Vec::new();
    for c in &spec.components {
        let (part, _) = cat.get(&c.part)?;
        nodes.push(Node {
            id: c.id.clone(),
            part: part.id.clone(),
            kind: part.kind.clone(),
            pos: c.pos_mm,
            size: size_for(&part.kind),
            about: part.description.clone(),
            mass_g: part.mass_g,
        });
    }
    for s in &spec.sensors {
        let (part, _) = cat.get(&s.part)?;
        nodes.push(Node {
            id: s.id.clone(),
            part: part.id.clone(),
            kind: part.kind.clone(),
            pos: s.pos_mm,
            size: size_for(&part.kind),
            about: part.description.clone(),
            mass_g: part.mass_g,
        });
        if let (Some(fov), Some(range)) = (part.fov_deg, part.range_mm) {
            // True FoV, true range; downward cones clip at the floor.
            let norm = (s.dir[0] * s.dir[0] + s.dir[1] * s.dir[1] + s.dir[2] * s.dir[2])
                .sqrt()
                .max(1e-9);
            let dir = [s.dir[0] / norm, s.dir[1] / norm, s.dir[2] / norm];
            let length = if dir[2] < -1e-6 {
                (s.pos_mm[2] / -dir[2]).min(range)
            } else {
                range.min(80.0) // upward/lateral cones drawn truncated
            };
            cones.push(Cone {
                apex: s.pos_mm,
                dir,
                half_angle_deg: fov / 2.0,
                length,
                label: s.id.clone(),
            });
        }
    }
    let mut wheels = Vec::new();
    for w in &spec.drive.wheels {
        let (part, _) = cat.get(&w.part)?;
        wheels.push(WheelGeo {
            pos: w.pos_mm,
            radius: part.wheel_radius_mm.unwrap_or(16.0),
            width: part.wheel_width_mm.unwrap_or(8.0),
        });
        if let Some(m) = &w.motor_part {
            let (mp, _) = cat.get(m)?;
            let mut pos = w.pos_mm;
            pos[1] *= 0.6;
            nodes.push(Node {
                id: format!("motor{}", if w.pos_mm[1] >= 0.0 { "_l" } else { "_r" }),
                part: mp.id.clone(),
                kind: mp.kind.clone(),
                pos,
                size: [15.0, 11.0, 11.0],
                about: mp.description.clone(),
                mass_g: mp.mass_g,
            });
        }
    }

    let d = derived;
    let ledger = vec![
        LedgerRow {
            k: "total mass".into(),
            v: format!("{:.1} g of {} g", d.mass_total_g, crate::WEIGHT_LIMIT_G),
            verdict: if d.mass_total_g <= crate::WEIGHT_LIMIT_G { "ok" } else { "bad" }.into(),
        },
        LedgerRow {
            k: "budget margin".into(),
            v: format!("{:.1} g", d.budget_margin_g),
            verdict: if d.budget_margin_g >= 30.0 {
                "ok"
            } else if d.budget_margin_g >= 0.0 {
                "warn"
            } else {
                "bad"
            }
            .into(),
        },
        LedgerRow {
            k: "mass split".into(),
            v: format!("chassis {:.1} g · parts {:.1} g", d.mass_chassis_g, d.mass_parts_g),
            verdict: "".into(),
        },
        LedgerRow {
            k: "CoG".into(),
            v: format!("({:.1}, {:.1}, {:.1}) mm", d.cog_mm[0], d.cog_mm[1], d.cog_mm[2]),
            verdict: "".into(),
        },
        LedgerRow {
            k: "yaw inertia".into(),
            v: format!("{:.0} g·mm²", d.yaw_inertia_gmm2),
            verdict: "".into(),
        },
        LedgerRow {
            k: "cube fit".into(),
            v: format!(
                "{:.1} × {:.1} × {:.1} mm — {}",
                d.bbox_mm[0],
                d.bbox_mm[1],
                d.bbox_mm[2],
                if d.cube_fit { "FITS" } else { "VIOLATION" }
            ),
            verdict: if d.cube_fit { "ok" } else { "bad" }.into(),
        },
        LedgerRow {
            k: "worst tip edge".into(),
            v: format!(
                "{} · {:.1} mm · {:.1}° · {:.2} mJ",
                d.worst_tip_edge, d.worst_tip_distance_mm, d.worst_tip_angle_deg, d.static_tip_energy_mj
            ),
            verdict: if d.worst_tip_angle_deg > 40.0 { "ok" } else { "warn" }.into(),
        },
        LedgerRow {
            k: "brake pitch limit".into(),
            v: format!("{:.1} m/s²", d.brake_pitch_limit_ms2),
            verdict: "".into(),
        },
    ];

    let scene = Scene {
        name: spec.identity.name.clone(),
        revision: spec.identity.revision.clone(),
        robot_hash: d.robot_hash.clone(),
        body_hash: d.body_hash.clone(),
        pipeline: d.pipeline_version.clone(),
        chassis: ChassisGeo {
            profile: vec![
                [-l / 2.0, 0.0],
                [l / 2.0, 0.0],
                [l / 2.0, hn],
                [l / 2.0 - lw, h],
                [-l / 2.0, h],
            ],
            half_width: chassis.width_mm / 2.0,
        },
        wheels,
        nodes,
        cones,
        cog: d.cog_mm,
        support: d.support_polygon_mm.clone(),
        tip: TipInfo {
            edge: d.worst_tip_edge.clone(),
            distance_mm: d.worst_tip_distance_mm,
            angle_deg: d.worst_tip_angle_deg,
            energy_mj: d.static_tip_energy_mj,
        },
        cube_mm: crate::CUBE_MM,
        cube_fit: d.cube_fit,
        bbox: d.bbox_mm,
        ledger,
        checks: d
            .checks
            .iter()
            .map(|c| CheckRow {
                code: c.code.clone(),
                desc: c.description.clone(),
                pass: c.pass,
                detail: c.detail.clone(),
            })
            .collect(),
    };

    let data = serde_json::to_string(&scene).map_err(|e| e.to_string())?;
    const PLACEHOLDER: &str = "//__SCENE__\n{};";
    let template = include_str!("../templates/inspector.html");
    if !template.contains(PLACEHOLDER) {
        return Err("inspector template missing //__SCENE__ placeholder".into());
    }
    Ok(template.replace(PLACEHOLDER, &format!("{data};")))
}
