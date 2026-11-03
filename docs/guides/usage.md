# Editing, conversion, and rendering

Install through the [Python quickstart](../../bindings/python/README.md). The snippets
below share one schematic; full scripts and their outputs are in the [example gallery](../../examples/README.md).
Method arguments and behavior are defined in the generated [API reference](../reference/api.md).

## Build and edit

All regions have their own relative coordinate system.

```python
from mcschemora import Schematic, bed, block, chest, door, item, mob, sign

# Create a Java schematic and get its default region.
scene = Schematic.create(version="1.21.1")
region = scene.region("main")

# Fill a seven-by-seven floor, then place a block with explicit states.
region.select(start=(0, 0, 0), size=(7, 1, 7)).fill(block("stone_bricks"))
region.set((4, 1, 1), block("furnace", facing="west"))
```

```python
# Bulk write blocks
region.set_many({
    (4, 1, 2): block("crafting_table"),
    (0, 1, 0): block("oak_log", axis="y"),
})

region.set_many([
    ((6, 1, 0), block("stone")),
    ((6, 1, 0), block("oak_log", axis="y")),
])

region.set_many(((x, 1, 6), block("oak_planks")) for x in range(7))
```

```python
# place special blocks
region.place(bed(color="red", head_toward="north"), at=(2, 1, 4))
region.place(door(material="oak", facing="north"), at=(3, 1, 0))
region.place(chest(items={0: item("stone", count=64)}), at=(5, 1, 4))
region.place(sign(["Workshop", "Tools inside"]), at=(0, 1, 3))

# inspect block properties
furnace = region.get((4, 1, 1))
print(furnace.id, dict(furnace.states))
print(scene.registry.describe("furnace"))

# inspect region properties
print(region.bounds, region.origin)
print(scene.edition, scene.version, scene.metadata, scene.regions)
```


```python
# Duplicate the selected structure, rotate it, then mirror and move the copy.
area = region.select(start=(0, 0, 0), size=(7, 3, 7))
copy = area.duplicate(offset=(10, 0, 0))
copy.rotate(axis="y", steps=1)
copy.flip(axis="x")
copy.move(offset=(0, 0, 10))

# Copy a fragment into another region, anchored at its local (0, 0, 0).
tower = scene.add_region("tower", origin=(40, 0, 0))
tower.set((0, 0, 0), area.copy())
print(scene.region("tower").get((0, 0, 0)).id)

# The region origin converts local coordinates to world coordinates.
local = (2, 1, 4)
world = tower.to_global(local)
print(world)  # (42, 1, 4).
print(tower.to_local(world))  # (2, 1, 4).
```

```python
# Check game rules, then save a litematic that keeps the named regions.
print(scene.validate())
repairs = scene.repair()
print(repairs.changed, repairs.skipped)
scene.save("workshops.litematic")

# Review the losses from merging regions, then accept them for this schem export.
print(scene.check_export(format="schem", flatten=True))
scene.save("workshops.schem", flatten=True, allow_loss=True)

# Reopen the file and inspect the blocks from disk.
reopened = Schematic.load("workshops.schem")
print(reopened.region().get_all())
```

