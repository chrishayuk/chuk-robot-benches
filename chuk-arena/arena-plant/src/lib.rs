//! arena-plant — bot plant models. M0 scope: kinematic differential-drive plant
//! (SPEC §10 M0 "kinematic plant"); full dynamic plant with per-wheel friction
//! cones arrives at M1.

use arena_core::{Vec2, GRAVITY};
use serde::{Deserialize, Serialize};

pub mod dynamic;

/// Version of the M0 kinematic path — unchanged so the banked M0 corpus
/// stays reproducible; the dynamic model carries DYNAMIC_PLANT_VERSION.
pub const ARENA_PLANT_VERSION: &str = "0.1.0-m0-kinematic";

/// The design vector (SPEC §3), M0 subset. Everything sweepable lives here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BotSpec {
    pub name: String,
    pub mass_kg: f64,
    pub footprint_half_w: f64,
    pub footprint_half_l: f64,
    /// Motor-limited top speed, m/s.
    pub v_max: f64,
    /// Motor-limited linear acceleration, m/s^2 (traction may limit further).
    pub a_motor: f64,
    /// Yaw rate limit, rad/s.
    pub omega_max: f64,
    /// Yaw acceleration limit, rad/s^2.
    pub alpha_max: f64,
    /// Tyre-floor friction band carried as a range, never a point (SPEC §3).
    pub mu_min: f64,
    pub mu_max: f64,
}

impl BotSpec {
    /// Datasheet-provisional M0 baseline (flagged as such per SPEC §3): an
    /// antweight-class pusher in a 0.9 m edge-out arena.
    pub fn default_m0() -> Self {
        BotSpec {
            name: "m0-baseline-pusher".to_string(),
            mass_kg: 0.150,
            footprint_half_w: 0.05,
            footprint_half_l: 0.05,
            v_max: 1.5,
            a_motor: 4.0,
            omega_max: 8.0,
            alpha_max: 40.0,
            mu_min: 0.4,
            mu_max: 0.7,
        }
    }

    /// Circumradius of the footprint — conservative reach used by edge logic.
    pub fn footprint_circumradius(&self) -> f64 {
        (self.footprint_half_w * self.footprint_half_w
            + self.footprint_half_l * self.footprint_half_l)
            .sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DriveCmd {
    /// Forward/reverse, [-1, 1].
    pub throttle: f64,
    /// Yaw, [-1, 1].
    pub turn: f64,
}

impl DriveCmd {
    pub const ZERO: DriveCmd = DriveCmd {
        throttle: 0.0,
        turn: 0.0,
    };
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlantState {
    pub pos: Vec2,
    pub heading: f64,
    /// Longitudinal speed along heading (signed), m/s. The kinematic plant is
    /// nonholonomic: no lateral slip until the dynamic plant lands at M1.
    pub v: f64,
    pub omega: f64,
}

impl PlantState {
    pub fn at_rest_at(pos: Vec2, heading: f64) -> Self {
        PlantState {
            pos,
            heading,
            v: 0.0,
            omega: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KinematicPlant {
    pub spec: BotSpec,
    pub state: PlantState,
}

impl KinematicPlant {
    pub fn new(spec: BotSpec, state: PlantState) -> Self {
        KinematicPlant { spec, state }
    }

    /// One world tick. Acceleration is limited by the lesser of motor torque
    /// and the traction circle at the episode's sampled mu.
    pub fn step_world(&mut self, cmd: DriveCmd, mu: f64, dt: f64) {
        let a_lim = self.spec.a_motor.min(mu * GRAVITY);
        let v_target = cmd.throttle.clamp(-1.0, 1.0) * self.spec.v_max;
        let dv = (v_target - self.state.v).clamp(-a_lim * dt, a_lim * dt);
        self.state.v += dv;

        let om_target = cmd.turn.clamp(-1.0, 1.0) * self.spec.omega_max;
        let dom = (om_target - self.state.omega)
            .clamp(-self.spec.alpha_max * dt, self.spec.alpha_max * dt);
        self.state.omega += dom;

        self.state.heading += self.state.omega * dt;
        self.state.pos.x += self.state.v * self.state.heading.cos() * dt;
        self.state.pos.y += self.state.v * self.state.heading.sin() * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_core::{WORLD_DT, WORLD_HZ};

    #[test]
    fn braking_distance_respects_traction_limit() {
        let spec = BotSpec::default_m0();
        let mu = spec.mu_min;
        let mut plant = KinematicPlant::new(
            spec.clone(),
            PlantState {
                pos: Vec2::ZERO,
                heading: 0.0,
                v: spec.v_max,
                omega: 0.0,
            },
        );
        let a = spec.a_motor.min(mu * GRAVITY);
        let analytic = spec.v_max * spec.v_max / (2.0 * a);
        for _ in 0..(3 * WORLD_HZ) {
            plant.step_world(DriveCmd::ZERO, mu, WORLD_DT);
            if plant.state.v == 0.0 {
                break;
            }
        }
        assert!(plant.state.v.abs() < 1e-9);
        // Discrete stopping distance must not exceed analytic + one tick of travel.
        assert!(plant.state.pos.x <= analytic + spec.v_max * WORLD_DT);
        assert!(plant.state.pos.x >= analytic - spec.v_max * WORLD_DT);
    }
}
