//! arena-cells — cell80 executor embedding. M0 scope: a NATIVE PLACEHOLDER
//! edge-failsafe kernel (the fast-mode precursor). Per SPEC §7, results
//! produced through this path are provisional until the differential job
//! against the real executor stands (M2); the placeholder exists so the M0
//! ablation runs against the same call shape the executor will use.

use arena_core::{ArenaGeom, CONTROL_DT, GRAVITY};
use arena_plant::dynamic::{RigidBotSpec, RigidState};
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

    /// The certified worst-case stopping distance this kernel claims for a
    /// given speed — the number the envelope bench (§4.2) holds it to.
    pub fn certified_stop_distance(&self, speed: f64) -> f64 {
        let v1 = (speed.abs() + self.a_accel * CONTROL_DT).min(self.v_max);
        v1 * v1 / (2.0 * self.a_brake) + v1 * CONTROL_DT
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

/// Active-braking envelope cell (M1, native placeholder). Where the M0 cell
/// coasts (throttle 0 — on the dynamic plant that's back-EMF braking, which
/// fades linearly with speed), this one commands *aligned* braking: the
/// longitudinal motor force targets the velocity's longitudinal share of the
/// certified friction budget, so that under friction-circle saturation the
/// total ground force stays anti-parallel to the velocity — the optimal
/// direction for a force of fixed magnitude. Over-braking longitudinally
/// while sliding at an angle would rotate the force off the velocity axis
/// and drop the along-track deceleration below cert (the §4.2 anisotropic
/// case).
///
/// Certified braking authority is the worst case over the whole μ band and
/// battery sag: min(kinetic-friction limit at μ_min, motor braking at
/// worst-sag voltage). The envelope bench (§4.2) holds `certified_stop_
/// distance` against the measured plant — any negative margin is a filed
/// finding against THIS cell version.
pub const ACTIVE_BRAKE_CELL_VERSION: &str = "0.1.0-m1-native-placeholder";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActiveBrakeCell {
    /// Certified worst-case braking deceleration, m/s².
    pub a_cert: f64,
    /// Certified worst-case yaw spin-down rate, rad/s² — bounds the window
    /// during which wheel scrub can contaminate linear braking.
    pub alpha_cert: f64,
    /// Per-wheel longitudinal force target at full certified braking, N.
    f_long_cert_pw: f64,
    /// Motor curve mirror (for inverting force -> throttle).
    stall_force: f64,
    no_load_speed: f64,
    volt_ratio_worst: f64,
    /// Low-speed fallback servo gain.
    servo_gain: f64,
}

impl ActiveBrakeCell {
    pub fn new(spec: &RigidBotSpec) -> Self {
        let n_wheels = spec.wheels.len() as f64;
        // Friction budget scales with the weight actually on the driven
        // wheels — a 2WD bot riding on skids cannot brake at full μg.
        let a_friction =
            spec.mu_min * spec.mu_kinetic_ratio * GRAVITY * spec.wheel_load_fraction;
        let a_motor =
            n_wheels * spec.motor.stall_force * spec.worst_voltage_ratio()
                / spec.mass_kg;
        let a_cert = a_friction.min(a_motor);
        // Worst-case spin-down: kinetic friction at μ_min acting at the
        // innermost wheel radius, derated 2x for partial slip alignment.
        let r_min = spec
            .wheels
            .iter()
            .map(|w| w.pos.norm())
            .fold(f64::INFINITY, f64::min);
        let alpha_cert = 0.5
            * spec.mu_min
            * spec.mu_kinetic_ratio
            * spec.mass_kg
            * GRAVITY
            * r_min
            / spec.yaw_inertia;
        ActiveBrakeCell {
            a_cert,
            alpha_cert,
            f_long_cert_pw: a_cert * spec.mass_kg / n_wheels,
            stall_force: spec.motor.stall_force,
            no_load_speed: spec.motor.no_load_speed,
            volt_ratio_worst: spec.worst_voltage_ratio(),
            servo_gain: 8.0,
        }
    }

    /// Certified worst-case stopping distance from `speed` with initial yaw
    /// rate `yaw_rate`, including one control period of reaction allowance.
    ///
    /// The yaw term is deliberately crude and conservative: while spinning,
    /// wheel friction may be consumed by rotational scrub, so we certify
    /// ZERO linear deceleration until the certified spin-down rate has
    /// killed the rotation, then constant-decel braking from unchanged
    /// speed. The envelope bench measures how much conservatism this costs.
    pub fn certified_stop_distance(&self, speed: f64, yaw_rate: f64) -> f64 {
        // 5% envelope factor on the braking term: without it the worst grid
        // point (30° slip, μ_min) clears by tens of microns — real margin,
        // not luck, is what gets certified.
        const ENVELOPE_FACTOR: f64 = 1.05;
        let v = speed.abs();
        let t_spin = yaw_rate.abs() / self.alpha_cert;
        v * t_spin + ENVELOPE_FACTOR * v * v / (2.0 * self.a_cert) + v * CONTROL_DT
    }

    /// Certified worst-case acceleration the driver could add in one control
    /// period (traction-capped) — used by edge triggers to inflate speed.
    pub fn a_accel_worst(spec: &RigidBotSpec) -> f64 {
        let motor = spec.wheels.len() as f64 * spec.motor.stall_force / spec.mass_kg;
        motor.min(spec.mu_max * GRAVITY * spec.wheel_load_fraction)
    }

    /// Brake command from the body-frame longitudinal speed and total speed.
    pub fn brake_cmd(&self, v_long: f64, speed: f64) -> DriveCmd {
        let throttle = if speed < 0.02 {
            // Near rest: plain speed servo, no alignment needed.
            (-self.servo_gain * v_long).clamp(-1.0, 1.0)
        } else {
            // Target per-wheel longitudinal force = -(v_long/|v|) * certified
            // budget; invert the linear motor curve at worst-case voltage.
            let f_target = -self.f_long_cert_pw * (v_long / speed);
            ((f_target / self.stall_force + v_long / self.no_load_speed)
                / self.volt_ratio_worst)
                .clamp(-1.0, 1.0)
        };
        DriveCmd { throttle, turn: 0.0 }
    }
}

/// Edge failsafe for the dynamic plant (§4.5): the M0 kernel's trigger
/// logic wrapped around the M1 aligned-brake cell. Vetoes any command once
/// the certified stopping distance (speed inflated by one control period of
/// worst-case acceleration, yaw-rate allowance included) no longer fits in
/// the space between the footprint and the edge.
pub const DYN_EDGE_FAILSAFE_VERSION: &str = "0.1.0-m1-native-placeholder";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DynEdgeFailsafe {
    pub cell: ActiveBrakeCell,
    pub enabled: bool,
    reach: f64,
    a_accel: f64,
    escape_turn_cap: f64,
}

impl DynEdgeFailsafe {
    pub fn new(spec: &RigidBotSpec, margin_m: f64, enabled: bool) -> Self {
        // Escape-pivot authority derived from the friction budget: the
        // pivot's per-wheel longitudinal demand must use well under half of
        // the kinetic circle at μ_min, or turning consumes ALL grip
        // (longitudinal priority) and lateral velocity stops decaying — the
        // bot then scrub-drifts sideways off the edge while fully vetoed
        // (measured failure mode).
        let n = spec.wheels.len() as f64;
        let cap_k_pw = spec.mu_min * spec.mu_kinetic_ratio
            * (spec.mass_kg * GRAVITY * spec.wheel_load_fraction / n);
        let escape_turn_cap = (0.35 * cap_k_pw
            / (spec.motor.stall_force * spec.worst_voltage_ratio()))
        .clamp(0.02, 0.3);
        DynEdgeFailsafe {
            cell: ActiveBrakeCell::new(spec),
            enabled,
            reach: spec.footprint_circumradius() + margin_m,
            a_accel: ActiveBrakeCell::a_accel_worst(spec),
            escape_turn_cap,
        }
    }

    /// Filter one control-tick command. Returns (command, intervened).
    pub fn filter(
        &self,
        geom: &ArenaGeom,
        st: &RigidState,
        cmd: DriveCmd,
    ) -> (DriveCmd, bool) {
        if !self.enabled {
            return (cmd, false);
        }
        let speed = st.speed();
        let v1 = speed + self.a_accel * CONTROL_DT;
        let d_stop = self.cell.certified_stop_distance(v1, st.omega);
        let d_avail = geom.dist_to_edge(st.pos) - self.reach;
        if d_avail <= d_stop {
            let fwd_x = st.heading.cos();
            let fwd_y = st.heading.sin();
            let v_long = st.vel.x * fwd_x + st.vel.y * fwd_y;
            let mut brake = self.cell.brake_cmd(v_long, speed);
            // Once braked to near-rest, allow a SLOW pivot so the driver can
            // point away and leave — without it the bot parks at the
            // boundary forever. The pivot must not break the envelope:
            // unrestricted turn spins the bot to tens of rad/s and skid-
            // steer scrub-walk translates it off the edge (measured: 39/40
            // losses). So the pass-through is capped and yaw-rate gated —
            // a few rad/s of rotation moves the CoG negligibly while the
            // longitudinal servo keeps holding v ≈ 0.
            const ESCAPE_SPEED_GATE: f64 = 0.05;
            const ESCAPE_OMEGA_CAP: f64 = 3.0;
            if speed < ESCAPE_SPEED_GATE && st.omega.abs() < ESCAPE_OMEGA_CAP {
                brake.turn = cmd.turn.clamp(-self.escape_turn_cap, self.escape_turn_cap);
            }
            (brake, true)
        } else {
            (cmd, false)
        }
    }
}
