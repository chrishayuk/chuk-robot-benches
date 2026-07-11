//! RobotSpec → dynamic plant binding: the moment the derivation chain
//! becomes load-bearing. Every plant parameter comes from the derived
//! record or the parts catalogue — nothing hand-entered (X03 across the
//! module boundary). The episode cites the robot hash this returns.

use crate::dynamic::{BatterySpec, MotorCurve, RigidBotSpec, WheelSide, WheelSpec};
use arena_core::Vec2;
use robotspec::catalogue::Catalogue;
use robotspec::derive::DerivedRecord;
use robotspec::schema::{MechSource, RobotSpec};
use std::path::Path;

pub struct BoundRobot {
    pub spec: RigidBotSpec,
    pub derived: DerivedRecord,
    pub robot_hash: String,
    pub body_hash: String,
    pub kernel_ref: String,
    /// Traceability: every mapping decision, human-readable.
    pub notes: Vec<String>,
}

pub fn bind_robot(robot: &RobotSpec, cat: &Catalogue) -> Result<BoundRobot, String> {
    let derived = robotspec::derive(robot, cat)?;
    let MechSource::Parametric { chassis } = &robot.sources.mech;
    let mut notes = Vec::new();

    // Wheels + motor + tyre from the drive section.
    if robot.drive.wheels.is_empty() {
        return Err("robot has no wheels".into());
    }
    let mut wheels = Vec::new();
    let mut motor_part = None;
    let mut wheel_part = None;
    for w in &robot.drive.wheels {
        if !w.driven {
            continue;
        }
        wheels.push(WheelSpec {
            pos: Vec2::new(w.pos_mm[0] / 1000.0, w.pos_mm[1] / 1000.0),
            side: if w.pos_mm[1] >= 0.0 { WheelSide::Left } else { WheelSide::Right },
        });
        wheel_part = Some(cat.get(&w.part)?.0.clone());
        if let Some(m) = &w.motor_part {
            motor_part = Some(cat.get(m)?.0.clone());
        }
    }
    let wheel_part = wheel_part.ok_or("no driven wheel parts")?;
    let motor_part = motor_part.ok_or("no motor parts on driven wheels")?;
    let motor_props = motor_part
        .motor
        .as_ref()
        .ok_or_else(|| format!("part '{}' has no motor props", motor_part.id))?;
    let tyre = wheel_part
        .tyre
        .as_ref()
        .ok_or_else(|| format!("part '{}' has no tyre props", wheel_part.id))?;
    let radius_m = wheel_part
        .wheel_radius_mm
        .ok_or("wheel part lacks wheel_radius_mm")?
        / 1000.0;

    // Rim conversion: torque/radius, rpm -> rim m/s. Datasheet-provisional
    // until the Station-2 dyno fits real curves.
    let stall_force = motor_props.stall_torque_mnm / 1000.0 / radius_m;
    let no_load_speed = motor_props.no_load_rpm / 60.0 * std::f64::consts::TAU * radius_m;
    notes.push(format!(
        "motor rim conversion: {:.0} mN·m / {:.0} rpm at r={:.0}mm -> stall {stall_force:.2} N, no-load {no_load_speed:.2} m/s (provisional: {})",
        motor_props.stall_torque_mnm,
        motor_props.no_load_rpm,
        radius_m * 1000.0,
        motor_part.provisional
    ));

    // Battery from the electrical source declaration.
    let batt = robot
        .components
        .iter()
        .find_map(|c| {
            let part = &cat.get(&c.part).ok()?.0;
            part.elec.as_ref()?.source.as_ref().map(|s| s.clone())
        })
        .ok_or("no battery source found among components")?;
    let r_internal = batt.r_internal_ohm.unwrap_or(0.18);

    // Wheel load fraction from support geometry: lever balance between the
    // wheel axle line and the skid line about the CoG.
    let x_wheels = wheels.iter().map(|w| w.pos.x).sum::<f64>() / wheels.len() as f64;
    let x_skids = robot
        .skids
        .iter()
        .map(|s| s[0] / 1000.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let cog_x = derived.cog_mm[0] / 1000.0;
    let wheel_load_fraction = if robot.skids.is_empty() {
        1.0
    } else {
        ((x_skids - cog_x) / (x_skids - x_wheels)).clamp(0.2, 1.0)
    };
    notes.push(format!(
        "wheel load fraction {wheel_load_fraction:.3} (wheels x={x_wheels:.3}m, skids x={x_skids:.3}m, CoG x={cog_x:.3}m)"
    ));
    notes.push(format!(
        "mass {:.1} g, yaw inertia {:.0} g·mm² from derived record {}",
        derived.mass_total_g, derived.yaw_inertia_gmm2, derived.pipeline_version
    ));

    let spec = RigidBotSpec {
        name: format!("{}-{}", robot.identity.name, robot.identity.revision),
        mass_kg: derived.mass_total_g / 1000.0,
        yaw_inertia: derived.yaw_inertia_gmm2 * 1e-9, // g·mm² -> kg·m²
        footprint_half_w: derived.footprint_mm[1] / 2000.0,
        footprint_half_l: derived.footprint_mm[0] / 2000.0,
        wheels,
        motor: MotorCurve {
            stall_force,
            no_load_speed,
            stall_current: motor_props.stall_current_a,
        },
        battery: BatterySpec { v_nominal: batt.volts, r_internal },
        mu_min: tyre.mu_min,
        mu_max: tyre.mu_max,
        mu_kinetic_ratio: tyre.mu_kinetic_ratio,
        c_rr: 0.015, // provisional constant until sled campaigns
        wheel_load_fraction,
    };

    Ok(BoundRobot {
        spec,
        robot_hash: derived.robot_hash.clone(),
        body_hash: derived.body_hash.clone(),
        kernel_ref: robot.sources.kernel.family_hash.clone(),
        derived,
        notes,
    })
}

/// Convenience: bind straight from files (CLI path).
pub fn bind_robot_from_files(robot_path: &Path, parts_dir: &Path) -> Result<BoundRobot, String> {
    let robot: RobotSpec = serde_json::from_slice(
        &std::fs::read(robot_path).map_err(|e| format!("{robot_path:?}: {e}"))?,
    )
    .map_err(|e| format!("parse {robot_path:?}: {e}"))?;
    let cat = Catalogue::load(parts_dir)?;
    bind_robot(&robot, &cat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn mvp_wedge_binds_with_derived_quantities() {
        let root = repo_root();
        let bound = bind_robot_from_files(
            &root.join("robots/mvp-wedge.json"),
            &root.join("parts"),
        )
        .unwrap();
        let s = &bound.spec;
        assert!((s.mass_kg - 0.1009).abs() < 0.005, "mass {:.4}", s.mass_kg);
        assert_eq!(s.wheels.len(), 2);
        // 2WD + front skids: wheels must NOT carry full weight.
        assert!(
            s.wheel_load_fraction > 0.5 && s.wheel_load_fraction < 0.9,
            "load fraction {}",
            s.wheel_load_fraction
        );
        assert!(s.motor.stall_force > 0.5, "rim stall {}", s.motor.stall_force);
        assert!(s.motor.no_load_speed > 1.0 && s.motor.no_load_speed < 2.5);
        assert!(!bound.robot_hash.is_empty() && bound.robot_hash != bound.body_hash);
    }
}
