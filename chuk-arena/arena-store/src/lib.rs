//! arena-store — episode schema, identity hashing, layer version tags
//! (SPEC §2, §2.1: episode identity = hash of config; same identity =>
//! same log, byte-for-byte).

use arena_agents::DriverParams;
use arena_cells::EdgeFailsafeParams;
use arena_core::ArenaGeom;
use arena_plant::BotSpec;
use serde::{Deserialize, Serialize};

pub const ARENA_STORE_VERSION: &str = "0.1.0-m0";

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Version tag of every layer, embedded in every episode record (SPEC §2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerVersions {
    pub core: String,
    pub plant: String,
    pub agents: String,
    pub cells: String,
    pub store: String,
    pub tourney: String,
}

/// Everything needed to reproduce an episode. Stochastic elements (mu draw,
/// driver skill, blunder timing) derive from `seed` inside the run; the
/// distributions they are drawn from are pinned by the layer version tags.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeConfig {
    pub versions: LayerVersions,
    pub arena: ArenaGeom,
    pub bot: BotSpec,
    pub kernel: EdgeFailsafeParams,
    pub duration_s: f64,
    pub seed: u64,
}

impl EpisodeConfig {
    /// Episode identity per SPEC §2.1.
    pub fn identity(&self) -> String {
        sha256_hex(&serde_json::to_vec(self).expect("config serializes"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Outcome {
    Survived,
    /// Analytic crossing time, interpolated inside the world tick (SPEC §1:
    /// events are analytic over the 2D core, never timestep-refined).
    EdgeOut { t: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Event {
    InterventionStart { t: f64 },
    InterventionEnd { t: f64 },
    EdgeOut { t: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub t: f64,
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub v: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeResult {
    pub outcome: Outcome,
    /// Resolved per-episode draws, recorded for traceability.
    pub mu: f64,
    pub driver: DriverParams,
    /// Number of distinct intervention windows (off->on transitions).
    pub interventions: u64,
    /// Minimum CoG distance to the edge over the episode, m.
    pub min_edge_distance: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeLog {
    pub config: EpisodeConfig,
    pub identity: String,
    pub result: EpisodeResult,
    pub events: Vec<Event>,
    /// 50 Hz state samples (decimated from the 8 kHz world).
    pub samples: Vec<Sample>,
}

impl EpisodeLog {
    pub fn log_hash(&self) -> String {
        sha256_hex(&serde_json::to_vec(self).expect("log serializes"))
    }
}
