# chuk-arena — the proving ground

Deterministic virtual physics test environment for the robot programme.
Full design: [../specs/chuk-arena.md](../specs/chuk-arena.md). Status: **M0
done; M1 done except §4.5 edge-bench formalization on the dynamic plant**
(spec §10). §2.2 kill criterion: full §2.3 table evaluated — see
`arena diff` for the current verdict.

## Layout

Cargo workspace mirroring SPEC §2. Crates land at their milestone; absent
crates (`arena-events`, `arena-sense`) are M1/M2 scope — adding them early is
scope creep per SPEC §11.3.

| crate | SPEC layer | M0 scope |
|---|---|---|
| `arena-core` | §2 arena-core | 8kHz world / 1kHz control clock, owned xoshiro256++ PRNG with domain-separated substreams, square edge-out geometry |
| `arena-plant` | §3 | M0 kinematic plant (frozen — banked corpus) + M1 dynamic plant: per-wheel friction circles with longitudinal priority, DC motor curves with back-EMF braking, battery sag, rolling resistance |
| `arena-agents` | §5.1 | humanlike driver: latency queue, aim noise, waypoint chase, full-stick-toward-edge blunders sampled from the seed |
| `arena-cells` | §7 | **native placeholder** kernels (provisional per §7 until the executor differential job stands): M0 edge failsafe (coast brake) + M1 aligned active-brake cell with μ-band/sag/yaw-rate certification |
| `arena-bench` | §4 | envelope bench (§4.2): certified vs achieved stopping distance sweeps, negative margin = build-blocking finding; dyno bench (§4.1): top speed, 0→v, stopping distances, stall push force, battery sag |
| `arena-store` | §9/arena-store | episode schema, sha256 episode identity, layer version tags in every record |
| `arena-tourney` | §8 | serializable `EpisodeMachine`, failsafe-ablation experiment with Wilson CIs and corpus hash |
| `arena-cli` | — | `arena run / fuzz / ablate` |
| `arena-diff` | §2.2/§2.3 | Rapier differential adversary: owned integrator vs rapier2d-f64 vs analytic closed forms on the §2.3 scenario table; kill-criterion verdict in every report |
| `arena-wasm` | §7 (Q3 lean) | the real plant/cells/bench crates compiled to wasm32 for the browser bench console — fast-mode analog, same source as native |
| `arena-view/` | §2 arena-view | offline HTML tooling (not a crate): `render.py` splices episode logs into the replayer `template.html`; `render_bench.py` embeds the wasm build into `bench-template.html` for the interactive bench console |

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
arena bench envelope            # §4.2 conservatism report, both brake kernels
arena bench dyno                # §4.1 speed/thrust/braking/sag table
arena diff                      # §2.3 Rapier differential rig + kill-criterion verdict

# interactive bench console (tweak scenario + design vector live in the browser)
cargo build --release --target wasm32-unknown-unknown -p arena-wasm
python3 arena-view/render_bench.py -o bench.html
```

`arena replay` runs both arms of a seed (failsafe on + off), splices them into
the embedded `arena-view/template.html`, writes `replay-seed<N>.html`, and opens
it (`--out F` / `--no-open` to override). For replaying arbitrary exported logs
(`arena run --out ep.json`), use `python3 arena-view/render.py -o replay.html ep.json …`.