See [Region](../reference/api.md#region), [Selection](../reference/api.md#selection),
[helpers](../reference/api.md#function-bed), [entities](../reference/api.md#entities),
and [block entities](../reference/api.md#blockentities) for the full signatures.

## Normalize materials

Normalization applies inline rules, optionally extending a bundled profile or
JSON file, to every region in place:

```python
report = scene.normalize(
    keep=["glass", "dirt", "hopper"],
    replace={"spruce_planks": "oak_planks", "birch_log": "oak_log"},
    fallback="iron_block",
)
```

Omit `fallback` to leave unlisted blocks unchanged:

```python
scene.normalize(replace={"red_wool": "white_wool"})
```

The bundled Java `redstone` profile standardizes general building blocks to gray
concrete, ordinary slabs to smooth stone slabs, ordinary stairs to stone brick stairs,
planks to oak planks, stripped logs and wood to stripped oak equivalents, and wool
slabs and stairs to white.
Unstripped logs, wood and woody stems become oak logs, leaves become oak leaves,
and all current stone walls become stone brick walls.
Use `keep` for farms that depend on specific materials.

```python
report = scene.normalize("redstone")
```

Customize a profile with the same keyword arguments:

```python
report = scene.normalize(
    "redstone",
    keep=["spruce_log", "spruce_leaves"],
    replace={"red_wool": "blue_wool"},
)
```

Edit the explicit lists in [redstone.json](../../data/profiles/java/redstone.json).

For a custom profile, save this as `materials.json`:

```json
{
  "edition": "java",
  "replacements": [
    {
      "from": ["minecraft:granite", "minecraft:diorite"],
      "to": "minecraft:stone"
    },
    {
      "from": ["minecraft:red_stained_glass"],
      "to": "minecraft:glass"
    }
  ]
}
```

```python
report = scene.normalize("materials.json")
print(report["changed"])
print(report["replacements"])
scene.save("normalized.litematic")
```

The report contains the total `changed` count and `replacements` records with
`from`, `to`, and `count`, sorted by source block ID. Entities and metadata remain
unchanged. Only explicitly present cells are replaced.

To keep selected materials and replace everything else eligible with one block:

```json
{
  "edition": "java",
  "keep": ["minecraft:glass", "minecraft:red_stained_glass"],
  "fallback": "minecraft:stone"
}
```

`keep` takes priority over explicit `replacements`. The optional `fallback`
applies to remaining eligible blocks. Without it, unlisted blocks stay unchanged.
Replacement targets and the fallback block remain canonical, so they are not
replaced by the fallback. `keep` and `replacements` default to empty lists.

Profiles are version independent and must match the schematic's edition. Block
IDs are checked against the schematic's own catalog. Unavailable or invalid
sources are skipped; an unavailable or invalid target skips its whole group.
Only non-air blocks can be replaced. The original block's complete state is
resolved, then properties supported by the target are copied unchanged. Unsupported
properties are discarded, and properties present only on the target use its
defaults. Shared property values must be valid for the target or the replacement
is skipped. For example, converting a bone block to iron discards `axis`, while
converting a log to another log preserves it.
For example, slab replacements keep top/bottom/double and waterlogging, and stair
replacements keep facing, half, shape, and waterlogging.

Both blocks must use the same block-entity type. Compatible attached data is
preserved unchanged, including sign text. An attached-data ID must match when
present; if omitted, the known owning block-entity type determines compatibility.
Invalid attached data and positions outside bounds are skipped. Skipped
replacements are not reported.

The first valid mapping for each source wins. Mappings targeting another source
that maps to a different block are skipped, preventing chains and cycles.
Rules apply once to the original blocks, and applying the profile again changes
nothing. Malformed JSON, unknown fields, edition mismatches, and unavailable
schematic catalogs still fail before any changes are committed.

The profile author decides which materials are interchangeable. Normalization
does not simulate redstone or establish that a farm still works.

Rust callers pass an optional profile and `normalize::Options`:

```rust
let options = mcschemora::normalize::Options {
    keep: vec!["spruce_log".into()],
    replace: [("red_wool".into(), "blue_wool".into())].into(),
    fallback: None,
};
schematic.normalize(None, &options)?;
```

Browser callers pass an optional profile and an options object:

```javascript
schematic.normalize(undefined, { replace: { red_wool: "white_wool" } });
schematic.normalize("redstone", {
  keep: ["spruce_log"],
  replace: { red_wool: "blue_wool" },
});
```


## Material costs

```python
cost = scene.material_cost()
print(cost["items"])
print(cost["raw_materials"])
```

The Rust implementation compiles recipe expansions once per catalog version and
shares the resulting item-to-primitive table between schematics using the same
`MinecraftData`. Quantities are fractional: five oak planks cost 1.25 oak logs,
without rounding crafting batches or charging for surplus. The schematic is not
modified. Normalize first when comparing standardized builds:

```python
scene.normalize("redstone")
cost = scene.material_cost()
```

The [default primitive policy](../../data/cost/java/primitives.json) defines where
expansion stops: logs, minerals and ingots, natural blocks, drops, and plants.
Any block or item without a recipe expansion becomes a primitive worth
one unit of itself, including missing recipes and recipe cycles.
Unavailable primitive IDs are ignored for older versions. Shaped crafting recipes
precede other recipes; candidates otherwise retain catalog order. The first
acyclic recipe whose ingredients resolve to declared primitives is used. Costs
are deterministic for a version, not a search for the cheapest acquisition path.
One-to-one acquisition steps cover stripping logs, hardening concrete, drying
coral, copper oxidation, carving pumpkins, and anvil damage.

Water and lava count only source blocks (`level=0`); flowing and falling fluids
are excluded from both `items` and `raw_materials`.

Block states are tallied as placement items. Door halves, bed parts, and tall
plants count once per pair, with missing partners counted as one item. Partners
can span regions. Double slabs count twice, and stacked candles, turtle eggs,
snow layers, sea pickles, and flower patches use their item quantities. Wall
variants use the corresponding inventory item. Potted plants require a flower
pot and plant; candle cakes require a cake and candle. Farmland and paths require
dirt; crops require their planting items. Counts sum stored regions, so overlapping
regions contribute independently.

Reports contain `items` and `raw_materials`. The item counts include primitive
blocks without inventory items. Inventories, entities, fuel, tool
wear, waterlogging, and fluid acquisition are outside this material estimate.
Special/component-dependent recipes are not expanded. Filled-container primitives,
such as milk buckets and honey bottles, count gross inputs; returned empty
containers are not credited. Blocks without placement items count as primitives. Recipe
availability varies by version; missing or malformed recipe data raises an error.

Rust callers use `schematic.material_cost().await?`. Browser callers use
`await schematic.materialCost()` and receive the same report fields. Native
offline use requires catalog metadata and any upstream block/item catalogs to
have been cached. A shared official recipe tracker covers every supported version.

## Material values

Apply a fixed unit-value profile to the build's primitive quantities:

```python
value = scene.value_cost("emc")
print(value["total"])
print(value["unpriced"])

server_value = scene.value_cost("essentialsx")
```

`total` is the sum of primitive quantity times unit price. Missing prices
contribute nothing and appear in `unpriced` with their fractional quantities.
A configured zero price is priced. No normalization or editing happens; call
`scene.normalize("redstone")` first when comparing standardized builds.

The bundled `emc` profile adapts ProjectE's values and conversions for vanilla
Java 1.21.1. `essentialsx` uses matching scalar entries from the official default
server sell-price table. Both are fixed across Minecraft versions; recipe
expansion still uses the build's version. EssentialsX's legacy defaults have
limited coverage, including no scalar prices for modern log or wool IDs.
See the [profile sources and coverage](../../data/cost/java/values/SOURCE.md).

Compare builds using the same profile and check unpriced quantities alongside
the total. These scores do not measure construction time or gathering difficulty.
Unknown profiles, invalid prices, and overflowing totals raise an error.

Rust callers use `schematic.value_cost("emc").await?`; browser callers use
`await schematic.valueCost("emc")`. Both return `total` and `unpriced`.

## Inspect and convert


```python
loaded = Schematic.load("build-1.20.4.litematic")
report = loaded.check_export(format="litematic", version="1.20.5")
print(report)
loaded.save("build-1.20.5.litematic", version="1.20.5")
```

```python
loaded = Schematic.load("workshops.schem")
report = loaded.check_export(format="litematic")
print(report)
if not report.errors and not report.losses:
    loaded.save("workshops.litematic")
```

Use `flatten=True` to merge all regions into one.
See [check_export](../reference/api.md#schematiccheck_export) and
[save](../reference/api.md#schematicsave) for more information.

## Render and crop

```python
diagnostics = scene.export_png("build.png", size=(800, 600), y=(0, 2))
scene.export_png("layer.png", view="top", grid=True, y=1)
scene.export_glb("build.glb")
print(diagnostics)
```

## Sprite diagrams and wiki blueprints

```python
print(scene.export_sprites("sprites.png", view="top", y=1, grid=True))
print(scene.export_blueprint("build.wiki", name="Workshops", y=(0, 2)))
```

Sprite diagrams use wiki icons to render wiki-style blueprints. See the [rendered example outputs](../../examples/README.md#render-and-export-a-blueprint).


Importing a wiki blueprint

```python
template = "{{layered blueprint|A=BlockSprite:stone|B=BlockSprite:chest|----Floor|A|----Chest|B}}"
imported = Schematic.from_bytes(
    template.encode(),
    format="blueprint",
    version="1.21.1",
    palette={"B": block("chest", facing="north")},
)
print(imported.import_diagnostics)
```

Choose symbols from the input file. Blueprint imports infer some defaults due to limitations in blueprint format. See [from_bytes](../reference/api.md#schematicfrom_bytes).
