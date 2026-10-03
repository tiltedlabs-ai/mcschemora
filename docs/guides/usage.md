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
