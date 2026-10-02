import argparse
import gc
import random
import statistics
import tempfile
from pathlib import Path
from time import perf_counter

from schemora import MinecraftData, Schematic, block, mob

COLORS = (
    "white orange magenta light_blue yellow lime pink gray light_gray cyan "
    "purple blue brown green red black"
).split()
PALETTE = tuple(
    block(name)
    for name in [
        "stone",
        "granite",
        "diorite",
        "andesite",
        "cobblestone",
        "stone_bricks",
        "bricks",
        "obsidian",
        "gold_block",
        "iron_block",
        "diamond_block",
        "emerald_block",
        "oak_planks",
        "birch_planks",
        "spruce_planks",
        "dark_oak_planks",
        *(f"{color}_concrete" for color in COLORS),
    ]
)
MOBS = tuple(mob(name) for name in ("pig", "cow", "sheep", "chicken", "zombie", "skeleton"))
STAGES = (
    "Prepare inputs",
    "Author scene",
    "Validate",
    "Save",
    "Load",
    "Sprite render",
    "3D render",
)


def prepare(args):
    rng = random.Random(args.seed)
    batches = []
    batch = []
    for x in range(args.side):
        for y in range(args.side):
            for z in range(args.side):
                batch.append(((x, y, z), rng.choice(PALETTE)))
                if len(batch) == 16_384:
                    batches.append(batch)
                    batch = []
    if batch:
        batches.append(batch)
    entities = [
        (
            rng.choice(MOBS),
            (rng.random() * args.side, float(args.side), rng.random() * args.side),
        )
        for _ in range(args.entities)
    ]
    return batches, entities


def author(args, data, batches, entities):
    scene = Schematic.create(version=args.version, data=data)
    region = scene.region()
    for batch in batches:
        region.set_many(batch)
    for value, position in entities:
        region.entities.add(value, at=position)
    return scene


def timed(results, stage, operation):
    start = perf_counter()
    value = operation()
    results[stage] = perf_counter() - start
    print(f"  {stage:<14} {results[stage]:9.3f} s", flush=True)
    return value


def print_table(runs):
    headers = ["Stage", *(f"Run {i + 1}" for i in range(len(runs))), "Median", "Min", "Max"]
    rows = []
    for stage in (*STAGES, "Schemora total"):
        values = [
            sum(run[name] for name in STAGES if name != "Prepare inputs")
            if stage == "Schemora total"
            else run[stage]
            for run in runs
        ]
        rows.append(
            [stage]
            + [f"{value:.3f}" for value in values]
            + [f"{statistics.median(values):.3f}", f"{min(values):.3f}", f"{max(values):.3f}"]
        )
    widths = [max(len(row[i]) for row in [headers, *rows]) for i in range(len(headers))]
    print("\nWall-clock seconds (Schemora total excludes input preparation):")
    for row in [headers, *rows]:
        print(" | ".join(value.ljust(widths[i]) for i, value in enumerate(row)))


def main():
    parser = argparse.ArgumentParser(description="Benchmark a random solid cube and free entities.")
    parser.add_argument("--side", type=int, default=100, help="Cube side length (default: 100).")
    parser.add_argument("--entities", type=int, default=1000)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--version", default="1.21.1")
    parser.add_argument("--cell-size", type=int, default=16)
    parser.add_argument("--cache-dir", type=Path)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument(
        "--output", type=Path, default=Path(tempfile.gettempdir()) / "schemora-benchmark"
    )
    args = parser.parse_args()
    if not 1 <= args.side <= 256 or args.entities < 0 or args.runs < 1:
        parser.error("side must be 1..256, entities nonnegative, and runs positive")
    if not 1 <= args.cell_size <= 128 or args.side * args.cell_size > 4096:
        parser.error("cell-size must be 1..128 and side * cell-size must be <= 4096")
    args.output.mkdir(parents=True, exist_ok=True)
    data = MinecraftData(cache_dir=args.cache_dir, offline=args.offline)
    pixels = args.side * args.cell_size
    schematic = args.output / "scene.litematic"
    sprite_image = args.output / "sprites.png"
    geometry_image = args.output / "geometry.png"
    print(
        f"{args.side}³ = {args.side**3:,} blocks, {args.entities:,} entities; "
        f"{args.runs} runs, seed {args.seed}, Minecraft {args.version}\n"
        f"32 solid block types; entities on top of cube; full-volume top views at {pixels}².\n"
        "Preparing catalog and renderer caches (excluded from measurements)...",
        flush=True,
    )
    start = perf_counter()
    data.load(args.version)
    data.load_visuals(args.version)
    warmup = Schematic.create(version=args.version, data=data)
    warmup.region().set((0, 0, 0), block("stone"))
    warmup.region().entities.add(MOBS[0], at=(0.5, 1.0, 0.5))
    with tempfile.TemporaryDirectory(prefix="schemora-benchmark-warmup-") as directory:
        warmup.export_sprites(Path(directory) / "sprites.png")
        warmup.export_png(Path(directory) / "geometry.png", size=(32, 32), view="top")
    del warmup
    print(f"Setup: {perf_counter() - start:.3f} s", flush=True)
    runs = []
    for index in range(args.runs):
        gc.collect()
        results = {}
        print(f"\nRun {index + 1}/{args.runs}", flush=True)
        batches, entities = timed(results, "Prepare inputs", lambda: prepare(args))
        gc.collect()
        scene = timed(
            results,
            "Author scene",
            lambda batches=batches, entities=entities: author(args, data, batches, entities),
        )
        del batches, entities
        report = timed(results, "Validate", scene.validate)
        print(
            f"  Validation: {len(report.errors)} errors, {len(report.warnings)} warnings, "
            f"{len(report.unknown)} unknown"
        )
        if not report.ok:
            raise RuntimeError(f"Generated scene failed validation: {report.issues[:5]}")
        timed(results, "Save", lambda scene=scene: scene.save(schematic))
        del scene
        loaded = timed(results, "Load", lambda: Schematic.load(schematic, data=data))
        sprite_diagnostics = timed(
            results,
            "Sprite render",
            lambda loaded=loaded: loaded.export_sprites(
                sprite_image, cell_size=args.cell_size, view="top"
            ),
        )
        geometry_diagnostics = timed(
            results,
            "3D render",
            lambda loaded=loaded: loaded.export_png(
                geometry_image, size=(pixels, pixels), view="top"
            ),
        )
        region = loaded.region()
        counts = region.select(start=(0, 0, 0), size=(args.side,) * 3).counts()
        if (
            sum(count for state, count in counts.items() if state != "minecraft:air")
            != args.side**3
            or len(tuple(region.entities)) != args.entities
        ):
            raise RuntimeError("Loaded block/entity counts differ from generated scene")
        print(
            f"  Round-trip counts OK; file {schematic.stat().st_size / 1024**2:.2f} MiB; "
            f"render diagnostics: {len(sprite_diagnostics)} sprite, {len(geometry_diagnostics)} 3D"
        )
        for diagnostic in sorted(set(sprite_diagnostics + geometry_diagnostics)):
            print(f"    {diagnostic}")
        runs.append(results)
        del loaded, region
    print_table(runs)
    print(f"\nLatest scene and PNGs: {args.output.resolve()}")
    print(
        "Save/load include filesystem I/O with a warm OS cache; no fsync or cold-disk simulation."
    )
    print("Prepare inputs: random sampling and placement lists; excluded from Schemora total.")
    print("Author scene: scene creation, set_many batches of 16,384, and entity additions.")
    print("Authoring includes the public Python API conversion and Rust insertion costs.")


if __name__ == "__main__":
    main()
