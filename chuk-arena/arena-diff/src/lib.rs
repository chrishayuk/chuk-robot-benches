//! arena-diff — the Rapier differential adversary (SPEC §2.2/§2.3).
//!
//! Matched scenarios run in the owned integrator and in rapier2d-f64 from
//! identical initial conditions; divergence beyond the §2.3 tolerances is a
//! filed finding, and the M1 kill criterion is evaluated on this table.
//!
//! Scope honesty: rapier2d is a side-view engine with no native top-down
//! ground friction, so for the friction scenarios both sides apply the SAME
//! external Coulomb force law — what C1/C4 actually cross-check is mass
//! handling, force integration, and impulse application (plus the analytic
//! closed forms as a third oracle). The contact scenarios C2/C3/C5, where
//! Rapier's impulse resolution is a genuine independent oracle, need the M2
//! impact layer and are carried as PENDING, not dropped.

use arena_core::{Vec2, GRAVITY, WORLD_DT};
use rapier2d_f64::prelude::*;
use serde::{Deserialize, Serialize};

pub const ARENA_DIFF_VERSION: &str = "0.1.0-m1";
pub const RAPIER_VERSION: &str = "rapier2d-f64 0.22";

/// Below this speed both sides snap to rest — identical law on both sides so
/// the comparison isolates the integrators, not the stopping heuristics.
const V_EPS: f64 = 1e-3;
const PUCK_MASS: f64 = 0.150;

fn coulomb(vel: Vec2, mu: f64, mass: f64) -> Vec2 {
    let n = vel.norm();
    if n > V_EPS {
        vel * (-mu * mass * GRAVITY / n)
    } else {
        Vec2::ZERO
    }
}

// ---------------------------------------------------------------------------
// Owned side: the same semi-implicit Euler scheme as arena-plant, on a bare
// puck (§2.3 scenarios are generic rigid-body cases, not wheeled bots).
// ---------------------------------------------------------------------------

struct OwnedPuck {
    pos: Vec2,
    vel: Vec2,
}

impl OwnedPuck {
    fn new(vel: Vec2) -> Self {
        OwnedPuck { pos: Vec2::ZERO, vel }
    }

    fn step(&mut self, extra_force: Vec2, mu: f64) {
        let f = coulomb(self.vel, mu, PUCK_MASS) + extra_force;
        self.vel = self.vel + f * (WORLD_DT / PUCK_MASS);
        if self.vel.norm() < V_EPS {
            self.vel = Vec2::ZERO;
        }
        self.pos = self.pos + self.vel * WORLD_DT;
    }

    fn impulse(&mut self, j: Vec2) {
        self.vel = self.vel + j * (1.0 / PUCK_MASS);
    }
}

// ---------------------------------------------------------------------------
// Rapier side
// ---------------------------------------------------------------------------

struct RapierPuck {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad: DefaultBroadPhase,
    narrow: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd: CCDSolver,
    params: IntegrationParameters,
    handle: RigidBodyHandle,
}

impl RapierPuck {
    fn new(vel: Vec2) -> Self {
        let mut bodies = RigidBodySet::new();
        let rb = RigidBodyBuilder::dynamic()
            .translation(vector![0.0, 0.0])
            .linvel(vector![vel.x, vel.y])
            .additional_mass(PUCK_MASS)
            .lock_rotations()
            .build();
        let handle = bodies.insert(rb);
        let mut params = IntegrationParameters::default();
        params.dt = WORLD_DT;
        RapierPuck {
            bodies,
            colliders: ColliderSet::new(),
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad: DefaultBroadPhase::new(),
            narrow: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            params,
            handle,
        }
    }

    fn vel(&self) -> Vec2 {
        let v = self.bodies[self.handle].linvel();
        Vec2::new(v.x, v.y)
    }

    fn pos(&self) -> Vec2 {
        let t = self.bodies[self.handle].translation();
        Vec2::new(t.x, t.y)
    }

