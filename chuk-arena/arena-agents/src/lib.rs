//! arena-agents — scripted drivers and noise-injected intent streams.
//! M0 scope: the humanlike single-bot driver used by the edge-failsafe
//! ablation (SPEC §10 M0); opponent archetypes arrive at M2.

use std::collections::VecDeque;

use arena_core::{wrap_pi, ArenaGeom, Rng, Vec2, CONTROL_DT};
use arena_plant::{DriveCmd, PlantState};
use serde::{Deserialize, Serialize};

pub const ARENA_AGENTS_VERSION: &str = "0.1.0-m0";

/// Skill parameters — the human meta as a distribution, not a constant
/// (SPEC §5.1). Sampled per episode from the seed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DriverParams {
    /// Command latency, s (queue between decision and actuation).
    pub reaction_latency_s: f64,
    /// Heading aim noise, rad (std of per-tick normal draw).
    pub aim_noise: f64,
    /// Throttle scale, 0..1.
    pub aggression: f64,
    /// Poisson-ish rate of full-stick-toward-edge blunders, 1/s.
    pub blunder_rate_hz: f64,
    /// Nominal blunder duration, s (actual draw is 0.7–1.3x).
    pub blunder_duration_s: f64,
}

impl DriverParams {
    /// Draw one driver from the M0 human-meta distribution (SPEC §5.1 ranges).
    pub fn sample(rng: &mut Rng) -> Self {
        DriverParams {
            reaction_latency_s: rng.range(0.15, 0.30),
            aim_noise: rng.range(0.02, 0.12),
            aggression: rng.range(0.6, 1.0),
            blunder_rate_hz: rng.range(0.08, 0.20),
            blunder_duration_s: rng.range(0.5, 1.1),
        }
    }
}

/// Waypoint-chasing driver with reaction latency, aim noise, and sampled
/// blunder windows during which it drives full-stick at the nearest edge —
/// the adversarial intent stream for the edge bench (SPEC §4.5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HumanlikeDriver {
    pub params: DriverParams,
    rng: Rng,
    latency_queue: VecDeque<DriveCmd>,
    waypoint: Vec2,
    waypoint_timer_s: f64,
    blunder_timer_s: f64,
    blunder_target: Vec2,
}

impl HumanlikeDriver {
    pub fn new(params: DriverParams, rng: Rng) -> Self {
        let lat_ticks = (params.reaction_latency_s / CONTROL_DT).round() as usize;
        let mut latency_queue = VecDeque::with_capacity(lat_ticks + 1);
        for _ in 0..lat_ticks {
            latency_queue.push_back(DriveCmd::ZERO);
        }
        HumanlikeDriver {
            params,
            rng,
            latency_queue,
            waypoint: Vec2::ZERO,
            waypoint_timer_s: 0.0,
            blunder_timer_s: 0.0,
            blunder_target: Vec2::ZERO,
        }
    }

    pub fn in_blunder(&self) -> bool {
        self.blunder_timer_s > 0.0
    }

    /// One control tick: returns the (latency-delayed) command.
    pub fn control_tick(&mut self, state: &PlantState, geom: &ArenaGeom) -> DriveCmd {
        // Waypoint churn keeps the bot moving across the whole floor,
        // including near the edge.
        self.waypoint_timer_s -= CONTROL_DT;
        if self.waypoint_timer_s <= 0.0 {
            let r = geom.half_extent * 0.8;
            self.waypoint = Vec2::new(self.rng.range(-r, r), self.rng.range(-r, r));
            self.waypoint_timer_s = self.rng.range(1.5, 4.0);
        }

        // Blunder onset: full stick toward (and past) the nearest edge.
        if self.blunder_timer_s <= 0.0
            && self
                .rng
                .chance(self.params.blunder_rate_hz * CONTROL_DT)
        {
            self.blunder_timer_s =
                self.params.blunder_duration_s * self.rng.range(0.7, 1.3);
            self.blunder_target =
                geom.nearest_edge_overshoot(state.pos, geom.half_extent * 0.4);
        }

        let blundering = self.blunder_timer_s > 0.0;
        let target = if blundering {
            self.blunder_timer_s -= CONTROL_DT;
            self.blunder_target
        } else {
            self.waypoint
        };

        let to = target - state.pos;
        let desired =
            to.y.atan2(to.x) + self.rng.normal(0.0, self.params.aim_noise);
        let err = wrap_pi(desired - state.heading);
        let turn = (2.0 * err).clamp(-1.0, 1.0);
        let throttle = if blundering {
            1.0
        } else {
            (self.params.aggression * err.cos().max(0.0)).clamp(0.0, 1.0)
        };

        let cmd = DriveCmd { throttle, turn };
        self.latency_queue.push_back(cmd);
        // Non-empty by construction: we just pushed.
        self.latency_queue.pop_front().unwrap()
    }
}
