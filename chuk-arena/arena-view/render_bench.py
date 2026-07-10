#!/usr/bin/env python3
"""arena-view — build the interactive bench console from the wasm build.

Usage:
    cargo build --release --target wasm32-unknown-unknown -p arena-wasm
    python3 render_bench.py -o bench.html \
        [--wasm ../target/wasm32-unknown-unknown/release/arena_wasm.wasm]

Base64-embeds the wasm module (the real arena-plant/arena-cells/arena-bench
crates) into bench-template.html so the output is a single self-contained file.
"""

import argparse
import base64
import pathlib
import sys

PLACEHOLDER = '//__WASM__\n"";'


def main():
    here = pathlib.Path(__file__).parent
    default_wasm = here.parent / "target/wasm32-unknown-unknown/release/arena_wasm.wasm"
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("-o", "--out", default="bench.html")
    ap.add_argument("--wasm", default=str(default_wasm))
    args = ap.parse_args()

    template = (here / "bench-template.html").read_text()
    if PLACEHOLDER not in template:
        sys.exit("bench-template.html is missing the //__WASM__ placeholder")
    wasm = pathlib.Path(args.wasm).read_bytes()
    b64 = base64.b64encode(wasm).decode()
    pathlib.Path(args.out).write_text(template.replace(PLACEHOLDER, f'"{b64}";'))
    print(f"{args.out}: wasm {len(wasm)} bytes -> {pathlib.Path(args.out).stat().st_size} bytes total")


if __name__ == "__main__":
    main()
