//! D/X design checks (codes registered in specs/codes.md). Each check is an
//! individually callable function; `run_checks` is the standard composition.
//! New codes: register in codes.md first, then add here (bugs-become-rules).

use crate::catalogue::Catalogue;
use crate::schema::{MechSource, RobotSpec};
use crate::{CUBE_MM, WEIGHT_LIMIT_G};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckResult {
    pub code: String,
    pub description: String,
    pub pass: bool,
    pub detail: String,
}

pub fn d01_cube_fit(bbox_mm: [f64; 3]) -> CheckResult {
    CheckResult {
        code: "D01".into(),
        description: "cube fit (bbox vs 101.6mm)".into(),
        pass: bbox_mm.iter().all(|&d| d <= CUBE_MM),
        detail: format!(
            "bbox {:.1} x {:.1} x {:.1} mm",
            bbox_mm[0], bbox_mm[1], bbox_mm[2]
        ),
    }
}

pub fn d02_mass_limit(mass_total_g: f64) -> CheckResult {
    CheckResult {
        code: "D02".into(),
        description: "mass within class limit".into(),
        pass: mass_total_g <= WEIGHT_LIMIT_G,
        detail: format!("{mass_total_g:.1} g of {WEIGHT_LIMIT_G} g"),
    }
}

pub fn d05_wheel_containment(
    spec: &RobotSpec,
    cat: &Catalogue,
) -> Result<CheckResult, String> {
    let MechSource::Parametric { chassis } = &spec.sources.mech;
    let mut pass = true;
    let mut detail = String::from("all wheels within footprint");
    for w in &spec.drive.wheels {
        let (part, _) = cat.get(&w.part)?;
        let half_w = part.wheel_width_mm.unwrap_or(0.0) / 2.0;
        if w.pos_mm[1].abs() + half_w > chassis.width_mm / 2.0 {
            pass = false;
            detail = format!(
                "wheel at y={:.1} (+{half_w:.1} half-width) exceeds half-width {:.1}",
                w.pos_mm[1],
                chassis.width_mm / 2.0
            );
        }
    }
    Ok(CheckResult {
        code: "D05".into(),
        description: "wheel-chassis containment".into(),
        pass,
        detail,
    })
}

pub fn x03_derivation_rule() -> CheckResult {
    // The schema has no fields for hand-entered mass/CoG/inertia, so the
    // rule holds by construction in parametric mode; the check exists so the
    // report says so explicitly (and so CAD mode has a place to enforce it).
    CheckResult {
        code: "X03".into(),
        description: "derivation rule: no hand-entered derivables".into(),
        pass: true,
        detail: "all mass/CoG/inertia/geometry quantities computed by this pipeline".into(),
    }
}

pub fn run_checks(
    spec: &RobotSpec,
    cat: &Catalogue,
    mass_total_g: f64,
    bbox_mm: [f64; 3],
) -> Result<Vec<CheckResult>, String> {
    Ok(vec![
        d01_cube_fit(bbox_mm),
        d02_mass_limit(mass_total_g),
        d05_wheel_containment(spec, cat)?,
        x03_derivation_rule(),
    ])
}
