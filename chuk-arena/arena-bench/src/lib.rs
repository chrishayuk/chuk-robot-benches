//! arena-bench — virtual benches (SPEC §4). M1 scope: braking/envelope bench
//! (§4.2) and dyno bench (§4.1). Benches are pure parameter sweeps: no RNG,
//! fully deterministic, reports carry version tags and a PASS/FINDING verdict.

use arena_cells::{ActiveBrakeCell, ACTIVE_BRAKE_CELL_VERSION, ARENA_CELLS_VERSION};
use arena_core::{Vec2, DECIMATION, GRAVITY, WORLD_DT, WORLD_HZ};
use arena_plant::dynamic::{DynamicPlant, RigidBotSpec, RigidState, DYNAMIC_PLANT_VERSION};
use arena_plant::DriveCmd;
use serde::{Deserialize, Serialize};

pub const ARENA_BENCH_VERSION: &str = "0.1.0-m1";

/// Speed below which the bot counts as stopped.
const V_STOPPED: f64 = 0.005;
/// Give-up horizon for a single braking scenario.
const BRAKE_TIMEOUT_S: f64 = 5.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchVersions {
    pub bench: String,
    pub plant_dynamic: String,
    pub cells: String,
    pub active_brake_cell: String,
}

pub fn bench_versions() -> BenchVersions {
    BenchVersions {
        bench: ARENA_BENCH_VERSION.to_string(),
        plant_dynamic: DYNAMIC_PLANT_VERSION.to_string(),
        cells: ARENA_CELLS_VERSION.to_string(),
        active_brake_cell: ACTIVE_BRAKE_CELL_VERSION.to_string(),
    }
}

