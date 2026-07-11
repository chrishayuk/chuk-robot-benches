//! The `power:` section (specs/robotspec.md §3, specs/robowire.md §4 item
//! 1) — plain data, no logic. Owned here (rather than in `robowire`) so it
//! can live on `DerivedRecord`, but `robowire` is what actually computes it:
//! `robowire` already depends on `robotspec` (the other way would be
//! circular), so it constructs these types directly, mirroring how
//! `robowire::view` already reads `robotspec::schema::RobotSpec` without
//! robotspec ever depending back on robowire.

use serde::{Deserialize, Serialize};

/// One electrical source (battery, or a regulator/BEC output pin) and its
/// worst-case draw vs its declared capability — the same numbers robowire's
/// E30 check already computes, exposed here so RobotSpec's derived record
/// carries them too, not just a pass/fail.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerRail {
    /// The battery instance id, or "instance.PIN" for a regulator/BEC output.
    pub source: String,
    pub worst_case_a: f64,
    /// `None` when the source declares no capacity (e.g. a regulator with
    /// no `max_a`) — not fabricated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_a: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin_a: Option<f64>,
}

/// A wire segment: one net with a declared gauge/length, its derived
/// resistance, and its worst-case current vs ampacity — the same figures
/// robowire's E31 check already computes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WireSegment {
    pub net: String,
    pub gauge_awg: u32,
    pub length_mm: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resistance_ohms: Option<f64>,
    pub worst_case_a: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ampacity_a: Option<f64>,
}

/// A source -> ESC -> motor chain (specs/robotspec.md §2's "source→ESC→motor
/// chains"). `source` is empty if no battery's reachability includes the
/// ESC's own supply net (a malformed or not-yet-fully-wired harness) — not
/// fabricated.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerChain {
    pub source: String,
    pub esc: String,
    pub motor: String,
}

/// The full power graph. `sense_points` is honestly empty in v1 — no
/// current-sense part exists in the catalogue yet, so nothing is
/// fabricated there; it's kept as a field so the shape matches
/// specs/robotspec.md §3's schema sketch and can be populated later without
/// a breaking change.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerGraph {
    pub rails: Vec<PowerRail>,
    pub segments: Vec<WireSegment>,
    pub chains: Vec<PowerChain>,
    #[serde(default)]
    pub sense_points: Vec<String>,
}
