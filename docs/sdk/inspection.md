---
description: Inspect schematic blocks, inventories, entities, typed NBT, retained data, and combined resource reports.
---

# Inspection

Inspection reports describe the blocks and contents stored in a schematic, including
inventories and entities. Load the [workshop](../quickstart/developers.md) to inspect
its structure and the 64 stone in its chest.

## Blocks and inventories

```python
from mcschemora import Schematic

scene = Schematic.load("output/workshop.schem")
region = scene.region()
print(region.bounds)
print(region.get((4, 1, 4)))
resources = scene.inspect()
print(resources["blocks"])
print(resources["items"])
print(resources["inventories"])
```

Block counts describe the placed structure, while item counts also include stored
inventory contents. Inspection follows nested containers, equipment, and passengers.
Check the diagnostics and `complete` field to see whether it could interpret
everything that contributes to the totals.

For a block-state count in one area, call `region.select(...).counts()`. To read
non-air cells and their positions, use `region.get_all()`.

For construction quantities, recipe ingredients, and EMC/EssentialsX values, see
[materials and costs](materials-and-costs.md).

## Data managers

`region.entities` manages free entities such as mobs. `region.block_entities`
manages the [NBT](https://minecraft.wiki/w/NBT_format) attached to blocks such as chests. Both are separate from
`get_all()`, which returns blocks only. See
[Entities](../reference/python.md#entities) and
[BlockEntities](../reference/python.md#blockentities) for adding, updating,
or reading their typed SNBT.

## Completeness

```python
report = scene.inspect()
print(report["complete"], report["retained_data"])
for diagnostic in report["diagnostics"]:
    print(diagnostic["path"], diagnostic["message"])
print(report["identifiers"])
print(report["payload_fields"])
```

If `complete=False`, inspection could not interpret some content; keep the diagnostic
paths with any totals you export. The separate `retained_data` flag identifies
imported data that MCSchemora preserves without fully modeling. Inspection can
complete with this flag set, even though resizing, copying, or converting that data
can still fail.

The report also includes volume, data version, entity counts, moving-entity and
outside-bounds flags, and maximum stack counts. These fields describe the stored
schematic. Inspection does not execute commands, infer resources that commands could
create, or certify a downloaded build as safe.

## Typed NBT

```python
region = scene.region()
existing = region.block_entities.get((4, 1, 4))
print(existing)
region.block_entities.set(
    (4, 1, 4),
    '{id:"minecraft:chest",Items:[{Slot:0b,id:"minecraft:stone",count:64}]}',
)
```

Pass NBT as a typed SNBT string, keeping byte/short/long suffixes, lists, arrays,
and compound structure intact. The compound must include its ID and be compatible
with the owning block's block-entity type. Because `set` replaces the whole compound,
read the existing value first if you need to preserve fields. To remove the attached
NBT while keeping the block, call `remove(at)`.

The common `chest` and `sign` [placement helpers](schematics.md#placement-helpers)
handle version-specific data for normal authoring. Prefer those when a full
custom compound is unnecessary.

## Entity references

```python
from mcschemora import mob

reference = region.entities.add(mob("sheep"), at=(2.5, 1.0, 2.5))
snapshot = region.entities.get(reference)
region.entities.update(reference, position=(3.5, 1.0, 2.5))
print(snapshot.position, region.entities.get(reference).position)
region.entities.remove(reference)
```

An `Entity` is an immutable snapshot, identified within the current schematic by a
reference. These references are separate from Minecraft UUIDs and may change across
exports. Iteration also snapshots the reference list, so later edits leave an
existing iterator unchanged. To move an entity without replacing its NBT, use
`update(position=...)` with coordinates local to its region. `update(nbt=...)`
replaces the full compound, which must include the required ID.

`scene.metadata` is also a full typed SNBT compound. Assigning it replaces all
schematic metadata, and conversion may omit it. Preview data changes with a
fresh inspection and [export report](conversion.md#export-reports).

## Combined reports

`inspect_schematics` preserves individual reports in `placements` and sums their
resource counts. Counts use submitted IDs and include overlaps. Inputs retain their
coordinates and versions, and the call leaves their schematics separate.
`initial_items` adds existing inventory to stored item totals. Calculate remaining
construction materials separately. The combined `complete` flag is false if any input
inspection is incomplete.

To audit containers, use the inventory paths and item quantities. To estimate
construction, use [material_cost](materials-and-costs.md) instead. Browser WASM
does not expose the native inventory/entity inspection managers; check the
[binding support table](../reference/formats.md#language-bindings) before porting
a Python inspection workflow to the browser.

## Combine schematics

```python
from mcschemora import inspect_schematics

combined = inspect_schematics(
    [scene],
    initial_items=[("minecraft:stone", 64)],
)
print(combined["blocks"], combined["items"], combined["entities"])
print(combined["placements"][0]["inventories"])
```

Pass more loaded schematics in the list to combine their resource counts.
`initial_items` adds existing inventory to the combined item totals.
Inspection uses submitted IDs without converting versions. It does not infer
items that could be created by commands or other privileged payloads.

