"""Check every Java catalog state; run with --transforms-only after transform edits."""

import argparse
import itertools
import json

from schemora import MinecraftData, Schematic, block, water_source

DIRECTIONS = ("north", "south", "east", "west")
# Expected direction changes, independent of the Rust matrix implementation.
TURNS = {
    "rotate": dict(zip(DIRECTIONS, ("west", "east", "north", "south"), strict=True)),
    "x": dict(zip(DIRECTIONS, ("north", "south", "west", "east"), strict=True)),
    "z": dict(zip(DIRECTIONS, ("south", "north", "east", "west"), strict=True)),
}


def position(index):
    return index % 64, index // 4096, (index // 64) % 64


def catalog_states(scene, entries):
    states = []
    for entry in entries:
        names = [s["name"] for s in entry["states"]]
        choices = [
            s.get("values")
            or (
                ["true", "false"] if s["type"] == "bool" else list(map(str, range(s["num_values"])))
            )
            for s in entry["states"]
        ]
        variants = [
            block(entry["name"], **dict(zip(names, values, strict=True)))
            for values in itertools.product(*choices)
        ]
        assert len(variants) == entry["maxStateId"] - entry["minStateId"] + 1, entry["name"]
        default = variants[entry["defaultState"] - entry["minStateId"]]
        schema = scene.registry.describe(entry["name"])["properties"]
        assert {k: p["values"] for k, p in schema.items()} == dict(zip(names, choices, strict=True))
        assert {k: p["default"] for k, p in schema.items()} == dict(default.states)
        states.extend(variants)
    return states


def transformed(value, operation):
    directions = TURNS[operation]
    properties = {}
    for key, val in value.states.items():
        if key in DIRECTIONS:
            key = directions[key]
        elif key == "facing":
            val = directions.get(val, val)
        elif key == "orientation":
            val = "_".join(directions.get(part, part) for part in val.split("_"))
        elif key == "axis" and operation == "rotate":
            val = {"x": "z", "z": "x"}.get(val, val)
        elif key == "rotation":
            rotation = {"rotate": int(val) - 4, "x": -int(val), "z": 8 - int(val)}[operation]
            val = str(rotation % 16)
        elif key == "shape" and value.id.endswith("rail"):
            if val.startswith("ascending_"):
                val = "ascending_" + directions[val.removeprefix("ascending_")]
            else:
                val = "_".join(
                    sorted((directions[p] for p in val.split("_")), key=DIRECTIONS.index)
                )
        elif key in ("shape", "hinge", "type", "side_chain") and operation != "rotate":
            val = {
                "left": "right",
                "right": "left",
                "inner_left": "inner_right",
                "inner_right": "inner_left",
                "outer_left": "outer_right",
                "outer_right": "outer_left",
            }.get(val, val)
        properties[key] = val
    return block(value.id, **properties)


def check_transforms(region, states):
    area = region.select(start=region.bounds.start, size=region.bounds.size)
    expected = [(position(i), value) for i, value in enumerate(states)]
    for operation in TURNS:
        if operation == "rotate":
            area.rotate(pivot=(0.5, 0.5, 0.5))
        else:
            area.flip(axis=operation, center=0.5)
        updated = []
        for (x, y, z), value in expected:
            at = {"rotate": (z, y, -x), "x": (-x, y, z), "z": (x, y, -z)}[operation]
            value = transformed(value, operation)
            assert region.get(at) == value, (operation, at, value, region.get(at))
            updated.append((at, value))
        expected = updated


def rejected(region, value):
    before = region.get((0, 0, 0))
    try:
        region.set((0, 0, 0), value)
    except ValueError:
        assert region.get((0, 0, 0)) == before
    else:
        raise AssertionError(f"Invalid block accepted: {value}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("versions", nargs="*", help="Default: every advertised Java catalog")
    parser.add_argument("--cache-dir", help="Runtime download cache directory")
    parser.add_argument("--offline", action="store_true", help="Use only already cached data")
    parser.add_argument("--transforms-only", action="store_true", help="Skip file round-trips")
    args = parser.parse_args()
    source = MinecraftData(args.cache_dir, offline=args.offline)
    versions = args.versions or source.versions
    checked_datasets = set()
    total = 0
    for version in versions:
        scene = Schematic.create(version=version, data=source)
        entries = json.loads(source.dataset_path(version, "blocks").read_text())
        states = catalog_states(scene, entries)
        region = scene.region()
        region.set_many([(position(i), value) for i, value in enumerate(states)])
        for i, value in enumerate(states):
            assert region.get(position(i)) == value, (version, i, value)
        if not args.transforms_only:
            for fmt in ("schem", "litematic", "nbt", "snbt"):
                loaded = Schematic.from_bytes(scene.to_bytes(format=fmt), format=fmt, data=source)
                assert loaded.data_version == scene.data_version, (version, fmt)
                restored = loaded.region()
                for i, value in enumerate(states):
                    assert restored.get(position(i)) == value, (version, fmt, i, value)
        dataset = source.dataset_path(version, "blocks")
        if dataset not in checked_datasets:
            check_transforms(region, states)
            checked_datasets.add(dataset)
        for invalid in (
            block("water", level=16),
            block("lava", level=-1),
            block("redstone_wire", power=16),
        ):
            rejected(region, invalid)
        rejected(region, block("schemora:missing_block"))
        if not any(b["name"] == "crafter" for b in entries):
            rejected(region, block("crafter"))
        region.set((0, 0, 0), water_source())
        assert region.get((0, 0, 0)).states["level"] == "0"
        total += len(states)
        print(f"{version}: {len(entries)} blocks, {len(states)} states passed", flush=True)
    print(
        f"Passed: {len(versions)} versions, {total} state cases, {len(checked_datasets)} transform datasets."
    )


if __name__ == "__main__":
    main()
