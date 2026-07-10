//! arena-cells — cell80 executor embedding. M0 scope: a NATIVE PLACEHOLDER
//! edge-failsafe kernel (the fast-mode precursor). Per SPEC §7, results
//! produced through this path are provisional until the differential job
//! against the real executor stands (M2); the placeholder exists so the M0
//! ablation runs against the same call shape the executor will use.

use arena_core::{ArenaGeom, CONTROL_DT, GRAVITY};
use arena_plant::{BotSpec, DriveCmd, PlantState};
use serde::{Deserialize, Serialize};

pub const ARENA_CELLS_VERSION: &str = "0.1.0-m0-native-placeholder";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeFailsafeParams {
    pub enabled: bool,
    /// Extra physical margin beyond the footprint circumradius, m.
    pub margin_m: f64,
}

impl EdgeFailsafeParams {
    pub fn enabled_default() -> Self {
        EdgeFailsafeParams {
            enabled: true,
            margin_m: 0.02,
        }
    }

    pub fn disabled() -> Self {
        EdgeFailsafeParams {
            enabled: false,
            margin_m: 0.02,
        }
    }
}

/// Certified-envelope edge failsafe (native placeholder).
///
/// Invariant maintained: Chebyshev distance-to-edge minus reach is always
/// >= the worst-case stopping distance at the certified minimum-mu braking
/// deceleration. The check inflates current speed by one control period of
/// worst-case motor acceleration, so a command admitted this tick cannot
/// create an unbrakeable state before the next check runs. Direction-agnostic
/// (nearest edge), hence conservative; the M1 envelope bench measures exactly
/// how conservative (SPEC §4.2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeFailsafeCell {
    pub params: EdgeFailsafeParams,
    a_brake: f64,
    a_accel: f64,
    v_max: f64,
    reach: f64,
}

impl EdgeFailsafeCell {
    pub fn new(spec: &BotSpec, params: EdgeFailsafeParams) -> Self {
        EdgeFailsafeCell {
            a_brake: spec.a_motor.min(spec.mu_min * GRAVITY),
            a_accel: spec.a_motor,
            v_max: spec.v_max,
            reach: spec.footprint_circumradius() + params.margin_m,
            params,
        }
    }

    /// Filter one control-tick command. Returns (command, intervened).
    pub fn filter(
        &self,
        geom: &ArenaGeom,
        state: &PlantState,
        cmd: DriveCmd,
    ) -> (DriveCmd, bool) {
        if !self.params.enabled {
            return (cmd, false);
        }
        let v1 = (state.v.abs() + self.a_accel * CONTROL_DT).min(self.v_max);
        let d_stop = v1 * v1 / (2.0 * self.a_brake) + v1 * CONTROL_DT;
        let d_avail = geom.dist_to_edge(state.pos) - self.reach;
        if d_avail <= d_stop {
            (DriveCmd::ZERO, true)
        } else {
            (cmd, false)
        }
    }
}
