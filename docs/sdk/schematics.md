---
description: Understand MCSchemora schematics, block states, named regions, coordinate systems, batch writes, and attached data.
---

# Schematics

A schematic stores blocks in named regions (such as how it is down in `.litematic` files). Each region has
its own coordinates, so you can build parts separately and position them within
the schematic. Use `set`
for individual blocks, selections for areas, and batches for generated geometry.

## Create a schematic

```python
from mcschemora import Schematic, bed, block, chest, door, item, sign

scene = Schematic.create(version="1.21.1")
region = scene.region()
region.select(start=(0, 0, 0), size=(7, 1, 7)).fill(block("stone_bricks"))
region.set((4, 1, 1), block("furnace", facing="west"))
```

`scene.region()` returns the default region, named `main`, and edits through this
handle update the schematic. Positions are `(x, y, z)`, with Y pointing up; negative
coordinates are allowed. A selection starts at the inclusive `start` and extends
for `size` cells, so this floor covers X and Z from 0 through 6 at Y=0.

## Block states

`block("stone_bricks")` expands the name to `minecraft:stone_bricks`. Pass block
states as keyword arguments, such as `facing="west"` or `axis="y"`. Writes are
checked against the schematic's Minecraft version.

Look up a block's supported properties before choosing a value:

```python
print(scene.registry.describe("furnace"))
print(region.get((4, 1, 1)))
```

The registry description includes the ID, properties, version, and numeric ID.
When you read an empty cell, you get air.

## Batch writes

Continue with the same region:

```python
region.select(start=(0, 1, 0), size=(1, 3, 1)).fill(block("oak_log", axis="y"))
region.set_many({
    (4, 1, 2): block("crafting_table"),
    (6, 1, 0): block("oak_log", axis="y"),
})
```

The fill creates a three-block pillar, while `set_many` places the crafting table
and second log together. It accepts a mapping or an iterable of `(position, block)`
pairs and validates the entire batch before writing. If any input is invalid, the
batch stays unwritten; if a coordinate appears more than once, the last entry wins.
An iterable works well for procedural shapes such as the [rainbow reef](../examples/reef.md).

## Placement helpers

Placement helpers handle paired blocks and attached inventory or sign data:

```python
region.place(bed(color="red", head_toward="north"), at=(1, 1, 4))
region.place(door(material="oak", facing="north"), at=(3, 1, 0))
region.place(chest(items={0: item("stone", count=64)}), at=(4, 1, 4))
region.place(sign(["Workshop", "Tools inside"]), at=(0, 1, 3))
```

The anchor identifies the bed's foot, the door's lower block, or the sign or chest
position. These helpers reject occupied destinations unless you pass `replace=True`.
For the chest, the mapping keys are inventory slot numbers.

## Region coordinates

Block operations use coordinates local to their region. A region's origin sets
where those coordinates sit in the schematic:

```python
tower = scene.add_region("tower", origin=(40, 0, 0))
tower.set((2, 1, 4), block("stone"))
print(tower.to_global((2, 1, 4)))
print(tower.to_local((42, 1, 4)))
```

The first conversion prints `(42, 1, 4)`; the second prints `(2, 1, 4)`.
Rendering filters use global coordinates. Save as `.litematic` when you want to
keep multiple regions; see [load, save, and convert](conversion.md).

The [developer quickstart](../quickstart/developers.md) puts these operations
together in a complete workshop. See [Region](../reference/python.md#region)
and [placement helpers](../reference/python.md#function-bed) for signatures.

## Bounds and air

New regions have no bounds until blocks are written. Handle `region.bounds is
None` before selecting a whole region. Bounds have an inclusive `start` and a
`size` in cells; the exclusive end is `start + size`. A selection includes air
within its selected cells even though `get_all()` and `counts()` omit air.

A rectangular fragment can clear destination cells wherever it contains selected air.
A sparse selection copies only its selected cells. Deleting selected cells also
removes attached data and selected entities; filling blocks leaves free entities
unchanged. See [selections and transformations](editing.md).

## Generated geometry

```python
wall = block("stone_bricks")
region.set_many(
    ((x, y, z), wall)
    for x in range(7)
    for y in range(1, 4)
    for z in range(7)
    if x in (0, 6) or z in (0, 6)
)
```

This adds walls to the current scene. Start with a fresh scene if you want to keep
the workshop unchanged for later examples. Immutable `Block` values can be reused
across cells, and a generator avoids creating an extra Python list. `set_many` still
validates and stores the whole batch before writing, so each batch must fit in memory.
