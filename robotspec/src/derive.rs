//! The derivation pipeline: pure functions from (spec, catalogue) to derived
//! quantities. Each stage is public so consumers can compose what they need
//! (the viewer's ledger, arena-plant's mass properties, the MCP servers'
//! re-derive-per-edit loop) without dragging the rest.

use crate::catalogue::Catalogue;
use crate::checks::{run_checks, CheckResult};
use crate::geom::{dist_point_to_segment, polygon_area_centroid};
use crate::identity::identity_hashes;
use crate::schema::{MechSource, RobotSpec, WedgeChassis};
use crate::{DERIVATION_PIPELINE_VERSION, GRAVITY, SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DerivedRecord {
    pub pipeline_version: String,
    pub schema_version: String,
    pub resolved_parts: BTreeMap<String, String>,
    pub mass_total_g: f64,
    pub mass_chassis_g: f64,
    pub mass_parts_g: f64,
    pub budget_margin_g: f64,
    pub cog_mm: [f64; 3],
    /// Yaw inertia about the CoG, g·mm² (point-mass + plate approximation, v0).
    pub yaw_inertia_gmm2: f64,
    pub footprint_mm: [f64; 2],
    pub bbox_mm: [f64; 3],
    pub cube_fit: bool,
    pub support_polygon_mm: Vec<[f64; 2]>,
    pub worst_tip_edge: String,
    pub worst_tip_distance_mm: f64,
    pub worst_tip_angle_deg: f64,
    pub static_tip_energy_mj: f64,
    pub brake_pitch_limit_ms2: f64,
    pub checks: Vec<CheckResult>,
    pub body_hash: String,
    pub robot_hash: String,
}

/// A point mass in the roll-up (chassis plate or placed part).
#[derive(Clone, Copy, Debug)]
pub struct MassCarrier {
    pub mass_g: f64,
    pub pos_mm: [f64; 3],
}

/// Stage 1: the parametric chassis as plate masses.
pub fn chassis_plates(c: &WedgeChassis) -> Result<Vec<MassCarrier>, String> {
    let rho = *c
        .density_map
        .get(&c.material)
        .ok_or_else(|| format!("material '{}' not in density_map", c.material))?
        / 1000.0; // g/cm^3 -> g/mm^3
    let (l, w, h, hn, lw, t) = (
        c.length_mm,
        c.width_mm,
        c.rear_height_mm,
        c.nose_height_mm,
        c.wedge_length_mm,
        c.wall_mm,
    );
    if lw >= l {
        return Err("wedge_length must be < length".into());
    }
    let mut plates = Vec::new();
    plates.push(MassCarrier {
        mass_g: l * w * t * rho,
        pos_mm: [0.0, 0.0, t / 2.0],
    });
    // Side profile polygon (x rear -> nose), minus the base strip.
    let profile = [
        [-l / 2.0, t],
        [l / 2.0, t],
        [l / 2.0, hn.max(t)],
        [l / 2.0 - lw, h],
        [-l / 2.0, h],
    ];
    let (area, cen) = polygon_area_centroid(&profile);
    for side in [-1.0, 1.0] {
        plates.push(MassCarrier {
            mass_g: area * t * rho,
            pos_mm: [cen[0], side * (w - t) / 2.0, cen[1]],
        });
    }
    plates.push(MassCarrier {
        mass_g: w * (h - t) * t * rho,
        pos_mm: [-(l - t) / 2.0, 0.0, (h + t) / 2.0],
    });
    let slope = (lw * lw + (h - hn) * (h - hn)).sqrt();
    plates.push(MassCarrier {
        mass_g: slope * w * t * rho,
        pos_mm: [l / 2.0 - lw / 2.0, 0.0, (h + hn) / 2.0],
    });
    Ok(plates)
}

/// Stage 2: every mass carrier (chassis + resolved parts), plus the resolved
/// part-hash map that feeds identity.
pub fn mass_carriers(
    spec: &RobotSpec,
    cat: &Catalogue,
) -> Result<(Vec<MassCarrier>, BTreeMap<String, String>, f64), String> {
    let MechSource::Parametric { chassis } = &spec.sources.mech;
    let plates = chassis_plates(chassis)?;
    let mass_chassis: f64 = plates.iter().map(|p| p.mass_g).sum();
    let mut carriers = plates;
    let mut resolved = BTreeMap::new();
    for wheel in &spec.drive.wheels {
        let (part, hash) = cat.get(&wheel.part)?;
        resolved.insert(part.id.clone(), hash.clone());
        carriers.push(MassCarrier { mass_g: part.mass_g, pos_mm: wheel.pos_mm });
        if let Some(motor_id) = &wheel.motor_part {
            let (motor, mhash) = cat.get(motor_id)?;
            resolved.insert(motor.id.clone(), mhash.clone());
            let mut mpos = wheel.pos_mm;
            mpos[1] *= 0.6; // motor sits inboard of its wheel
            carriers.push(MassCarrier { mass_g: motor.mass_g, pos_mm: mpos });
        }
    }
    for s in &spec.sensors {
        let (part, hash) = cat.get(&s.part)?;
        resolved.insert(part.id.clone(), hash.clone());
        carriers.push(MassCarrier { mass_g: part.mass_g, pos_mm: s.pos_mm });
    }
    for c in &spec.components {
        let (part, hash) = cat.get(&c.part)?;
        resolved.insert(part.id.clone(), hash.clone());
        carriers.push(MassCarrier { mass_g: part.mass_g, pos_mm: c.pos_mm });
    }
    Ok((carriers, resolved, mass_chassis))
}

/// Stage 3: mass properties from carriers.
pub fn mass_properties(carriers: &[MassCarrier]) -> (f64, [f64; 3], f64) {
    let total: f64 = carriers.iter().map(|c| c.mass_g).sum();
    let mut cog = [0.0f64; 3];
    for c in carriers {
        for k in 0..3 {
            cog[k] += c.mass_g * c.pos_mm[k];
        }
    }
    for v in cog.iter_mut() {
        *v /= total;
    }
    let yaw: f64 = carriers
        .iter()
        .map(|c| {
            let (dx, dy) = (c.pos_mm[0] - cog[0], c.pos_mm[1] - cog[1]);
            c.mass_g * (dx * dx + dy * dy)
        })
        .sum();
    (total, cog, yaw)
}

/// Stage 4: support polygon (wheel contacts + skids), CCW-ordered.
pub fn support_polygon(spec: &RobotSpec) -> Vec<[f64; 2]> {
    let mut pts: Vec<[f64; 2]> = spec
        .drive
        .wheels
        .iter()
        .map(|w| [w.pos_mm[0], w.pos_mm[1]])
        .chain(spec.skids.iter().copied())
        .collect();
    let n = pts.len() as f64;
    let cx = pts.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = pts.iter().map(|p| p[1]).sum::<f64>() / n;
    pts.sort_by(|a, b| {
        (a[1] - cy)
            .atan2(a[0] - cx)
            .partial_cmp(&(b[1] - cy).atan2(b[0] - cx))
            .unwrap()
    });
    pts
}

/// Stage 5: static tip metrics from CoG + support polygon.
pub struct TipMetrics {
    pub worst_edge: String,
    pub distance_mm: f64,
    pub angle_deg: f64,
    pub energy_mj: f64,
    pub brake_pitch_limit_ms2: f64,
}

pub fn tip_metrics(mass_g: f64, cog_mm: [f64; 3], support: &[[f64; 2]]) -> TipMetrics {
    let mut worst = (f64::INFINITY, String::new());
    for i in 0..support.len() {
        let a = support[i];
        let b = support[(i + 1) % support.len()];
        let d = dist_point_to_segment([cog_mm[0], cog_mm[1]], a, b);
        let (mx, my) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
        let label = if mx.abs() > my.abs() {
            if mx > 0.0 { "front" } else { "rear" }
        } else if my > 0.0 {
            "left"
        } else {
            "right"
        };
        if d < worst.0 {
            worst = (d, label.to_string());
        }
    }
    let (d_mm, h_mm) = (worst.0, cog_mm[2]);
    let (d_m, h_m, m_kg) = (d_mm / 1000.0, h_mm / 1000.0, mass_g / 1000.0);
    let x_front = support.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
    TipMetrics {
        worst_edge: worst.1,
        distance_mm: d_mm,
        angle_deg: (d_mm / h_mm).atan().to_degrees(),
        energy_mj: m_kg * GRAVITY * ((h_m * h_m + d_m * d_m).sqrt() - h_m) * 1000.0,
        brake_pitch_limit_ms2: GRAVITY * (x_front - cog_mm[0]).max(0.0) / h_mm.max(1e-9),
    }
}

/// The full pipeline: compose all stages into the derived record.
pub fn derive(spec: &RobotSpec, cat: &Catalogue) -> Result<DerivedRecord, String> {
    let MechSource::Parametric { chassis } = &spec.sources.mech;
    let (carriers, resolved, mass_chassis) = mass_carriers(spec, cat)?;
    let (mass_total, cog, yaw) = mass_properties(&carriers);

    let wheel_top = spec
        .drive
        .wheels
        .iter()
        .map(|w| {
            let r = cat
                .get(&w.part)
                .ok()
                .and_then(|(p, _)| p.wheel_radius_mm)
                .unwrap_or(0.0);
            w.pos_mm[2] + r
        })
        .fold(0.0f64, f64::max);
    let bbox = [
        chassis.length_mm,
        chassis.width_mm,
        chassis.rear_height_mm.max(wheel_top),
    ];

    let support = support_polygon(spec);
    let tips = tip_metrics(mass_total, cog, &support);
    let checks = run_checks(spec, cat, mass_total, bbox)?;
    let (body_hash, robot_hash) = identity_hashes(spec, &resolved);

    Ok(DerivedRecord {
        pipeline_version: DERIVATION_PIPELINE_VERSION.to_string(),
        schema_version: SCHEMA_VERSION.to_string(),
        resolved_parts: resolved,
        mass_total_g: mass_total,
        mass_chassis_g: mass_chassis,
        mass_parts_g: mass_total - mass_chassis,
        budget_margin_g: crate::WEIGHT_LIMIT_G - mass_total,
        cog_mm: cog,
        yaw_inertia_gmm2: yaw,
        footprint_mm: [chassis.length_mm, chassis.width_mm],
        bbox_mm: bbox,
        cube_fit: bbox.iter().all(|&d| d <= crate::CUBE_MM),
        support_polygon_mm: support,
        worst_tip_edge: tips.worst_edge,
        worst_tip_distance_mm: tips.distance_mm,
        worst_tip_angle_deg: tips.angle_deg,
        static_tip_energy_mj: tips.energy_mj,
        brake_pitch_limit_ms2: tips.brake_pitch_limit_ms2,
        checks,
        body_hash,
        robot_hash,
    })
}
