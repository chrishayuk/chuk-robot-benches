//! arena-tourney — episode runner and experiment harness. M0 scope: the
//! single-bot episode machine and the edge-failsafe ablation experiment
//! (SPEC §10 M0). Match play and tournaments arrive at M2.

use arena_agents::{DriverParams, HumanlikeDriver, ARENA_AGENTS_VERSION};
use arena_cells::{EdgeFailsafeCell, EdgeFailsafeParams, ARENA_CELLS_VERSION};
use arena_core::{
    ArenaGeom, Rng, Vec2, ARENA_CORE_VERSION, DECIMATION, WORLD_DT, WORLD_HZ,
};
use arena_plant::{BotSpec, DriveCmd, KinematicPlant, PlantState, ARENA_PLANT_VERSION};
use arena_store::{
    EpisodeConfig, EpisodeLog, EpisodeResult, Event, LayerVersions, Outcome, Sample,
    ARENA_STORE_VERSION,
};
use serde::{Deserialize, Serialize};

pub const ARENA_TOURNEY_VERSION: &str = "0.1.0-m0";

/// State samples recorded at 50 Hz.
const SAMPLE_EVERY_TICKS: u64 = (WORLD_HZ as u64) / 50;

pub fn layer_versions() -> LayerVersions {
    LayerVersions {
        core: ARENA_CORE_VERSION.to_string(),
        plant: ARENA_PLANT_VERSION.to_string(),
        agents: ARENA_AGENTS_VERSION.to_string(),
        cells: ARENA_CELLS_VERSION.to_string(),
        store: ARENA_STORE_VERSION.to_string(),
        tourney: ARENA_TOURNEY_VERSION.to_string(),
    }
}

pub fn m0_config(seed: u64, kernel: EdgeFailsafeParams, duration_s: f64) -> EpisodeConfig {
    EpisodeConfig {
        versions: layer_versions(),
        arena: ArenaGeom { half_extent: 0.45 },
        bot: BotSpec::default_m0(),
        kernel,
        duration_s,
        seed,
    }
}

/// The whole mutable state of a running episode. Fully serializable so the
/// serialize-roundtrip leg of the determinism fuzz (SPEC §2.1) can suspend an
/// episode mid-flight, round-trip it, and require bit-identical completion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeMachine {
    pub config: EpisodeConfig,
    mu: f64,
    plant: KinematicPlant,
    driver: HumanlikeDriver,
    cell: EdgeFailsafeCell,
    current_cmd: DriveCmd,
    intervening: bool,
    interventions: u64,
    min_edge_distance: f64,
    tick: u64,
    n_ticks: u64,
    events: Vec<Event>,
    samples: Vec<Sample>,
    outcome: Option<Outcome>,
}

impl EpisodeMachine {
    pub fn new(config: EpisodeConfig) -> Self {
        let seed = config.seed;
        let mut env_rng = Rng::substream(seed, "env");
        let mu = env_rng.range(config.bot.mu_min, config.bot.mu_max);
        let mut params_rng = Rng::substream(seed, "driver-params");
        let driver_params = DriverParams::sample(&mut params_rng);
        let driver = HumanlikeDriver::new(driver_params, Rng::substream(seed, "driver"));
        let cell = EdgeFailsafeCell::new(&config.bot, config.kernel.clone());
        let plant = KinematicPlant::new(
            config.bot.clone(),
            PlantState::at_rest_at(Vec2::ZERO, 0.0),
        );
        let n_ticks = (config.duration_s * WORLD_HZ as f64).round() as u64;
        let min_edge_distance = config.arena.dist_to_edge(plant.state.pos);
        EpisodeMachine {
            config,
            mu,
            plant,
            driver,
            cell,
            current_cmd: DriveCmd::ZERO,
            intervening: false,
            interventions: 0,
            min_edge_distance,
            tick: 0,
            n_ticks,
            events: Vec::new(),
            samples: Vec::new(),
            outcome: None,
        }
    }

    pub fn done(&self) -> bool {
        self.outcome.is_some() || self.tick >= self.n_ticks
    }

