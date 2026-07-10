//! Determinism fuzz, SPEC §2.1: rerun and serialize-roundtrip legs must be
//! bit-identical. (The fresh-process leg lives in arena-cli/tests.)

use arena_cells::EdgeFailsafeParams;
use arena_tourney::{m0_config, run_episode, EpisodeMachine};

const FUZZ_SEEDS: u64 = 16;
const DURATION_S: f64 = 20.0;

#[test]
fn rerun_is_bit_identical() {
    for seed in 0..FUZZ_SEEDS {
        let cfg = m0_config(seed, EdgeFailsafeParams::enabled_default(), DURATION_S);
        let a = run_episode(cfg.clone());
        let b = run_episode(cfg);
        assert_eq!(a, b, "seed {seed}: rerun diverged");
        assert_eq!(a.log_hash(), b.log_hash());
    }
}

#[test]
fn serialize_roundtrip_mid_episode_is_bit_identical() {
    for seed in 0..FUZZ_SEEDS {
        let cfg = m0_config(seed, EdgeFailsafeParams::enabled_default(), DURATION_S);
        let mut reference = EpisodeMachine::new(cfg.clone());
        let mut suspended = EpisodeMachine::new(cfg);

        // Run both halfway, then round-trip one through JSON.
        let halfway = 20_000u64; // 2.5 s of world ticks
        for _ in 0..halfway {
            if reference.done() {
                break;
            }
            reference.step();
            suspended.step();
        }
        let json = serde_json::to_string(&suspended).unwrap();
        let mut resumed: EpisodeMachine = serde_json::from_str(&json).unwrap();
        assert_eq!(resumed, suspended, "seed {seed}: roundtrip changed state");

        while !reference.done() {
            reference.step();
        }
        while !resumed.done() {
            resumed.step();
        }
        let a = reference.finish();
        let b = resumed.finish();
        assert_eq!(a, b, "seed {seed}: resumed run diverged");
        assert_eq!(a.log_hash(), b.log_hash());
    }
}

#[test]
fn episode_identity_is_stable_and_seed_sensitive() {
    let c1 = m0_config(7, EdgeFailsafeParams::enabled_default(), DURATION_S);
    let c2 = m0_config(7, EdgeFailsafeParams::enabled_default(), DURATION_S);
    let c3 = m0_config(8, EdgeFailsafeParams::enabled_default(), DURATION_S);
    let c4 = m0_config(7, EdgeFailsafeParams::disabled(), DURATION_S);
    assert_eq!(c1.identity(), c2.identity());
    assert_ne!(c1.identity(), c3.identity());
    assert_ne!(c1.identity(), c4.identity());
}
