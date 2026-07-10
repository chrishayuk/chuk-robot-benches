//! Fresh-process leg of the determinism fuzz (SPEC §2.1): two separate OS
//! processes running the same episode must emit byte-identical output.

use std::process::Command;

fn run_once(seed: &str) -> Vec<u8> {
    let out = Command::new(env!("CARGO_BIN_EXE_arena"))
        .args(["run", "--seed", seed, "--duration", "20"])
        .output()
        .expect("arena binary runs");
    assert!(out.status.success(), "arena exited nonzero: {:?}", out);
    out.stdout
}

#[test]
fn fresh_process_output_is_byte_identical() {
    for seed in ["0", "7", "42"] {
        let a = run_once(seed);
        let b = run_once(seed);
        assert_eq!(a, b, "seed {seed}: fresh-process outputs differ");
    }
}
