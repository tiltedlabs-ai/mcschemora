# Editing, conversion, and rendering

Install through the [Python quickstart](../../bindings/python/README.md). The snippets
below share one schematic; full scripts and their outputs are in the [example gallery](../../examples/README.md).
Method arguments and behavior are defined in the generated [API reference](../reference/api.md).

## Build and edit

```python
from mcschemora import Schematic, bed, block, chest, item, mob

scene = Schematic.create(version="1.21.1")
region = scene.region()
region.select(start=(0, 0, 0), size=(7, 1, 7)).fill(block("stone_bricks"))
region.place(bed(color="red", head_toward="north"), at=(2, 1, 4))
region.place(chest(items={0: item("stone", count=64)}), at=(5, 1, 4))

area = region.select(start=(0, 0, 0), size=(7, 3, 7))
copy = area.duplicate(offset=(10, 0, 0))
copy.rotate(steps=1)
copy.select(block="stone_bricks").fill(block("mossy_stone_bricks"))
print(copy.describe_layer(y=1))
print(scene.validate())
scene.save("workshops.schem")
```

Use a block for a cell, a placement helper for a structure such as a bed, and a
selection for bulk editing. Inspect version-specific states with
`scene.registry.describe("lever")`. See [Region](../reference/api.md#region),
[Selection](../reference/api.md#selection), and [helpers](../reference/api.md#function-bed).

To transfer content between regions, copy a selection to a fragment:

```python
tower = scene.add_region("tower", origin=(40, 0, 0))
tower.set((0, 0, 0), area.copy())
```

[Entity managers](../reference/api.md#entities) handle mobs and
[block-entity managers](../reference/api.md#blockentities) handle attached typed SNBT:

```python
villager = region.entities.add(mob("villager"), at=(4.5, 1.0, 4.5))
region.entities.update(villager, position=(4.5, 1.0, 2.5))
print(region.block_entities.get((5, 1, 4)))
```

## Inspect and convert

```python
loaded = Schematic.load("workshops.schem")
report = loaded.check_export(format="litematic")
print(report)
if not report.errors and not report.losses:
    loaded.save("workshops.litematic")
```

Inspect the report before accepting any losses. Replace unsupported content when
errors block conversion; `allow_loss=True` only accepts reported omissions.
For multiple regions, evaluate `flatten=True` when the destination needs one region.
The [conversion script](../../examples/convert.py) exposes these choices as CLI options.
See [check_export](../reference/api.md#schematiccheck_export) and
[save](../reference/api.md#schematicsave) for the contract.

## Render and crop

```python
diagnostics = scene.export_png("build.png", size=(800, 600), y=(0, 2))
scene.export_png("layer.png", view="top", grid=True, y=1)
scene.export_glb("build.glb")
print(diagnostics)
```

Use PNG for an image and GLB for a model. Cropping can reveal interiors or isolate
layers; render filters use world coordinates, including region origins.
First use prepares visual assets; see [offline preparation](../reference/minecraft-data.md#cache-and-offline-use).

## Sprite diagrams and wiki blueprints

```python
print(scene.export_sprites("sprites.png", view="top", y=1, grid=True))
print(scene.export_blueprint("build.wiki", name="Workshops", y=(0, 2)))
```

Sprite diagrams use bundled wiki icons. Blueprints are visual plans, so inspect
diagnostics for omitted schematic data. See the [rendered example outputs](../../examples/README.md#render-and-export-a-blueprint).

Import a supported wiki template with a fixed Java version and explicit blocks for
ambiguous palette symbols:

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

Choose symbols from the input file. Blueprint imports infer some defaults and do not
provide a lossless schematic round trip; see [from_bytes](../reference/api.md#schematicfrom_bytes).
