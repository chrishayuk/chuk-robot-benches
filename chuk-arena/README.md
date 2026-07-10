# chuk-arena — the proving ground

Deterministic virtual physics test environment for the robot programme.
Full design: [SPEC.md](SPEC.md). Status: **M0 in progress** (SPEC §10).

## Layout

Cargo workspace mirroring SPEC §2. Crates land at their milestone; absent
crates (`arena-events`, `arena-sense`) are M1/M2 scope — adding them early is
scope creep per SPEC §11.3.

| crate | SPEC layer | M0 scope |
|---|---|---|
| `arena-core` | §2 arena-core | 8kHz world / 1kHz control clock, owned xoshiro256++ PRNG with domain-separated substreams, square edge-out geometry |
| `arena-plant` | §3 | kinematic differential-drive plant, traction-circle accel limit, `BotSpec` design vector (M0 subset, datasheet-provisional baseline) |
| `arena-agents` | §5.1 | humanlike driver: latency queue, aim noise, waypoint chase, full-stick-toward-edge blunders sampled from the seed |
| `arena-cells` | §7 | **native placeholder** edge-failsafe kernel (fast-mode precursor; provisional per §7 until the executor differential job stands) |
| `arena-store` | §9/arena-store | episode schema, sha256 episode identity, layer version tags in every record |
| `arena-tourney` | §8 | serializable `EpisodeMachine`, failsafe-ablation experiment with Wilson CIs and corpus hash |
| `arena-cli` | — | `arena run / fuzz / ablate` |
| `arena-view/` | §2 arena-view | offline HTML replayer (not a crate): `render.py` splices episode logs into `template.html` — counterfactual-ghost view when given both arms of one seed |

## Determinism (SPEC §2.1)

All three fuzz legs are enforced in CI-shape tests:

- **rerun** — same config run twice, bit-identical logs (`arena-tourney/tests/determinism.rs`)
- **serialize-roundtrip** — episode suspended mid-flight, JSON round-tripped,
  resumed; final log bit-identical (requires serde_json `float_roundtrip` —
  see workspace `Cargo.toml`)
- **fresh-process** — two OS processes, byte-identical stdout (`arena-cli/tests/fresh_process.rs`)

## Usage

```sh
cargo test --release            # includes determinism fuzz + zero-loss safety property
arena run --seed 42             # one episode, prints identity/log hash/result
arena fuzz --seeds 16           # determinism fuzz, in-process legs
arena ablate --n 500 --seed 1   # M0 failsafe ablation report (JSON)
arena replay --seed 42          # render both arms to HTML and open in browser
```

`arena replay` runs both arms of a seed (failsafe on + off), splices them into
the embedded `arena-view/template.html`, writes `replay-seed<N>.html`, and opens
it (`--out F` / `--no-open` to override). For replaying arbitrary exported logs
(`arena run --out ep.json`), use `python3 arena-view/render.py -o replay.html ep.json …`.
