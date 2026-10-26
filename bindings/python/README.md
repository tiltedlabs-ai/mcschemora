# MCSchemora for Python

Rust-based programmatic tools for editing, converting, and rendering Minecraft schematics.

## Install

```sh
python -m pip install mcschemora
```

## Install from source

With [uv](https://docs.astral.sh/uv/) and a stable [Rust toolchain](https://rustup.rs/),
run from the repository root:

```sh
uv sync --all-packages
uv run --all-packages python examples/build.py
```

After changing Rust code, run `make rebuild` to refresh the native extension.

## Build a wheel

From the repository root:

```sh
uv run --all-packages maturin build --release --locked --manifest-path bindings/python/Cargo.toml --out dist
```

Install the generated wheel from `dist/` with `python -m pip install <wheel-path>`
in the destination environment.

## Usage

```python
from mcschemora import Schematic, block

schematic = Schematic.create(version="1.21.1")
floor = schematic.region().select(start=(0, 0, 0), size=(7, 1, 7))
floor.fill(block("stone_bricks"))
print(schematic.validate())
schematic.save("floor.schem")
```

## Inspect submissions

```python
from mcschemora import inspect_schematics

resources = inspect_schematics(
    ["farm.schem", "storage.schem"],
    initial_items=[("minecraft:stone", 64)],
    max_bytes=16 * 1024 * 1024,
    max_unpacked_bytes=64 * 1024 * 1024,
    max_volume=2_000_000,
    max_entities=1000,
)
print(resources["blocks"], resources["items"], resources["entities"])
print(resources["placements"][0]["inventories"])
```

Inspection reads Sponge v2/v3 files without loading Minecraft catalogs or using
the network. File reading, bounded NBT decoding, traversal, resource counts, and
aggregation all run in Rust. Python only converts the returned JSON to a dict.
Byte limits apply per file; volume and entity limits also apply across placements.
Malformed structures and arithmetic overflow raise `ValueError`.

Each placement reports format/data versions, dimensions, volume, block/item/entity
counts, inventory paths, embedded identifiers, nonempty payload field names,
extra sections, entity motion, and the largest item stack. Block counts include
unused palette IDs with zero counts, so callers can filter every declared block.
Passengers, equipment, and nested containers are counted; nested contents are
multiplied by their containing stack count. `initial_items` contributes only to
the combined item totals.

These are inspection facts, not an admission policy. Callers must apply their own
forbidden IDs, payload restrictions, and stack limits. Unknown extra sections are
reported but their contents are not included in resource totals. Counts use the
submitted IDs without version conversion; inspection does not calculate crafting
costs or infer resources created by privileged payloads.

## Documentation

- [Usage guide](https://github.com/whuang37/schemora/blob/master/docs/guides/usage.md) · [Examples and outputs](https://github.com/whuang37/schemora/blob/master/examples/README.md).
- [API reference](https://github.com/whuang37/schemora/blob/master/docs/reference/api.md#python) · [Catalogs and offline preparation](https://github.com/whuang37/schemora/blob/master/docs/reference/minecraft-data.md).
- [Release workflow](https://github.com/whuang37/schemora/blob/master/.github/workflows/python-release.yml).

## Licensing

Project code is MIT licensed. Bundled geometry and sprites have separate terms;
each installed package includes notices and attribution under `mcschemora/licenses/`.
MCSchemora is an independent project and is not affiliated with Mojang or Microsoft.
