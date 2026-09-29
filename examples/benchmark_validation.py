"""Benchmark random Java block types and states with a reproducible scene."""

import argparse
import hashlib
import json
from pathlib import Path
from random import Random
from statistics import median
from time import perf_counter

from schemora import Schematic, block

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--size", type=int, default=100, help="Cube side length (default: 1M cells)")
parser.add_argument("--runs", type=int, default=3)
parser.add_argument("--seed", type=int, default=20260929)
parser.add_argument("--input", type=Path, help="Reuse a saved benchmark scene")
parser.add_argument("--save", type=Path, help="Save and reload the generated scene")
args = parser.parse_args()
if args.size <= 0 or args.runs < 2:
    parser.error("size must be positive and runs must be at least two")

start = perf_counter()
if args.input:
    scene = Schematic.load(args.input)
else:
    rng = Random(args.seed)
    catalog = Path(__file__).resolve().parents[1] / "data/minecraft-data/data/pc/1.21.1/blocks.json"
    definitions = json.loads(catalog.read_text())
    variants = []
    for definition in definitions:
        for _ in range(4):
            states = {}
            for state in definition["states"]:
                if "values" in state:
                    value = rng.choice(state["values"])
                elif state["type"] == "bool":
                    value = rng.choice((False, True))
                else:
                    value = rng.randrange(state["num_values"])
                states[state["name"]] = value
            variants.append(block(definition["name"], **states))
    scene = Schematic.create(version="1.21.1")
    region = scene.region()
    # Bound Python's temporary placement list to one plane.
    for x in range(args.size):
        region.set_many(
            ((x, y, z), rng.choice(variants)) for y in range(args.size) for z in range(args.size)
        )
    print(
        f"Generated {args.size**3:,} cells from {len(definitions):,} block types; seed={args.seed}",
        flush=True,
    )
    if args.save:
        scene.save(args.save)
        scene = Schematic.load(args.save)
print(f"Scene preparation: {perf_counter() - start:.3f}s", flush=True)

times = []
expected = None
for run in range(args.runs):
    start = perf_counter()
    report = scene.validate()
    elapsed = perf_counter() - start
    times.append(elapsed)
    # Compare complete findings outside the timed section. Random placement is
    # intentionally invalid; success means stable findings, not an empty report.
    digest = hashlib.sha256()
    for level, messages in (
        ("error", report.errors),
        ("warning", report.warnings),
        ("unknown", report.unknown),
    ):
        for message in sorted(messages):
            digest.update(f"{level}\0{message}\n".encode())
    result = (len(report.errors), len(report.warnings), len(report.unknown), digest.hexdigest())
    if expected is not None:
        assert result == expected, "Validation findings changed between runs"
    expected = result
    print(f"Validation {run + 1}: {elapsed:.3f}s", flush=True)
    del report
print(f"Warm median: {median(times[1:]):.3f}s", flush=True)
print(f"Findings: errors={expected[0]}, warnings={expected[1]}, unknown={expected[2]}", flush=True)
print(f"Findings SHA-256: {expected[3]}", flush=True)
