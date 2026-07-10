//! arena-core — deterministic world primitives: clock, PRNG, planar math, edge geometry.
//!
//! Determinism contract (SPEC §2.1): world integrates at 8kHz, control decimated to
//! 1kHz; all stochastic draws come from the episode seed via domain-separated
//! substreams; no HashMaps, no wall clock, no platform-dependent iteration order.

use serde::{Deserialize, Serialize};

pub mod contact;

/// Version of the M0 world primitives — unchanged so the banked corpus stays
/// reproducible; the contact module carries contact::CONTACT_VERSION.
pub const ARENA_CORE_VERSION: &str = "0.1.0-m0";

pub const WORLD_HZ: u32 = 8_000;
pub const CONTROL_HZ: u32 = 1_000;
pub const DECIMATION: u32 = WORLD_HZ / CONTROL_HZ;
pub const WORLD_DT: f64 = 1.0 / WORLD_HZ as f64;
pub const CONTROL_DT: f64 = 1.0 / CONTROL_HZ as f64;
pub const GRAVITY: f64 = 9.80665;

// ---------------------------------------------------------------------------
// PRNG: xoshiro256++ seeded via splitmix64, with FNV-1a domain separation.
// Owned implementation so the bit stream is pinned by this crate's version tag,
// not by an external crate's release history.
// ---------------------------------------------------------------------------

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    pub fn from_seed(seed: u64) -> Self {
        let mut sm = seed;
        Rng {
            s: [
                splitmix64(&mut sm),
                splitmix64(&mut sm),
                splitmix64(&mut sm),
                splitmix64(&mut sm),
            ],
        }
    }

    /// Domain-separated substream: same episode seed, independent streams per
    /// subsystem ("driver", "env", ...), so adding a consumer never perturbs
    /// the draws seen by existing ones.
    pub fn substream(seed: u64, domain: &str) -> Self {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in domain.as_bytes() {
            h = (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01B3);
        }
        Self::from_seed(seed ^ h)
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[0]
            .wrapping_add(self.s[3])
            .rotate_left(23)
            .wrapping_add(self.s[0]);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in [0, 1) with 53 bits of precision.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    /// Polar Box-Muller, one value per call. Deterministic on the reference
    /// platform per §2.1 (cross-platform bit-determinism not claimed in v0.1).
    pub fn normal(&mut self, mean: f64, sd: f64) -> f64 {
        loop {
            let u = 2.0 * self.next_f64() - 1.0;
            let v = 2.0 * self.next_f64() - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                return mean + sd * u * (-2.0 * s.ln() / s).sqrt();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Planar math
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }

    pub fn norm(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}

impl std::ops::Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}

/// Wrap an angle to (-pi, pi].
pub fn wrap_pi(a: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    let w = (a + std::f64::consts::PI).rem_euclid(two_pi);
    w - std::f64::consts::PI
}

// ---------------------------------------------------------------------------
// Arena geometry: square, edge-out (no walls), centred at the origin.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArenaGeom {
    pub half_extent: f64,
}

impl ArenaGeom {
    /// Distance from `p` to the nearest edge (Chebyshev). Negative = outside.
    pub fn dist_to_edge(&self, p: Vec2) -> f64 {
        self.half_extent - p.x.abs().max(p.y.abs())
    }

    pub fn is_out(&self, p: Vec2) -> bool {
        self.dist_to_edge(p) < 0.0
    }

    /// Point just beyond the nearest edge from `p` — a blunder target that a
    /// driver "aiming off the arena" would steer toward.
    pub fn nearest_edge_overshoot(&self, p: Vec2, overshoot: f64) -> Vec2 {
        if p.x.abs() >= p.y.abs() {
            let sx = if p.x >= 0.0 { 1.0 } else { -1.0 };
            Vec2::new(sx * (self.half_extent + overshoot), p.y)
        } else {
            let sy = if p.y >= 0.0 { 1.0 } else { -1.0 };
            Vec2::new(p.x, sy * (self.half_extent + overshoot))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_streams_are_deterministic_and_domain_separated() {
        let mut a = Rng::from_seed(42);
        let mut b = Rng::from_seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut d1 = Rng::substream(42, "driver");
        let mut d2 = Rng::substream(42, "env");
        assert_ne!(d1.next_u64(), d2.next_u64());
    }

    #[test]
    fn decimation_is_exact() {
        assert_eq!(DECIMATION, 8);
        assert_eq!(WORLD_HZ % CONTROL_HZ, 0);
    }

    #[test]
    fn edge_distance() {
        let g = ArenaGeom { half_extent: 0.45 };
        assert!((g.dist_to_edge(Vec2::ZERO) - 0.45).abs() < 1e-12);
        assert!((g.dist_to_edge(Vec2::new(0.4, -0.1)) - 0.05).abs() < 1e-12);
        assert!(g.is_out(Vec2::new(0.46, 0.0)));
    }
}
