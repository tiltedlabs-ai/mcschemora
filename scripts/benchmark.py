import argparse
import gc
import random
import statistics
import tempfile
from itertools import product
from pathlib import Path
from time import perf_counter

from mcschemora import MinecraftData, Schematic, block, mob

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
READ_STAGES = ("Get loop", "Get all", "Part get loop", "Part get all")
STAGES = (
    "Prepare inputs",
    "Author schematic",
    "Validate",
    "Save",
    "Load",
    *READ_STAGES,
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
                if args.density < 1 and rng.random() >= args.density:
                    continue
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
    schematic = Schematic.create(version=args.version, data=data)
    region = schematic.region()
    for batch in batches:
        region.set_many(batch)
    for value, position in entities:
        region.entities.add(value, at=position)
    return schematic


def timed(results, stage, operation):
    start = perf_counter()
    value = operation()
    results[stage] = perf_counter() - start
    print(f"  {stage:<14} {results[stage]:9.3f} s", flush=True)
    return value


def read_loop(region, bounds):
    result = {}
    ranges = [
        range(start, start + size) for start, size in zip(bounds.start, bounds.size, strict=True)
    ]
    for position in product(*ranges):
        value = region.get(position)
        if value.id not in {"minecraft:air", "minecraft:cave_air", "minecraft:void_air"}:
            result[position] = value
    return result


def measure_reads(schematic, args, results, index):
    region = schematic.region()
    start = (args.side // 4,) * 3
    size = (max(1, args.side // 2),) * 3
    part = region.select(start=start, size=size)
    pairs = (
        ("Get loop", "Get all", region.bounds, region.get_all),
        ("Part get loop", "Part get all", part.bounds, part.get_all),
    )
    for loop_stage, bulk_stage, bounds, bulk_operation in pairs:
        operations = [
            (loop_stage, lambda bounds=bounds: read_loop(region, bounds)),
            (bulk_stage, bulk_operation),
        ]
        if index % 2:
            operations.reverse()
        snapshots = {}
        for stage, operation in operations:
            gc.collect()
            snapshots[stage] = timed(results, stage, operation)
        if snapshots[loop_stage] != snapshots[bulk_stage]:
            raise RuntimeError(f"{bulk_stage} differs from the get loop")
        print(
            f"  {bulk_stage}: {len(snapshots[bulk_stage]):,} blocks; "
            f"{results[loop_stage] / results[bulk_stage]:.2f}x faster",
            flush=True,
        )


def print_table(runs, stages=STAGES):
    headers = ["Stage", *(f"Run {i + 1}" for i in range(len(runs))), "Median", "Min", "Max"]
    rows = []
    for stage in (*stages, "MCSchemora total"):
        values = [
            sum(run[name] for name in stages if name != "Prepare inputs")
            if stage == "MCSchemora total"
            else run[stage]
            for run in runs
        ]
        rows.append(
            [stage]
            + [f"{value:.6f}" for value in values]
            + [f"{statistics.median(values):.6f}", f"{min(values):.6f}", f"{max(values):.6f}"]
        )
    widths = [max(len(row[i]) for row in [headers, *rows]) for i in range(len(headers))]
    print("\nWall-clock seconds (MCSchemora total excludes input preparation):")
    for row in [headers, *rows]:
        print(" | ".join(value.ljust(widths[i]) for i, value in enumerate(row)))


def main():
    parser = argparse.ArgumentParser(description="Benchmark a random solid cube and entities.")
    parser.add_argument("--side", type=int, default=100, help="Cube side length (default: 100).")
    parser.add_argument(
        "--reads-only", action="store_true", help="Skip validation, I/O, and rendering."
    )
    parser.add_argument("--density", type=float, default=1.0, help="Occupied fraction (0..1).")
    parser.add_argument("--entities", type=int, default=1000)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--version", default="1.21.1")
    parser.add_argument("--cell-size", type=int, default=16)
    parser.add_argument("--cache-dir", type=Path)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument(
        "--output", type=Path, default=Path(tempfile.gettempdir()) / "mcschemora-benchmark"
    )
    args = parser.parse_args()
    if not 1 <= args.side <= 256 or args.entities < 0 or args.runs < 1:
        parser.error("side must be 1..256, entities nonnegative, and runs positive")
    if not 0 < args.density <= 1:
        parser.error("density must be in (0, 1]")
    if args.density < 1 and not args.reads_only:
        parser.error("density below 1 requires --reads-only")
    if not 1 <= args.cell_size <= 128 or args.side * args.cell_size > 4096:
        parser.error("cell-size must be 1..128 and side * cell-size must be <= 4096")
    args.output.mkdir(parents=True, exist_ok=True)
    data = MinecraftData(cache_dir=args.cache_dir, offline=args.offline)
    pixels = args.side * args.cell_size
    schematic_path = args.output / "schematic.litematic"
    sprite_image = args.output / "sprites.png"
    geometry_image = args.output / "geometry.png"
    print(
        f"{args.side}³ = {args.side**3:,} possible blocks, density {args.density:g}, {args.entities:,} entities; "
        f"{args.runs} runs, seed {args.seed}, Minecraft {args.version}\n"
        f"32 solid block types; entities on top of cube; full-volume top views at {pixels}².\n"
        "Preparing catalog and renderer caches (excluded from measurements)...",
        flush=True,
    )
    start = perf_counter()
    data.load(args.version)
    if not args.reads_only:
        data.load_visuals(args.version)
    warmup = Schematic.create(version=args.version, data=data)
    warmup.region().set((0, 0, 0), block("stone"))
    warmup.region().entities.add(MOBS[0], at=(0.5, 1.0, 0.5))
    warmup.region().get_all()
    warmup.region().select(start=(0, 0, 0), size=(1, 1, 1)).get_all()
    if not args.reads_only:
        with tempfile.TemporaryDirectory(prefix="mcschemora-benchmark-warmup-") as directory:
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
        schematic = timed(
            results,
            "Author schematic",
            lambda batches=batches, entities=entities: author(args, data, batches, entities),
        )
        del batches, entities
        if args.reads_only:
            measure_reads(schematic, args, results, index)
            runs.append(results)
            del schematic
            continue
        report = timed(results, "Validate", schematic.validate)
        print(
            f"  Validation: {len(report.errors)} errors, {len(report.warnings)} warnings, "
            f"{len(report.unknown)} unknown"
        )
        if not report.ok:
            raise RuntimeError(f"Generated schematic failed validation: {report.issues[:5]}")
        timed(results, "Save", lambda schematic=schematic: schematic.save(schematic_path))
        del schematic
        loaded = timed(results, "Load", lambda: Schematic.load(schematic_path, data=data))
        measure_reads(loaded, args, results, index)
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
            raise RuntimeError("Loaded block/entity counts differ from generated schematic")
        print(
            f"  Round-trip counts OK; file {schematic_path.stat().st_size / 1024**2:.2f} MiB; "
            f"render diagnostics: {len(sprite_diagnostics)} sprite, {len(geometry_diagnostics)} 3D"
        )
        for diagnostic in sorted(set(sprite_diagnostics + geometry_diagnostics)):
            print(f"    {diagnostic}")
        runs.append(results)
        del loaded, region
    if args.reads_only:
        print_table(runs, ("Prepare inputs", "Author schematic", *READ_STAGES))
        print("Reads include Python dictionary construction; equality checks are untimed.")
        print("Read order alternates between runs; selections cover the central half on each axis.")
        return
    print_table(runs)
    print(f"\nLatest schematic and PNGs: {args.output.resolve()}")
    print(
        "Save/load include filesystem I/O with a warm OS cache; no fsync or cold-disk simulation."
    )
    print("Prepare inputs: random sampling and placement lists; excluded from MCSchemora total.")
    print("Author schematic: schematic creation, set_many batches of 16,384, and entity additions.")
    print("Authoring includes the public Python API conversion and Rust insertion costs.")


if __name__ == "__main__":
    main()
