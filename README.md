# Schemora

Author and edit Minecraft schematics in code.

## Get started

Install [uv](https://docs.astral.sh/uv/) and a current stable
[Rust toolchain](https://rustup.rs/). From this directory:

```sh
uv sync --all-packages
```


## Code Example
```python
from schemora import Schematic, bed, block, chest, item, mob, water_source

scene = Schematic.create(edition="java", version="1.21.1")
region = scene.region("main")

region.select(start=(0, 0, 0), size=(12, 1, 12)).fill(block("stone_bricks"))
region.set((1, 1, 1), water_source())
region.set(
    (3, 1, 1),
    block("lever", face="floor", facing="north", powered=False),
)
region.patch((3, 1, 1), powered=True)
region.place(bed(color="red", head_toward="north"), at=(2, 1, 4))
region.place(chest(items={0: item("stone", count=64)}), at=(6, 1, 4))
region.entities.add(mob("villager"), at=(9.5, 1.0, 7.5))

area = region.select(start=(0, 0, 0), size=(12, 4, 12))
copy = area.duplicate(offset=(16, 0, 0))
copy.rotate(axis="y", steps=1)
copy.flip(axis="x")
copy.move(offset=(0, 0, 2))

print(area.describe_layer(y=1))
print(scene.validate())
scene.save("build.schem")
```

## Minecraft versions and data

Authoring supports Java **1.13 and later**, when a catalog is present in
[minecraft-data](https://github.com/PrismarineJS/minecraft-data). Block properties,
defaults, items, entity IDs, and file `DataVersion` come from the selected version.

```python
from schemora import MinecraftData, Schematic, block

data = MinecraftData()
print(data.versions)
scene = Schematic.create(version="1.20.4", data=data)
scene.region().set((0, 0, 0), block("lever", powered=True))
print(scene.registry.describe("lever"))
```

Catalogs download on first use from a pinned
[minecraft-data snapshot](https://github.com/PrismarineJS/minecraft-data/tree/8ffb321c74cffe779acf5c447d08c473c4c291d7).

The default cache is `~/.cache/schemora` on Linux (or `$XDG_CACHE_HOME/schemora`),
`~/Library/Caches/schemora` on macOS, and `%LOCALAPPDATA%/schemora` on Windows.

```python
data = MinecraftData(cache_dir="./minecraft-cache")
data.fetch("1.21.1")
offline = MinecraftData(cache_dir=data.cache_dir, offline=True)
scene = Schematic.create(version="1.21.1", data=offline)
```

Visual bundles include block and entity textures and are downloaded on demand:

```python
data.fetch("1.21.1", visuals=True)
prepared = data.visuals("1.21.1")
print(prepared / "manifest.json")
print(prepared / "atlas-0.png")
```



## Rendering

```python
scene.export_glb("build.glb")
scene.export_png("build.png")
scene.export_png("layer.png", camera="top_down", grid=True, y=10)
```

Special block model data comes from [block-model-renderer](data/block-models/SOURCE.md). Update with
```
python scripts/import_special_models.py
```

## Interface

| Object | Operations |
| --- | --- |
| `Schematic` | `create`, `load`, `save`, `from_bytes`, `to_bytes`, `region`, `add_region`, `validate`, `check_export` |
| `Region` | `get`, `set`, `set_many`, `patch`, `select`, `place` |
| `Selection` | `select`, `fill`, `replace`, `patch`, `delete`, `move`, `rotate`, `flip`, `duplicate`, `copy`, `counts`, `describe_layer` |
| `region.entities` | `add`, `get`, `update`, `remove`, iteration over references |
| `region.block_entities` | `get`, `set`, `remove` |
| `scene.registry` | `describe(block_id)` |

Helpers: `block`, `water_source`, `bed`, `door`, `sign`, `chest`, `item`, and `mob`.
Placement helpers return typed recipes. Pass the immutable values from `item()`
to `chest(items=...)`; item values are tuples of ID, count, and optional SNBT components.
There is one generic block value; exact Minecraft state names remain available.

### Selections and transforms

Coordinates use Minecraft axes: +X east, +Y up, +Z south. Region coordinates are
local to its origin. Selection sizes exclude the upper boundary. Negative starts
are allowed; negative sizes are not.

```python
area = region.select(start=(0, 0, 0), size=(12, 4, 12))
area.select(block="lever").patch(powered=False)
area.select(block="repeater", states={"delay": 1}).patch(delay=2)
```

A selection refers to current content at its selected coordinates. Filters resolve
once. Successful moves, rotations, and flips update that selection in place.
Other selections keep their coordinates. There is no global active selection.

- Rotation uses quarter turns about Y. Positive turns follow the right-hand rule:
  north becomes west. `steps=-1` reverses the direction.
- The default pivot is the geometric center of the selection bounds.
  `pivot=(x, y, z)` supplies a pivot in region coordinates.
- `flip(axis="x")` or `flip(axis="z")` reflects about the bounds center.
  `center=number` supplies that axis's plane coordinate.
- Off-grid results are errors. There is no rounding or resampling.
- Self-overlap is safe. Occupied destination cells outside the source require
  `replace=True`. Air in the selection can clear destination blocks.
- Transforms are immediate and atomic. Duplication returns a new selection.
- Box selections capture entities inside their bounds. Block filters contain
  blocks only. Fill, replace, and patch do not modify free entities.


### Copy across regions

```python
fragment = area.copy()
tower = scene.add_region("tower", origin=(40, 0, 0))
tower.set((0, 0, 0), fragment)
```

Fragments are independent snapshots relative to the selection's minimum corner.
They include selected air and attached data. Filtered copies skip unselected
cells. `set` replaces destination cells; it does not remove unrelated free
entities. Cross-document fragments require matching editions and versions.

### Attached data

Advanced data uses typed SNBT so byte, short, integer, long, array, and floating
point tags remain distinct:

```python
region.block_entities.set(
    (6, 1, 4),
    '{id:"minecraft:chest",Items:[{Slot:0b,id:"minecraft:stone",count:64}]}',
)
print(region.block_entities.get((6, 1, 4)))

villager = region.entities.add(
    mob("villager", nbt="{NoAI:1b,Silent:1b}"),
    at=(4.5, 1.0, 8.5),
)
region.entities.update(villager, position=(5.5, 1.0, 8.5))
```

Entity references are local to the document. Copies get fresh references and
discard stored UUIDs. Linked entities are not fully supported. Updating `nbt`
replaces the full compound, which must contain `id`; omitting `nbt` keeps it.

Beds anchor at the foot, doors at the lower block, and mobs at their base
position. Helpers validate the complete placement before committing it.

### Game-rule validation

`scene.validate()` checks the schematic to make sure its correctly aligned with game rules.

```python
report = scene.validate()
print(report.errors)    # Structural errors, with region, position, and rule ID.
print(report.warnings)  # Blocks that can fall or states that need tick simulation.
print(report.unknown)   # Missing world context or unavailable game data.
```

`report.ok` requires no errors, export losses, or unknown results. 


## File support

| Format | Read/write coverage |
| --- | --- |
| `.schem` | Sponge v1/v2/v3 read; v3 write. Blocks, entities, block entities, offsets, metadata; retained v3 biomes. |
| `.litematic` | Versions 4–6 read; v6 write. Named regions, signed source bounds, states, entities, block entities, and retained scheduled ticks. |
| `.nbt`, `.snbt` | Java structure files. Single palette, entities, block entities, and sparse placement masks. Multiple palette variants are rejected. |
| `.schematic` | Classic MCEdit blocks through a runtime numeric mapping. Unmapped blocks require explicit replacements. Legacy entity NBT is retained without a game-version upgrade. |
| `.mcstructure` | Native Bedrock read/write, including retained palette tag types, secondary layers, position data, and entities. Layered authoring and Bedrock-to-Java mapping are not implemented. Java export supports a small verified-name mapping of plain blocks. |

A filename extension selects the codec. Pass `format=` to override it.
NBT here means a Java structure file, not an arbitrary Minecraft NBT document.

```python
scene = Schematic.load("input.litematic")
print(scene.regions)
report = scene.check_export(format="schem", flatten=True)
print(report.errors)  # Blocking problems, such as missing block mappings.
print(report.losses)  # Omissions that allow_loss can accept.

# Acknowledge reported region-boundary/metadata losses if appropriate.
scene.save("output.schem", flatten=True, allow_loss=True)
```

Exports reject unapproved losses. `allow_loss=True` allows reported omissions,
such as unsupported metadata or entity schemas. It never invents a missing block
mapping. Flattening multiple regions requires `flatten=True`; overlapping region
bounds remain an error, including overlaps involving air. Flattened structure
files preserve explicit air and omitted cells; gaps between regions are omitted.

For byte-based clients:

```python
data = scene.to_bytes(format="schem", flatten=True, allow_loss=True)
restored = Schematic.from_bytes(data, format="schem")
```

## Current limits

- Java authoring starts at 1.13. New versions need a matching upstream catalog.
  Automatic upgrades between Minecraft versions are not implemented. Bedrock
  remains a file read/write path; new Bedrock authoring is not implemented.
- Full preservation of every format-specific field is not guaranteed.
- Retained biomes, ticks, and Bedrock layers block transforms or resizing when
  their spatial mapping is unsupported. Copying them into fragments is rejected.
- Limits: 16,777,216 cells per bounded volume, 256 MiB input/expanded NBT, and
  65,536 cells per text layer inspection.
- Storage is a sparse map. Bulk operations run in Rust, but very large builds
  need future profiling and storage improvements.
  this version. Runtime download/cache support currently targets native platforms.

## Development

```text
src/model.rs                 Shared document, region, and selection data
src/edit.rs                  Atomic edits and entity operations
src/transform.rs             Block and entity transforms
src/helpers.rs               Typed placement recipes and versioned NBT
src/registry.rs              Catalog loading and parsed block schemas
src/validate.rs              Read-only schematic game-rule checks
src/formats/                 One codec module per schema; shared NBT/SNBT structure schema
src/mc_data/                Runtime catalogs, prepared visual assets, and source attribution
bindings/python/
  src/lib.rs                 PyO3 adapter
  python/schemora/            Public Python package
  Cargo.toml
  pyproject.toml
Cargo.toml                   Cargo workspace and core crate
pyproject.toml               uv workspace and Ruff configuration
uv.lock                      Python development dependency lock
```

Python source is editable after `uv sync`. After changing Rust code, rebuild:

```sh
uv sync --all-packages --reinstall-package schemora
```

Format and lint:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
uv run --all-packages ruff format bindings/python/python
uv run --all-packages ruff check bindings/python/python
```

To build a wheel:

```sh
uv run --all-packages maturin build --release --manifest-path bindings/python/Cargo.toml --out dist
```