// ---------------------------------------------------------------------------
// §4.2 Braking / envelope bench
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrakeKernel {
    /// M0 edge-failsafe braking action: coast (throttle 0), which on the
    /// dynamic plant is back-EMF braking that fades linearly with speed.
    /// Certified prediction: the M0 constant-deceleration formula.
    NaiveCoast,
    /// M1 aligned active braking with μ-band + sag worst-case certification.
    ActiveAligned,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct EnvelopeSample {
    pub v0: f64,
    pub slip_deg: f64,
    pub mu: f64,
    pub omega0: f64,
    pub certified_m: f64,
    pub achieved_m: f64,
    /// certified - achieved; negative = safety finding.
    pub margin_m: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnvelopeReport {
    pub versions: BenchVersions,
    pub kernel: BrakeKernel,
    pub samples: u64,
    pub min_margin_m: f64,
    pub mean_margin_m: f64,
    pub worst: EnvelopeSample,
    /// Every negative-margin sample, in full: these are filed findings
    /// (SPEC §4.2: a single negative sample is build-blocking).
    pub violations: Vec<EnvelopeSample>,
    pub verdict: String,
}

/// Sweep grids (SPEC §4.2: v, heading-vs-velocity, μ, plus initial yaw rate
/// for the rotate-then-brake case).
const V0_GRID: [f64; 5] = [0.3, 0.6, 0.9, 1.2, 1.5];
const SLIP_DEG_GRID: [f64; 3] = [0.0, 30.0, 90.0];
const OMEGA0_GRID: [f64; 2] = [0.0, 6.0];
const MU_STEPS: u32 = 7; // 0.40 ..= 0.70 in 0.05 steps

fn mu_at(i: u32, spec: &RigidBotSpec) -> f64 {
    spec.mu_min + (spec.mu_max - spec.mu_min) * (i as f64) / ((MU_STEPS - 1) as f64)
}

/// The naive (M0) kernel's certified stopping distance for the dynamic bot's
/// equivalent parameters — the prediction §4.2 holds it to.
pub fn naive_certified(spec: &RigidBotSpec, v: f64) -> f64 {
    let a_motor = spec.wheels.len() as f64 * spec.motor.stall_force / spec.mass_kg;
    let a_brake = a_motor.min(spec.mu_min * GRAVITY);
    let v1 = (v + a_motor * arena_core::CONTROL_DT).min(spec.motor.no_load_speed);
    v1 * v1 / (2.0 * a_brake) + v1 * arena_core::CONTROL_DT
}

/// Run one braking scenario; returns the maximum excursion along the initial
/// velocity direction (the safety-relevant distance toward a hypothetical
/// edge), in metres. If `trace_every_ticks > 0`, appends
/// [t, x, y, heading, speed] samples to `sink` at that world-tick stride —
/// this is the same code path the browser bench console drives via WASM.
pub fn run_brake_traced(
    spec: &RigidBotSpec,
    kernel: BrakeKernel,
    cell: &ActiveBrakeCell,
    v0: f64,
    slip_rad: f64,
    mu: f64,
    omega0: f64,
    trace_every_ticks: u64,
    sink: &mut Vec<[f64; 5]>,
) -> f64 {
    let mut st = RigidState::at_rest_at(Vec2::ZERO, 0.0);
    st.vel = Vec2::new(v0 * slip_rad.cos(), v0 * slip_rad.sin());
    st.omega = omega0;
    let mut plant = DynamicPlant::new(spec.clone(), st);

    let v_dir = Vec2::new(slip_rad.cos(), slip_rad.sin());
    let mut max_excursion = 0.0f64;
    let mut cmd = DriveCmd::ZERO;
    let n_ticks = (BRAKE_TIMEOUT_S * WORLD_HZ as f64) as u64;

    for tick in 0..n_ticks {
        if tick % DECIMATION as u64 == 0 {
            cmd = match kernel {
                BrakeKernel::NaiveCoast => DriveCmd::ZERO,
                BrakeKernel::ActiveAligned => {
                    let fwd = Vec2::new(
                        plant.state.heading.cos(),
                        plant.state.heading.sin(),
                    );
                    cell.brake_cmd(plant.state.vel.dot(fwd), plant.state.speed())
                }
            };
        }
        if trace_every_ticks > 0 && tick % trace_every_ticks == 0 {
            sink.push([
                tick as f64 * WORLD_DT,
                plant.state.pos.x,
                plant.state.pos.y,
                plant.state.heading,
                plant.state.speed(),
            ]);
        }
        plant.step_world(cmd, mu, WORLD_DT);
        let excursion = plant.state.pos.dot(v_dir);
        if excursion > max_excursion {
            max_excursion = excursion;
        }
        if plant.state.speed() < V_STOPPED {
            break;
        }
    }
    if trace_every_ticks > 0 {
        let t_final = sink.last().map_or(0.0, |s| s[0]) + trace_every_ticks as f64 * WORLD_DT;
        sink.push([
            t_final,
            plant.state.pos.x,
            plant.state.pos.y,
            plant.state.heading,
            plant.state.speed(),
        ]);
    }
    max_excursion
}

fn run_brake_scenario(
    spec: &RigidBotSpec,
    kernel: BrakeKernel,
    cell: &ActiveBrakeCell,
    v0: f64,
    slip_rad: f64,
    mu: f64,
    omega0: f64,
) -> f64 {
    let mut no_trace = Vec::new();
    run_brake_traced(spec, kernel, cell, v0, slip_rad, mu, omega0, 0, &mut no_trace)
}

pub fn envelope_bench(kernel: BrakeKernel) -> EnvelopeReport {
    let spec = RigidBotSpec::default_m1();
    let cell = ActiveBrakeCell::new(&spec);
    let mut samples = Vec::new();
    for &v0 in &V0_GRID {
        for &slip_deg in &SLIP_DEG_GRID {
            for mu_i in 0..MU_STEPS {
                for &omega0 in &OMEGA0_GRID {
                    let mu = mu_at(mu_i, &spec);
                    let certified = match kernel {
                        // The naive kernel doesn't model rotation at all —
                        // that's part of what makes it naive.
                        BrakeKernel::NaiveCoast => naive_certified(&spec, v0),
                        BrakeKernel::ActiveAligned => {
                            cell.certified_stop_distance(v0, omega0)
                        }
                    };
                    let achieved = run_brake_scenario(
                        &spec,
                        kernel,
                        &cell,
                        v0,
                        slip_deg.to_radians(),
                        mu,
                        omega0,
                    );
                    samples.push(EnvelopeSample {
                        v0,
                        slip_deg,
                        mu,
                        omega0,
                        certified_m: certified,
                        achieved_m: achieved,
                        margin_m: certified - achieved,
                    });
                }
            }
        }
    }

    let n = samples.len() as u64;
    let worst = *samples
        .iter()
        .min_by(|a, b| a.margin_m.partial_cmp(&b.margin_m).unwrap())
        .unwrap();
    let mean = samples.iter().map(|s| s.margin_m).sum::<f64>() / n as f64;
    let violations: Vec<EnvelopeSample> =
        samples.iter().copied().filter(|s| s.margin_m < 0.0).collect();
    let verdict = if violations.is_empty() {
        "PASS".to_string()
    } else {
        format!("FINDING: {} negative-margin samples (build-blocking per §4.2)", violations.len())
    };
    EnvelopeReport {
        versions: bench_versions(),
        kernel,
        samples: n,
        min_margin_m: worst.margin_m,
        mean_margin_m: mean,
        worst,
        violations,
        verdict,
    }
}

// ---------------------------------------------------------------------------
// §4.1 Dyno bench
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DynoRow {
    pub mu: f64,
    pub top_speed_ms: f64,
    pub t_to_1ms_s: f64,
    pub stop_coast_m: f64,
    pub stop_active_m: f64,
    /// Sustained push force at stall (traction- or motor-limited), N.
    pub push_stall_n: f64,
    pub peak_current_a: f64,
    pub min_voltage_ratio: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DynoReport {
    pub versions: BenchVersions,
    pub bot: String,
    pub rows: Vec<DynoRow>,
}

pub fn dyno_bench() -> DynoReport {
    let spec = RigidBotSpec::default_m1();
    let cell = ActiveBrakeCell::new(&spec);
    let mut rows = Vec::new();

    for mu_i in 0..MU_STEPS {
        let mu = mu_at(mu_i, &spec);

        // Full-throttle run: top speed, 0->1 m/s, current, sag.
        let mut plant =
            DynamicPlant::new(spec.clone(), RigidState::at_rest_at(Vec2::ZERO, 0.0));
        let full = DriveCmd { throttle: 1.0, turn: 0.0 };
        let mut top_speed = 0.0f64;
        let mut t_to_1 = f64::NAN;
        let mut peak_current = 0.0f64;
        let mut min_vr = 1.0f64;
        let run_ticks = (4.0 * WORLD_HZ as f64) as u64;
        for tick in 0..run_ticks {
            plant.step_world(full, mu, WORLD_DT);
            let v = plant.state.speed();
            if v > top_speed {
                top_speed = v;
            }
            if t_to_1.is_nan() && v >= 1.0 {
                t_to_1 = (tick + 1) as f64 * WORLD_DT;
            }
            peak_current = peak_current.max(plant.state.current_a);
            min_vr = min_vr.min(
                (spec.battery.v_nominal
                    - plant.state.current_a * spec.battery.r_internal)
                    / spec.battery.v_nominal,
            );
        }

        // Stopping distances from top speed.
        let from_top = plant.state.clone();
        let stop_coast = measure_stop(&spec, from_top.clone(), mu, |_p| DriveCmd::ZERO);
        let stop_active = measure_stop(&spec, from_top, mu, |p| {
            let fwd = Vec2::new(p.state.heading.cos(), p.state.heading.sin());
            cell.brake_cmd(p.state.vel.dot(fwd), p.state.speed())
        });

        // Push force at stall: fixed-point on battery sag (current feeds
        // voltage feeds force), then the resolved traction-limited force.
        let mut pinned =
            DynamicPlant::new(spec.clone(), RigidState::at_rest_at(Vec2::ZERO, 0.0));
        let mut push = 0.0;
        for _ in 0..6 {
            let r = pinned.resolve(full, mu, WORLD_DT);
            push = r.force.norm();
            pinned.state.current_a = r.current_a;
        }

        rows.push(DynoRow {
            mu,
            top_speed_ms: top_speed,
            t_to_1ms_s: t_to_1,
            stop_coast_m: stop_coast,
            stop_active_m: stop_active,
            push_stall_n: push,
            peak_current_a: peak_current,
            min_voltage_ratio: min_vr,
        });
    }

    DynoReport {
        versions: bench_versions(),
        bot: spec.name.clone(),
        rows,
    }
}

fn measure_stop(
    spec: &RigidBotSpec,
    start: RigidState,
    mu: f64,
    mut policy: impl FnMut(&DynamicPlant) -> DriveCmd,
) -> f64 {
    let mut plant = DynamicPlant::new(spec.clone(), start);
    let p0 = plant.state.pos;
    let dir = {
        let v = plant.state.vel;
        let n = v.norm();
        if n == 0.0 { Vec2::new(1.0, 0.0) } else { v * (1.0 / n) }
    };
    let mut max_excursion = 0.0f64;
    let mut cmd = DriveCmd::ZERO;
    let n_ticks = (BRAKE_TIMEOUT_S * WORLD_HZ as f64) as u64;
    for tick in 0..n_ticks {
        if tick % DECIMATION as u64 == 0 {
            cmd = policy(&plant);
        }
        plant.step_world(cmd, mu, WORLD_DT);
        let e = (plant.state.pos - p0).dot(dir);
        if e > max_excursion {
            max_excursion = e;
        }
        if plant.state.speed() < V_STOPPED {
            break;
        }
    }
    max_excursion
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naive_coast_kernel_files_envelope_findings() {
        // The M0 kernel's constant-decel certification is wrong in kind for
        // back-EMF coast braking — the envelope bench must catch it.
        let r = envelope_bench(BrakeKernel::NaiveCoast);
        assert!(
            !r.violations.is_empty(),
            "expected findings against the naive kernel, got none"
        );
    }

    #[test]
    fn active_aligned_kernel_passes_envelope() {
        let r = envelope_bench(BrakeKernel::ActiveAligned);
        assert!(
            r.violations.is_empty(),
            "certified kernel violated envelope: worst {:?}",
            r.worst
        );
    }

    #[test]
    fn dyno_monotone_in_mu_where_traction_limited() {
        let r = dyno_bench();
        for w in r.rows.windows(2) {
            assert!(
                w[1].push_stall_n >= w[0].push_stall_n - 1e-9,
                "push force not monotone in mu"
            );
        }
    }
}
