//! Dynamic plant (M1, SPEC §3): planar rigid body with per-wheel friction
//! circles, linear DC motor curves with back-EMF braking, battery sag, and
//! rolling resistance. All sub-model parameters are datasheet-provisional
//! until Station-2 campaigns replace them (SPEC §9).
//!
//! The M0 kinematic plant is untouched: the banked M0 corpus must stay
//! bit-reproducible, so the dynamic model is additive and carries its own
//! version tag.

use crate::DriveCmd;
use arena_core::{Vec2, GRAVITY};
use serde::{Deserialize, Serialize};

pub const DYNAMIC_PLANT_VERSION: &str = "0.1.0-m1-dynamic";

/// Linear DC motor at the wheel rim: force falls off linearly with rim speed
/// (back-EMF), scales with terminal voltage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotorCurve {
    /// Rim force at stall and nominal voltage, N.
    pub stall_force: f64,
    /// No-load rim speed at nominal voltage, m/s.
    pub no_load_speed: f64,
    /// Stall current at nominal voltage, A (per motor).
    pub stall_current: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatterySpec {
    pub v_nominal: f64,
    /// Internal resistance, ohm (pack + leads + ESC, lumped).
    pub r_internal: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WheelSide {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WheelSpec {
    /// Contact point in the body frame (+x forward, +y left), m.
    pub pos: Vec2,
    pub side: WheelSide,
}

/// The M1 design vector: everything the design search sweeps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RigidBotSpec {
    pub name: String,
    pub mass_kg: f64,
    /// Yaw inertia about the CoG, kg·m².
    pub yaw_inertia: f64,
    pub footprint_half_w: f64,
    pub footprint_half_l: f64,
    pub wheels: Vec<WheelSpec>,
    /// One curve for all drive motors (per-wheel curves are a later refinement).
    pub motor: MotorCurve,
    pub battery: BatterySpec,
    /// Tyre-floor static friction band (SPEC §3: a range, never a point).
    pub mu_min: f64,
    pub mu_max: f64,
    /// μ_kinetic = mu_kinetic_ratio · μ_static once a wheel saturates.
    pub mu_kinetic_ratio: f64,
    /// Rolling resistance coefficient (force = c_rr · N per wheel).
    pub c_rr: f64,
}

impl RigidBotSpec {
    /// Datasheet-provisional M1 baseline: the same antweight pusher as
    /// BotSpec::default_m0, now with real drive.
    pub fn default_m1() -> Self {
        let m = 0.150;
        let (hw, hl) = (0.05, 0.05);
        RigidBotSpec {
            name: "m1-baseline-pusher".to_string(),
            mass_kg: m,
            // Uniform box about the CoG: I = m (w² + l²) / 3 with half-dims.
            yaw_inertia: m * (hw * hw + hl * hl) / 3.0,
            footprint_half_w: hw,
            footprint_half_l: hl,
            wheels: vec![
                WheelSpec { pos: Vec2::new(0.03, 0.04), side: WheelSide::Left },
                WheelSpec { pos: Vec2::new(-0.03, 0.04), side: WheelSide::Left },
                WheelSpec { pos: Vec2::new(0.03, -0.04), side: WheelSide::Right },
                WheelSpec { pos: Vec2::new(-0.03, -0.04), side: WheelSide::Right },
            ],
            motor: MotorCurve {
                stall_force: 0.225,
                no_load_speed: 2.0,
                stall_current: 1.8,
            },
            battery: BatterySpec { v_nominal: 7.4, r_internal: 0.18 },
            mu_min: 0.4,
            mu_max: 0.7,
            mu_kinetic_ratio: 0.85,
            c_rr: 0.015,
        }
    }

    pub fn footprint_circumradius(&self) -> f64 {
        (self.footprint_half_w * self.footprint_half_w
            + self.footprint_half_l * self.footprint_half_l)
            .sqrt()
    }

    /// Worst-case voltage ratio under full-fleet stall draw — used for
    /// certified (lower-bound) braking authority.
    pub fn worst_voltage_ratio(&self) -> f64 {
        let i_max = self.wheels.len() as f64 * self.motor.stall_current;
        ((self.battery.v_nominal - i_max * self.battery.r_internal)
            / self.battery.v_nominal)
            .max(0.5)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RigidState {
    pub pos: Vec2,
    pub heading: f64,
    /// World-frame velocity, m/s (lateral slip is real here).
    pub vel: Vec2,
    pub omega: f64,
    /// Total motor current drawn last tick, A (feeds next tick's sag).
    pub current_a: f64,
}

impl RigidState {
    pub fn at_rest_at(pos: Vec2, heading: f64) -> Self {
        RigidState { pos, heading, vel: Vec2::ZERO, omega: 0.0, current_a: 0.0 }
    }

    pub fn speed(&self) -> f64 {
        self.vel.norm()
    }
}

/// Per-tick force resolution, exposed for the dyno bench (§4.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved {
    pub force: Vec2,
    pub torque: f64,
    pub current_a: f64,
    /// True if any wheel saturated its friction circle this tick.
    pub any_sliding: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DynamicPlant {
    pub spec: RigidBotSpec,
    pub state: RigidState,
}

impl DynamicPlant {
    pub fn new(spec: RigidBotSpec, state: RigidState) -> Self {
        DynamicPlant { spec, state }
    }

    /// Resolve ground forces for the current state under `cmd` at floor `mu`,
    /// without integrating. `dt` sets the lateral-slip relaxation demand.
    pub fn resolve(&self, cmd: DriveCmd, mu: f64, dt: f64) -> Resolved {
        let s = &self.spec;
        let st = &self.state;
        let throttle = cmd.throttle.clamp(-1.0, 1.0);
        let turn = cmd.turn.clamp(-1.0, 1.0);
        let u_left = (throttle - turn).clamp(-1.0, 1.0);
        let u_right = (throttle + turn).clamp(-1.0, 1.0);

        let v_term = (s.battery.v_nominal - st.current_a * s.battery.r_internal)
            .max(0.5 * s.battery.v_nominal);
        let volt_ratio = v_term / s.battery.v_nominal;

        let (cos_h, sin_h) = (st.heading.cos(), st.heading.sin());
        let fwd = Vec2::new(cos_h, sin_h);
        let left = Vec2::new(-sin_h, cos_h);
        let n_load = s.mass_kg * GRAVITY / s.wheels.len() as f64;
        let m_share = s.mass_kg / s.wheels.len() as f64;

        let mut force = Vec2::ZERO;
        let mut torque = 0.0;
        let mut current = 0.0;
        let mut any_sliding = false;

        for w in &s.wheels {
            let r_world = Vec2::new(
                w.pos.x * cos_h - w.pos.y * sin_h,
                w.pos.x * sin_h + w.pos.y * cos_h,
            );
            // Contact-point velocity: v + ω ẑ × r.
            let v_c = Vec2::new(
                st.vel.x - st.omega * r_world.y,
                st.vel.y + st.omega * r_world.x,
            );
            let v_long = v_c.dot(fwd);
            let v_lat = v_c.dot(left);

            let u = match w.side {
                WheelSide::Left => u_left,
                WheelSide::Right => u_right,
            };
            let f_cap_motor = s.motor.stall_force * volt_ratio;
            let mut f_long = (s.motor.stall_force
                * (volt_ratio * u - v_long / s.motor.no_load_speed))
                .clamp(-f_cap_motor, f_cap_motor);
            current += f_long.abs() / s.motor.stall_force * s.motor.stall_current;
            // Rolling resistance, smoothed through zero to avoid chatter.
            f_long -= s.c_rr * n_load * (v_long / (v_long.abs() + 0.01));

            // Lateral demand: kill slip over ~8 world ticks (1 ms).
            let f_lat = -m_share * v_lat / (8.0 * dt);

            // Friction circle with longitudinal priority: a driven contact
            // patch holds longitudinal grip preferentially (skid-steer must
            // scrub its front/rear wheels sideways to yaw at all). Once
            // either axis exceeds the static budget the whole patch drops to
            // the kinetic circle.
            let cap_static = mu * n_load;
            let lat_budget_s = (cap_static * cap_static - f_long.clamp(-cap_static, cap_static).powi(2))
                .max(0.0)
                .sqrt();
            let sliding = f_long.abs() > cap_static || f_lat.abs() > lat_budget_s;
            let (f_long, f_lat) = if sliding {
                any_sliding = true;
                let cap_k = mu * s.mu_kinetic_ratio * n_load;
                let f_long_k = f_long.clamp(-cap_k, cap_k);
                let lat_budget_k =
                    (cap_k * cap_k - f_long_k * f_long_k).max(0.0).sqrt();
                (f_long_k, f_lat.clamp(-lat_budget_k, lat_budget_k))
            } else {
                (f_long, f_lat)
            };

            let f_w = fwd * f_long + left * f_lat;
            force = force + f_w;
            torque += r_world.x * f_w.y - r_world.y * f_w.x;
        }

        Resolved { force, torque, current_a: current, any_sliding }
    }

    /// One world tick, semi-implicit Euler.
    pub fn step_world(&mut self, cmd: DriveCmd, mu: f64, dt: f64) {
        let r = self.resolve(cmd, mu, dt);
        let st = &mut self.state;
        st.vel = st.vel + r.force * (dt / self.spec.mass_kg);
        st.omega += r.torque / self.spec.yaw_inertia * dt;
        st.pos = st.pos + st.vel * dt;
        st.heading += st.omega * dt;
        st.current_a = r.current_a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_core::WORLD_DT;

    fn plant_with_vel(vel: Vec2) -> DynamicPlant {
        let spec = RigidBotSpec::default_m1();
        let mut st = RigidState::at_rest_at(Vec2::ZERO, 0.0);
        st.vel = vel;
        DynamicPlant::new(spec, st)
    }

    #[test]
    fn pure_lateral_slide_decays_at_kinetic_mu() {
        // Heading +x, sliding +y: all friction budget opposes the slide, so
        // speed must decay at ~μ_k·g.
        let mu = 0.5;
        let mut p = plant_with_vel(Vec2::new(0.0, 1.0));
        let a_expect = mu * p.spec.mu_kinetic_ratio * GRAVITY;
        let t_sim = 0.1;
        let n = (t_sim / WORLD_DT) as usize;
        for _ in 0..n {
            p.step_world(DriveCmd::ZERO, mu, WORLD_DT);
        }
        let v_expect = 1.0 - a_expect * t_sim;
        assert!(
            (p.state.speed() - v_expect).abs() < 0.02,
            "speed {} vs expected {}",
            p.state.speed(),
            v_expect
        );
    }

    #[test]
    fn acceleration_from_rest_is_traction_limited_on_low_mu() {
        let mu = 0.4;
        let mut p = plant_with_vel(Vec2::ZERO);
        let a_cap = mu * GRAVITY; // static cap; kinetic once spinning
        for _ in 0..80 {
            // 10 ms
            p.step_world(DriveCmd { throttle: 1.0, turn: 0.0 }, mu, WORLD_DT);
        }
        let a_meas = p.state.vel.x / 0.01;
        assert!(a_meas > 0.5 * a_cap, "too slow: {a_meas}");
        assert!(a_meas <= a_cap * 1.01, "exceeds traction: {a_meas}");
        assert!(p.state.vel.y.abs() < 1e-9, "no lateral drift when straight");
    }

    #[test]
    fn top_speed_below_no_load_and_battery_sags() {
        let mu = 0.7;
        let mut p = plant_with_vel(Vec2::ZERO);
        let mut min_ratio: f64 = 1.0;
        for _ in 0..(4.0 / WORLD_DT) as usize {
            p.step_world(DriveCmd { throttle: 1.0, turn: 0.0 }, mu, WORLD_DT);
            let vr = (p.spec.battery.v_nominal
                - p.state.current_a * p.spec.battery.r_internal)
                / p.spec.battery.v_nominal;
            min_ratio = min_ratio.min(vr);
        }
        assert!(p.state.vel.x > 1.0, "top speed too low: {}", p.state.vel.x);
        assert!(
            p.state.vel.x < p.spec.motor.no_load_speed,
            "exceeds no-load speed"
        );
        assert!(min_ratio < 1.0, "battery never sagged");
    }

    #[test]
    fn turning_in_place_yaws_ccw_for_positive_turn() {
        let mu = 0.6;
        let mut p = plant_with_vel(Vec2::ZERO);
        for _ in 0..(0.5 / WORLD_DT) as usize {
            p.step_world(DriveCmd { throttle: 0.0, turn: 1.0 }, mu, WORLD_DT);
        }
        assert!(p.state.heading > 0.5, "did not yaw CCW: {}", p.state.heading);
        assert!(p.state.pos.norm() < 0.02, "walked while turning in place");
    }
}