    fn step(&mut self, extra_force: Vec2, mu: f64) {
        let f = coulomb(self.vel(), mu, PUCK_MASS) + extra_force;
        {
            let body = &mut self.bodies[self.handle];
            body.reset_forces(true);
            body.add_force(vector![f.x, f.y], true);
        }
        self.pipeline.step(
            &vector![0.0, 0.0],
            &self.params,
            &mut self.islands,
            &mut self.broad,
            &mut self.narrow,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd,
            None,
            &(),
            &(),
        );
        let body = &mut self.bodies[self.handle];
        let v = *body.linvel();
        if (v.x * v.x + v.y * v.y).sqrt() < V_EPS {
            body.set_linvel(vector![0.0, 0.0], true);
        }
    }

    fn impulse(&mut self, j: Vec2) {
        self.bodies[self.handle].apply_impulse(vector![j.x, j.y], true);
    }
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub id: String,
    pub description: String,
    pub status: String,
    pub grid_points: u64,
    /// Worst owned-vs-rapier divergence over the grid, in `unit`.
    pub max_divergence: f64,
    /// Worst owned-vs-analytic divergence (third oracle), in `unit`.
    pub max_vs_analytic: f64,
    pub tolerance: f64,
    pub unit: String,
    pub pass: bool,
}

fn pending(id: &str, description: &str, tolerance: f64, unit: &str) -> ScenarioResult {
    ScenarioResult {
        id: id.to_string(),
        description: description.to_string(),
        status: "PENDING — needs contact resolution (M2 impact layer)".to_string(),
        grid_points: 0,
        max_divergence: 0.0,
        max_vs_analytic: 0.0,
        tolerance,
        unit: unit.to_string(),
        pass: true, // not evaluable; excluded from the kill-criterion verdict
    }
}

/// C1: free sliding deceleration — stopping distance, tolerance 1% relative.
fn scenario_c1() -> ScenarioResult {
    let v0_grid = [0.5, 1.0, 1.5, 2.0];
    let mu_grid = [0.40, 0.55, 0.70];
    let mut max_div: f64 = 0.0;
    let mut max_ana: f64 = 0.0;
    let mut n = 0;
    for &v0 in &v0_grid {
        for &mu in &mu_grid {
            let analytic = v0 * v0 / (2.0 * mu * GRAVITY);
            let mut own = OwnedPuck::new(Vec2::new(v0, 0.0));
            let mut rap = RapierPuck::new(Vec2::new(v0, 0.0));
            for _ in 0..(5.0 / WORLD_DT) as u64 {
                let done_o = own.vel.norm() == 0.0;
                let done_r = rap.vel().norm() == 0.0;
                if !done_o {
                    own.step(Vec2::ZERO, mu);
                }
                if !done_r {
                    rap.step(Vec2::ZERO, mu);
                }
                if done_o && done_r {
                    break;
                }
            }
            let d_o = own.pos.x;
            let d_r = rap.pos().x;
            max_div = max_div.max((d_o - d_r).abs() / analytic);
            max_ana = max_ana.max((d_o - analytic).abs() / analytic);
            n += 1;
        }
    }
    let tol = 0.01;
    ScenarioResult {
        id: "C1".to_string(),
        description: "free sliding deceleration, single body: stopping distance".to_string(),
        status: "RUN".to_string(),
        grid_points: n,
        max_divergence: max_div,
        max_vs_analytic: max_ana,
        tolerance: tol,
        unit: "relative".to_string(),
        pass: max_div <= tol,
    }
}

