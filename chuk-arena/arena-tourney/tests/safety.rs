//! Edge bench acceptance shape (SPEC §4.5): with the failsafe kernel on,
//! edge-loss rate must be ZERO across the swept band — any loss is a
//! counterexample, not a statistic.

use arena_cells::EdgeFailsafeParams;
use arena_store::Outcome;
use arena_tourney::{m0_config, run_episode};

#[test]
fn kernel_on_zero_edge_losses() {
    let mut losses = Vec::new();
    for seed in 0..100u64 {
        let log = run_episode(m0_config(
            seed,
            EdgeFailsafeParams::enabled_default(),
            30.0,
        ));
        if let Outcome::EdgeOut { t } = log.result.outcome {
            losses.push((seed, t));
        }
        // The invariant is stronger than "survived": the CoG never gets
        // closer to the edge than the certified reach.
        assert!(
            log.result.min_edge_distance >= 0.0,
            "seed {seed}: min edge distance {} < 0",
            log.result.min_edge_distance
        );
    }
    assert!(
        losses.is_empty(),
        "counterexample seeds (seed, t_cross): {losses:?}"
    );
}

#[test]
fn kernel_off_ablation_shows_pressure() {
    // The ablation is only meaningful if the intent stream actually produces
    // edge losses without the kernel. This guards against a silently tame
    // driver model making the zero-loss claim vacuous.
    let mut losses = 0;
    for seed in 0..50u64 {
        let log = run_episode(m0_config(seed, EdgeFailsafeParams::disabled(), 30.0));
        if matches!(log.result.outcome, Outcome::EdgeOut { .. }) {
            losses += 1;
        }
    }
    assert!(
        losses > 0,
        "driver model produced no edge pressure — ablation would be vacuous"
    );
}
