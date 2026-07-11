//! §4.5 Edge bench on the dynamic plant — the MVP acceptance test against a
//! BOUND robot. Adversarial humanlike intent (blunders full-stick toward the
//! edge) × the μ band × N seeds. Acceptance for the certified arm: ZERO
//! edge losses — any loss is exported as a counterexample trace, not
//! averaged away. The unprotected arm exists as the vacuity guard: if it
//! doesn't lose, the intent stream isn't adversarial enough to make the
//! zero-loss claim mean anything.
//!
//! The report carries the §3a factored identity: robot (hash), environment
//! (arena + μ band), protocol (this bench's parameters).

use arena_agents::{DriverParams, HumanlikeDriver, ARENA_AGENTS_VERSION};
use arena_cells::{DynEdgeFailsafe, DYN_EDGE_FAILSAFE_VERSION};
use arena_core::{ArenaGeom, Rng, Vec2, DECIMATION, WORLD_DT, WORLD_HZ};
use arena_plant::dynamic::{DynamicPlant, RigidBotSpec, RigidState, DYNAMIC_PLANT_VERSION};
use arena_plant::{DriveCmd, PlantState};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const EDGE_BENCH_VERSION: &str = "0.1.0-m1";
const FAILSAFE_MARGIN_M: f64 = 0.02;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeBenchReport {
    /// §3a factored identity.
    pub robot: RobotIdentity,
    pub environment: EnvIdentity,
    pub protocol: ProtocolIdentity,
    pub versions: EdgeVersions,
    pub certified: EdgeArm,
    pub unprotected: EdgeArm,
    pub verdict: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RobotIdentity {
    pub name: String,
    pub robot_hash: String,
    pub body_hash: String,
    pub kernel_ref: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnvIdentity {
    pub arena_half_extent_m: f64,
    pub mu_band: [f64; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProtocolIdentity {
    pub bench: String,
    pub n_seeds: u64,
    pub base_seed: u64,
    pub duration_s: f64,
    pub driver: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeVersions {
    pub edge_bench: String,
    pub plant_dynamic: String,
    pub failsafe: String,
    pub agents: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeArm {
    pub episodes: u64,
    pub edge_losses: u64,
    pub loss_seeds: Vec<u64>,
    pub mean_interventions: f64,
    pub mean_speed_ms: f64,
    pub min_edge_distance_m: f64,
    pub counterexample_traces: Vec<String>,
}

struct EpisodeOutcome {
    lost: bool,
    interventions: u64,
    mean_speed: f64,
    min_edge: f64,
    trace: Vec<[f64; 5]>,
}

fn run_episode(
    spec: &RigidBotSpec,
    geom: &ArenaGeom,
    seed: u64,
    duration_s: f64,
    protected: bool,
) -> EpisodeOutcome {
    let mut env_rng = Rng::substream(seed, "edge-env");
    let mu = env_rng.range(spec.mu_min, spec.mu_max);
    let mut params_rng = Rng::substream(seed, "edge-driver-params");
    let driver_params = DriverParams::sample(&mut params_rng);
    let mut driver =
        HumanlikeDriver::new(driver_params, Rng::substream(seed, "edge-driver"));
    let failsafe = DynEdgeFailsafe::new(spec, FAILSAFE_MARGIN_M, protected);
    let mut plant =
        DynamicPlant::new(spec.clone(), RigidState::at_rest_at(Vec2::ZERO, 0.0));

    let n_ticks = (duration_s * WORLD_HZ as f64) as u64;
    let mut cmd = DriveCmd::ZERO;
    let mut intervening = false;
    let mut interventions = 0u64;
    let mut speed_sum = 0.0;
    let mut speed_n = 0u64;
    let mut min_edge = geom.dist_to_edge(plant.state.pos);
    let mut trace: Vec<[f64; 5]> = Vec::new();
    let mut lost = false;

    for tick in 0..n_ticks {
        let t = tick as f64 * WORLD_DT;
        if tick % DECIMATION as u64 == 0 {
            // The driver sees a kinematic-shaped observation of the rigid state.
            let fwd = (plant.state.heading.cos(), plant.state.heading.sin());
            let obs = PlantState {
                pos: plant.state.pos,
                heading: plant.state.heading,
                v: plant.state.vel.x * fwd.0 + plant.state.vel.y * fwd.1,
                omega: plant.state.omega,
            };
            let raw = driver.control_tick(&obs, geom);
            let (filtered, hit) = failsafe.filter(geom, &plant.state, raw);
            cmd = filtered;
            if hit && !intervening {
                interventions += 1;
            }
            intervening = hit;
            speed_sum += plant.state.speed();
            speed_n += 1;
        }
        if tick % (WORLD_HZ as u64 / 50) == 0 {
            let s = &plant.state;
            trace.push([t, s.pos.x, s.pos.y, s.heading, s.speed()]);
        }
        plant.step_world(cmd, mu, WORLD_DT);
        let d = geom.dist_to_edge(plant.state.pos);
        if d < min_edge {
            min_edge = d;
        }
        if d < 0.0 {
            lost = true;
            let s = &plant.state;
            trace.push([t, s.pos.x, s.pos.y, s.heading, s.speed()]);
            break;
        }
    }
    EpisodeOutcome {
        lost,
        interventions,
        mean_speed: speed_sum / speed_n.max(1) as f64,
        min_edge,
        trace,
    }
}

fn run_arm(
    spec: &RigidBotSpec,
    geom: &ArenaGeom,
    seeds: &[u64],
    duration_s: f64,
    protected: bool,
    trace_dir: Option<&Path>,
    label: &str,
) -> EdgeArm {
    let mut arm = EdgeArm {
        episodes: seeds.len() as u64,
        edge_losses: 0,
        loss_seeds: Vec::new(),
        mean_interventions: 0.0,
        mean_speed_ms: 0.0,
        min_edge_distance_m: f64::INFINITY,
        counterexample_traces: Vec::new(),
    };
    let mut interventions = 0u64;
    let mut speed_sum = 0.0;
    for &seed in seeds {
        let out = run_episode(spec, geom, seed, duration_s, protected);
        interventions += out.interventions;
        speed_sum += out.mean_speed;
        arm.min_edge_distance_m = arm.min_edge_distance_m.min(out.min_edge);
        if out.lost {
            arm.edge_losses += 1;
            arm.loss_seeds.push(seed);
            if let (Some(dir), true) = (trace_dir, protected) {
                // Counterexamples only matter for the certified arm.
                let path = dir.join(format!("edge-counterexample-{label}-{seed}.json"));
                let _ = std::fs::write(
                    &path,
                    serde_json::to_string(&serde_json::json!({
                        "seed": seed,
                        "samples_t_x_y_heading_speed": out.trace,
                    }))
                    .unwrap(),
                );
                arm.counterexample_traces
                    .push(path.to_string_lossy().to_string());
            }
        }
    }
    arm.mean_interventions = interventions as f64 / seeds.len().max(1) as f64;
    arm.mean_speed_ms = speed_sum / seeds.len().max(1) as f64;
    arm
}

pub struct EdgeBenchInput<'a> {
    pub spec: &'a RigidBotSpec,
    pub robot_name: String,
    pub robot_hash: String,
    pub body_hash: String,
    pub kernel_ref: String,
}

pub fn edge_bench(
    input: &EdgeBenchInput,
    n_seeds: u64,
    base_seed: u64,
    duration_s: f64,
    trace_dir: Option<&Path>,
) -> EdgeBenchReport {
    let geom = ArenaGeom { half_extent: 0.45 };
    let mut seed_rng = Rng::substream(base_seed, "edge-corpus");
    let seeds: Vec<u64> = (0..n_seeds).map(|_| seed_rng.next_u64()).collect();

    let certified = run_arm(
        input.spec, &geom, &seeds, duration_s, true, trace_dir, "certified",
    );
    let unprotected = run_arm(
        input.spec, &geom, &seeds, duration_s, false, None, "unprotected",
    );

    let verdict = if certified.edge_losses > 0 {
        format!(
            "FINDING: {} edge losses in the certified arm — counterexamples exported, build-blocking per §4.5",
            certified.edge_losses
        )
    } else if unprotected.edge_losses == 0 {
        "VACUOUS: unprotected arm also lost nothing — intent stream not adversarial enough to support the claim".to_string()
    } else {
        format!(
            "PASS: 0/{} certified losses across the μ band; unprotected arm lost {}/{} (pressure confirmed)",
            n_seeds, unprotected.edge_losses, n_seeds
        )
    };

    EdgeBenchReport {
        robot: RobotIdentity {
            name: input.robot_name.clone(),
            robot_hash: input.robot_hash.clone(),
            body_hash: input.body_hash.clone(),
            kernel_ref: input.kernel_ref.clone(),
        },
        environment: EnvIdentity {
            arena_half_extent_m: geom.half_extent,
            mu_band: [input.spec.mu_min, input.spec.mu_max],
        },
        protocol: ProtocolIdentity {
            bench: "edge-4.5".into(),
            n_seeds,
            base_seed,
            duration_s,
            driver: format!("humanlike-{ARENA_AGENTS_VERSION}"),
        },
        versions: EdgeVersions {
            edge_bench: EDGE_BENCH_VERSION.into(),
            plant_dynamic: DYNAMIC_PLANT_VERSION.into(),
            failsafe: DYN_EDGE_FAILSAFE_VERSION.into(),
            agents: ARENA_AGENTS_VERSION.into(),
        },
        certified,
        unprotected,
        verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_plant::bind::bind_robot_from_files;
    use std::path::PathBuf;

    fn bound() -> EdgeBenchInput<'static> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let b = bind_robot_from_files(&root.join("robots/mvp-wedge.json"), &root.join("parts"))
            .unwrap();
        let spec: &'static RigidBotSpec = Box::leak(Box::new(b.spec));
        EdgeBenchInput {
            spec,
            robot_name: "mvp-wedge-r3".into(),
            robot_hash: b.robot_hash,
            body_hash: b.body_hash,
            kernel_ref: b.kernel_ref,
        }
    }

    #[test]
    fn certified_arm_zero_losses_and_pressure_confirmed() {
        let input = bound();
        let report = edge_bench(&input, 40, 7, 30.0, None);
        assert_eq!(
            report.certified.edge_losses, 0,
            "counterexample seeds: {:?}",
            report.certified.loss_seeds
        );
        assert!(
            report.unprotected.edge_losses > 0,
            "vacuous: unprotected arm never lost"
        );
        // Non-vacuous motion: the certified bot must actually drive around.
        // Under adversarial intent in a 0.9 m arena the certified arm
        // legitimately spends much of its time in envelope custody (slow
        // escape pivots included) — the guard is against a frozen bot
        // (measured 0.036 m/s when escape was impossible), not against
        // conservatism, whose cost the report states via mean_speed.
        assert!(
            report.certified.mean_speed_ms > 0.12,
            "certified arm barely moved: {} m/s",
            report.certified.mean_speed_ms
        );
        assert!(report.certified.min_edge_distance_m >= 0.0);
    }
}
