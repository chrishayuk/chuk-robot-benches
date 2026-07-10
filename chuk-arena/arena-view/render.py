#!/usr/bin/env python3
"""arena-view — render episode logs into a self-contained HTML replay.

Reads full EpisodeLog JSON files (produced by `arena run --out <file>`),
compacts them (positions to 0.1 mm, timeline to 10 ms — replay precision,
not the bit-exact record), and splices them into template.html.

Usage:
    arena run --seed 42 --duration 45 --out ep_on.json
    arena run --seed 42 --duration 45 --no-kernel --out ep_off.json
    python3 render.py -o replay.html ep_on.json ep_off.json

Pass both arms of the same seed to get the counterfactual-ghost view.
Zero coupling to the sim per SPEC §2: this consumes episode logs only.
"""

import argparse
import json
import pathlib
import sys

PLACEHOLDER = "//__DATA__\n[];"


def r(x, places):
    return round(x, places)


def compact(log):
    cfg, res = log["config"], log["result"]
    return {
        "seed": cfg["seed"],
        "kernel": cfg["kernel"]["enabled"],
        "arena": cfg["arena"]["half_extent"],
        "bot": {
            "w": cfg["bot"]["footprint_half_w"],
            "l": cfg["bot"]["footprint_half_l"],
        },
        "mu": r(res["mu"], 3),
        "driver": {
            "lat": r(res["driver"]["reaction_latency_s"], 3),
            "agg": r(res["driver"]["aggression"], 2),
        },
        "outcome": res["outcome"],
        "interventions": res["interventions"],
        "minEdge": r(res["min_edge_distance"], 4),
        "events": [{"k": e["kind"], "t": r(e["t"], 3)} for e in log["events"]],
        "samples": [
            [r(s["t"], 2), r(s["x"], 4), r(s["y"], 4), r(s["heading"], 3), r(s["v"], 3)]
            for s in log["samples"]
        ],
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("logs", nargs="+", help="EpisodeLog JSON files")
    ap.add_argument("-o", "--out", default="replay.html", help="output HTML path")
    args = ap.parse_args()

    template = (pathlib.Path(__file__).parent / "template.html").read_text()
    if PLACEHOLDER not in template:
        sys.exit("template.html is missing the //__DATA__ placeholder")

    episodes = [compact(json.loads(pathlib.Path(p).read_text())) for p in args.logs]
    blob = json.dumps(episodes, separators=(",", ":"))
    pathlib.Path(args.out).write_text(template.replace(PLACEHOLDER, blob + ";"))
    print(f"{args.out}: {len(episodes)} episodes, {pathlib.Path(args.out).stat().st_size} bytes")


if __name__ == "__main__":
    main()