/// C4: lateral shove during forward motion — position divergence 500 ms
/// after the impulse, tolerance 5 mm absolute.
fn scenario_c4() -> ScenarioResult {
    let j_grid = [0.05, 0.10, 0.20]; // N·s lateral
    let mu_grid = [0.40, 0.70];
    let v0 = 1.2;
    let pre_ticks = (0.1 / WORLD_DT) as u64;
    let post_ticks = (0.5 / WORLD_DT) as u64;
    let mut max_div: f64 = 0.0;
    let mut max_ana: f64 = 0.0;
    let mut n = 0;
    for &j in &j_grid {
        for &mu in &mu_grid {
            let mut own = OwnedPuck::new(Vec2::new(v0, 0.0));
            let mut rap = RapierPuck::new(Vec2::new(v0, 0.0));
            for _ in 0..pre_ticks {
                own.step(Vec2::ZERO, mu);
                rap.step(Vec2::ZERO, mu);
            }
            // Analytic third oracle: uniform decel keeps velocity direction
            // constant, so each phase is closed-form.
            let a = mu * GRAVITY;
            let v1 = (v0 - a * 0.1).max(0.0);
            let d1 = (v0 + v1) * 0.5 * 0.1;
            let vy = j / PUCK_MASS;
            let sp = (v1 * v1 + vy * vy).sqrt();
            let t_stop = sp / a;
            let travel = if t_stop >= 0.5 {
                (sp - 0.5 * a * 0.5) * 0.5 // sp*t - a t²/2 at t=0.5
            } else {
                sp * sp / (2.0 * a)
            };
            let analytic = Vec2::new(d1 + travel * v1 / sp, travel * vy / sp);

            own.impulse(Vec2::new(0.0, j));
            rap.impulse(Vec2::new(0.0, j));
            for _ in 0..post_ticks {
                own.step(Vec2::ZERO, mu);
                rap.step(Vec2::ZERO, mu);
            }
            let div = (own.pos - rap.pos()).norm();
            let ana = (own.pos - analytic).norm();
            max_div = max_div.max(div);
            max_ana = max_ana.max(ana);
            n += 1;
        }
    }
    let tol = 0.005;
    ScenarioResult {
        id: "C4".to_string(),
        description: "lateral shove during forward motion: position 500ms after impulse".to_string(),
        status: "RUN".to_string(),
        grid_points: n,
        max_divergence: max_div,
        max_vs_analytic: max_ana,
        tolerance: tol,
        unit: "metres".to_string(),
        pass: max_div <= tol,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffReport {
    pub diff_version: String,
    pub rapier: String,
    pub scenarios: Vec<ScenarioResult>,
    pub kill_criterion: String,
}

pub fn differential_report() -> DiffReport {
    let scenarios = vec![
        scenario_c1(),
        pending(
            "C2",
            "wall impact at varying incidence: post-impact velocities (2%)",
            0.02,
            "relative",
        ),
        pending(
            "C3",
            "two-body head-on push: steady-state contact force (5%)",
            0.05,
            "relative",
        ),
        scenario_c4(),
        pending(
            "C5",
            "glancing spin contact: post-hit velocities / delivered energy (5%/10%)",
            0.05,
            "relative",
        ),
    ];
    let run: Vec<&ScenarioResult> =
        scenarios.iter().filter(|s| s.status == "RUN").collect();
    let all_pass = run.iter().all(|s| s.pass);
    let kill_criterion = format!(
        "{} on evaluable subset ({}); C2/C3/C5 pending M2 contact layer — checkpoint re-runs in full at M2",
        if all_pass { "PASS" } else { "FAIL — embed Rapier per §2.2" },
        run.iter().map(|s| s.id.as_str()).collect::<Vec<_>>().join(","),
    );
    DiffReport {
        diff_version: ARENA_DIFF_VERSION.to_string(),
        rapier: RAPIER_VERSION.to_string(),
        scenarios,
        kill_criterion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c1_within_tolerance() {
        let r = scenario_c1();
        assert!(r.pass, "C1 diverged: {:?}", r);
        assert!(r.max_vs_analytic < 0.01, "C1 off analytic: {:?}", r);
    }

    #[test]
    fn c4_within_tolerance() {
        let r = scenario_c4();
        assert!(r.pass, "C4 diverged: {:?}", r);
        assert!(r.max_vs_analytic < 0.005, "C4 off analytic: {:?}", r);
    }
}