    /// One world tick (control runs on the decimated 1 kHz grid).
    pub fn step(&mut self) {
        debug_assert!(!self.done());
        let t = self.tick as f64 * WORLD_DT;

        if self.tick % DECIMATION as u64 == 0 {
            let raw = self
                .driver
                .control_tick(&self.plant.state, &self.config.arena);
            let (cmd, intervened) =
                self.cell
                    .filter(&self.config.arena, &self.plant.state, raw);
            self.current_cmd = cmd;
            if intervened && !self.intervening {
                self.interventions += 1;
                self.events.push(Event::InterventionStart { t });
            } else if !intervened && self.intervening {
                self.events.push(Event::InterventionEnd { t });
            }
            self.intervening = intervened;
        }

        if self.tick % SAMPLE_EVERY_TICKS == 0 {
            let s = &self.plant.state;
            self.samples.push(Sample {
                t,
                x: s.pos.x,
                y: s.pos.y,
                heading: s.heading,
                v: s.v,
            });
        }

        let prev_dist = self.config.arena.dist_to_edge(self.plant.state.pos);
        self.plant.step_world(self.current_cmd, self.mu, WORLD_DT);
        let dist = self.config.arena.dist_to_edge(self.plant.state.pos);
        if dist < self.min_edge_distance {
            self.min_edge_distance = dist;
        }
        self.tick += 1;

        if dist < 0.0 {
            // Analytic crossing time: linear interpolation of the edge
            // distance across the tick that crossed zero.
            let frac = if prev_dist > dist {
                (prev_dist / (prev_dist - dist)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let t_cross = t + frac * WORLD_DT;
            self.events.push(Event::EdgeOut { t: t_cross });
            self.outcome = Some(Outcome::EdgeOut { t: t_cross });
        }
    }

    pub fn finish(self) -> EpisodeLog {
        debug_assert!(self.done());
        let identity = self.config.identity();
        let result = EpisodeResult {
            outcome: self.outcome.unwrap_or(Outcome::Survived),
            mu: self.mu,
            driver: self.driver.params.clone(),
            interventions: self.interventions,
            min_edge_distance: self.min_edge_distance,
        };
        EpisodeLog {
            config: self.config,
            identity,
            result,
            events: self.events,
            samples: self.samples,
        }
    }
}

pub fn run_episode(config: EpisodeConfig) -> EpisodeLog {
    let mut m = EpisodeMachine::new(config);
    while !m.done() {
        m.step();
    }
    m.finish()
}

pub mod experiments {
    use super::*;

    /// 95% Wilson score interval for a binomial proportion.
    pub fn wilson_ci(successes: u64, n: u64) -> (f64, f64) {
        if n == 0 {
            return (0.0, 1.0);
        }
        let z = 1.959963984540054_f64;
        let nf = n as f64;
        let p = successes as f64 / nf;
        let z2 = z * z;
        let denom = 1.0 + z2 / nf;
        let centre = p + z2 / (2.0 * nf);
        let half = z * (p * (1.0 - p) / nf + z2 / (4.0 * nf * nf)).sqrt();
        (((centre - half) / denom).max(0.0), ((centre + half) / denom).min(1.0))
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ArmSummary {
        pub episodes: u64,
        pub edge_losses: u64,
        pub loss_rate: f64,
        pub loss_rate_ci95: (f64, f64),
        pub mean_interventions: f64,
        pub mean_min_edge_distance: f64,
    }

    /// The M0 pre-registered claim record (SPEC §10 M0: failsafe ablation).
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct AblationReport {
        pub versions: LayerVersions,
        pub n_per_arm: u64,
        pub base_seed: u64,
        pub duration_s: f64,
        pub kernel_off: ArmSummary,
        pub kernel_on: ArmSummary,
        /// sha256 over the ordered per-episode log hashes of both arms.
        pub corpus_hash: String,
    }

    fn run_arm(
        n: u64,
        seeds: &[u64],
        kernel: EdgeFailsafeParams,
        duration_s: f64,
        corpus: &mut String,
    ) -> ArmSummary {
        let mut losses = 0u64;
        let mut interventions = 0u64;
        let mut min_edge_sum = 0.0f64;
        for &seed in seeds {
            let log = run_episode(m0_config(seed, kernel.clone(), duration_s));
            if matches!(log.result.outcome, Outcome::EdgeOut { .. }) {
                losses += 1;
            }
            interventions += log.result.interventions;
            min_edge_sum += log.result.min_edge_distance;
            corpus.push_str(&log.log_hash());
        }
        let nf = n as f64;
        ArmSummary {
            episodes: n,
            edge_losses: losses,
            loss_rate: losses as f64 / nf,
            loss_rate_ci95: wilson_ci(losses, n),
            mean_interventions: interventions as f64 / nf,
            mean_min_edge_distance: min_edge_sum / nf,
        }
    }

    /// Edge-failsafe ablation: identical seeds (same drivers, same mu draws,
    /// same blunder timings) with the kernel off vs on.
    pub fn failsafe_ablation(n_per_arm: u64, base_seed: u64, duration_s: f64) -> AblationReport {
        let mut seed_rng = Rng::substream(base_seed, "corpus");
        let seeds: Vec<u64> = (0..n_per_arm).map(|_| seed_rng.next_u64()).collect();
        let mut corpus = String::new();
        let kernel_off = run_arm(
            n_per_arm,
            &seeds,
            EdgeFailsafeParams::disabled(),
            duration_s,
            &mut corpus,
        );
        let kernel_on = run_arm(
            n_per_arm,
            &seeds,
            EdgeFailsafeParams::enabled_default(),
            duration_s,
            &mut corpus,
        );
        AblationReport {
            versions: layer_versions(),
            n_per_arm,
            base_seed,
            duration_s,
            kernel_off,
            kernel_on,
            corpus_hash: arena_store::sha256_hex(corpus.as_bytes()),
        }
    }
}
