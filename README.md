# Schemora

Author and edit Minecraft schematics in code.

## Get started

Install [uv](https://docs.astral.sh/uv/) and a current stable
[Rust toolchain](https://rustup.rs/). From this directory:

```sh
git submodule update --init --depth 1
uv sync --all-packages
uv run --all-packages python examples/build.py
```

uv creates `.venv` and builds the Python extension. The example writes files into
`examples/output/`. The Python package has no Python runtime dependencies.
It reads catalogs from the initialized submodule. An installed wheel needs a
`minecraft-data` checkout too; pass its path or set `SCHEMORA_DATA`.

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

This demonstrates stored blocks and states. It does not simulate water or redstone.

## Minecraft versions and data

Authoring supports Java **1.13 and later**, when a catalog is present in
[minecraft-data](https://github.com/PrismarineJS/minecraft-data). Block properties,
defaults, items, entity IDs, and file `DataVersion` come from the selected version.
The reader follows upstream `dataPaths.json`, including paths shared by releases.
It does not substitute a nearby version when a catalog is missing.

```python
from schemora import MinecraftData, Schematic, block

data = MinecraftData()  # Repository submodule, or SCHEMORA_DATA.
print(data.versions)
scene = Schematic.create(version="1.20.4", data=data)
scene.region().set((0, 0, 0), block("lever", powered=True))
print(scene.registry.describe("lever"))
```

`MinecraftData("/path/to/minecraft-data")` accepts an existing checkout or its
`data/` directory. For a separate installation:

```sh
git clone --depth 1 https://github.com/PrismarineJS/minecraft-data.git /path/to/minecraft-data
export SCHEMORA_DATA=/path/to/minecraft-data
```

The `data=` argument also works on `Schematic.load` and `Schematic.from_bytes`.
Catalogs load on demand and are cached within the `MinecraftData` object. Multiple
versions can be used in one process. `version="latest"` (the default) selects the
latest release with an available catalog. Set a version for reproducible builds.

The submodule records an upstream revision, not a Minecraft version restriction.
Update the available catalogs with `make data-update`; no Rust rebuild is needed.
Create a new `MinecraftData()` after an update, or restart Python, to read the new
snapshot. No network requests occur during import, authoring, or file conversion.

Imported Java files select their catalog through `DataVersion`. Files with
unknown version metadata retain their data but cannot use validated edits until
a matching catalog is available. Files explicitly older than 1.13 are rejected.
Classic `.schematic` files have no game-version tag: their numeric block mappings
are read into the Java 1.13 registry. This does not upgrade entity NBT.

Helpers use the document's catalog and reject blocks, items, and mobs absent from
that version. Signs handle the old text fields, two-sided text, and NBT text
components. Chest items use `Count` before 1.20.5 and `count`/`components` afterward.
Item `components=` is rejected on older versions; raw typed NBT remains available.
The upstream catalogs do not define every saved entity or block-entity NBT schema;
new game changes to those schemas may still need helper updates.

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

## File support

| Format | Read/write coverage |
| --- | --- |
| `.schem` | Sponge v1/v2/v3 read; v3 write. Blocks, entities, block entities, offsets, metadata; retained v3 biomes. |
| `.litematic` | Versions 4–6 read; v6 write. Named regions, signed source bounds, states, entities, block entities, and retained scheduled ticks. |
| `.nbt`, `.snbt` | Java structure files. Single palette, entities, block entities, and sparse placement masks. Multiple palette variants are rejected. |
| `.schematic` | Classic MCEdit blocks through a bundled numeric mapping. Unmapped blocks require explicit replacements. Legacy entity NBT is retained without a game-version upgrade. |
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
- Validation checks state schemas and incomplete beds/doors. It is not a complete
  support-block checker or a Minecraft simulator.
- Limits: 16,777,216 cells per bounded volume, 256 MiB input/expanded NBT, and
  65,536 cells per text layer inspection.
- Storage is a sparse map. Bulk operations run in Rust, but very large builds
  need future profiling and storage improvements.
- Renderer, live server connection, undo history, and WASM bindings are outside
  this version. The Rust core itself compiles for WebAssembly.

## Development

```text
src/model.rs                 Shared document, region, and selection data
src/edit.rs                  Atomic edits and entity operations
src/transform.rs             Block and entity transforms
src/helpers.rs               Typed placement recipes and versioned NBT
src/registry.rs              Catalog loading and parsed block schemas
src/formats/                 One codec module per schema; shared NBT/SNBT structure schema
data/minecraft-data/         Upstream data submodule, read at runtime
data/SOURCE.md               Data setup, updates, and attribution
bindings/python/
  src/lib.rs                 PyO3 adapter
  python/schemora/            Public Python package
  Cargo.toml
  pyproject.toml
examples/                    Runnable builds and file inspection
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
uv run --all-packages ruff format bindings/python/python examples
uv run --all-packages ruff check bindings/python/python examples
```

Example-based verification, without a unit-test suite:

```sh
uv run --all-packages python examples/build.py
uv run --all-packages python examples/inspect_files.py
uv run --all-packages python examples/check_edits.py
```

The last script checks atomic edits, entity copies, helper version schemas, sparse
flattening, and retained Bedrock data. The second script uses independent nbtlib and Litemapy readers and checks
block-state round-trips. These checks do not substitute for importing the
files into a running Minecraft/WorldEdit installation.

To check every state in every advertised Java catalog (takes several minutes):

```sh
uv run --all-packages python examples/check_catalogs.py
```

Pass version names to check specific catalogs, or `--transforms-only` to skip
file round-trips. The script checks schemas, defaults, placement, invalid-state
rejection, four Java file formats, Y rotation, and X/Z reflections.

To build a wheel:

```sh
uv run --all-packages maturin build --release --manifest-path bindings/python/Cargo.toml --out dist
```

To check the core for the web target:

```sh
rustup target add wasm32-unknown-unknown
cargo check -p schemora --target wasm32-unknown-unknown
```

## Data sources

Block, item, entity, and legacy mappings come from
[PrismarineJS/minecraft-data](https://github.com/PrismarineJS/minecraft-data).
See [data/SOURCE.md](data/SOURCE.md) for setup and upstream licensing. NBT codecs use fastnbt, fastsnbt, and a small Bedrock
little-endian adapter.
